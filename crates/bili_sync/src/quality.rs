//! Manual, bounded quality checks and verified replacement jobs.
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use anyhow::{Context, Result, ensure};
use bili_sync_entity::{page, video};
use parking_lot::Mutex;
use sea_orm::EntityTrait;
use serde::{Deserialize, Serialize};

use crate::bilibili::{BestStream, BiliClient, PageInfo, Stream, Video, VideoQuality};
use crate::config::{CONFIG_DIR, Config, VersionedConfig};
use crate::library::{self, FileReceipt, SavedQuality};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Comparison {
    pub checked_at: String,
    pub current: SavedQuality,
    pub candidate: SavedQuality,
    pub upgradeable: bool,
    pub message: String,
}
#[derive(Deserialize)]
pub struct BatchRequest {
    pub page_ids: Vec<i32>,
    pub action: String,
}
#[derive(Clone, Default, Serialize)]
pub struct JobState {
    pub running: bool,
    pub total: usize,
    pub completed: usize,
    pub results: Vec<JobResult>,
}
#[derive(Clone, Serialize)]
pub struct JobResult {
    pub page_id: i32,
    pub success: bool,
    pub message: String,
}
static JOB: LazyLock<Mutex<JobState>> = LazyLock::new(|| Mutex::new(JobState::default()));
pub fn status() -> JobState {
    JOB.lock().clone()
}
fn comparison_path(id: i32) -> PathBuf {
    CONFIG_DIR.join("quality").join(format!("{id}.json"))
}
pub async fn load_comparison(id: i32) -> Result<Option<Comparison>> {
    match tokio::fs::read(comparison_path(id)).await {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn fps(value: &Option<String>) -> Option<f64> {
    let value = value.as_ref()?;
    if let Some((a, b)) = value.split_once('/') {
        let b: f64 = b.parse().ok()?;
        if b == 0.0 {
            None
        } else {
            Some(a.parse::<f64>().ok()? / b)
        }
    } else {
        value.parse().ok()
    }
}
/// QN alone is insufficient: HDR, codecs and frame rate are not a linear quality scale.
fn improves(old: &SavedQuality, new: &SavedQuality) -> bool {
    let (Some(ow), Some(oh), Some(nw), Some(nh)) = (old.width, old.height, new.width, new.height) else {
        return false;
    };
    let old_short = ow.min(oh);
    let new_short = nw.min(nh);
    if new_short > old_short {
        return true;
    }
    if nw != ow || nh != oh || old.codec.is_none() || old.codec != new.codec {
        return false;
    }
    let (Some(of), Some(nf)) = (fps(&old.frame_rate), fps(&new.frame_rate)) else {
        return false;
    };
    if nf > of + 5.0 {
        return true;
    }
    nf + 0.1 >= of
        && old
            .bitrate
            .zip(new.bitrate)
            .is_some_and(|(o, n)| o > 0 && n as f64 > o as f64 * 1.10)
}
fn verified(old: &SavedQuality, new: &SavedQuality) -> bool {
    let duration_ok = old
        .duration
        .zip(new.duration)
        .is_some_and(|(o, n)| o > 0.0 && (o - n).abs() <= (o * 0.01).max(3.0));
    duration_ok && improves(old, new)
}

pub async fn start(db: sea_orm::DatabaseConnection, client: Arc<BiliClient>, mut request: BatchRequest) -> Result<()> {
    ensure!(
        matches!(request.action.as_str(), "check" | "upgrade" | "strm"),
        "未知画质操作"
    );
    request.page_ids.sort_unstable();
    request.page_ids.dedup();
    ensure!(
        !request.page_ids.is_empty() && request.page_ids.len() <= 25,
        "每批请选择 1–25 个视频分 P"
    );
    let guard = crate::task::DownloadTaskManager::get().try_library_lock()?;
    {
        let mut job = JOB.lock();
        ensure!(!job.running, "上一批任务尚未完成");
        *job = JobState {
            running: true,
            total: request.page_ids.len(),
            ..Default::default()
        };
    }
    tokio::spawn(async move {
        let _guard = guard;
        let config = VersionedConfig::get().snapshot();
        for id in request.page_ids {
            let result = process(&db, &client, &config, id, &request.action).await;
            let risk_control = result
                .as_ref()
                .err()
                .and_then(|error| error.downcast_ref::<crate::bilibili::BiliError>())
                .is_some_and(|error| error.is_risk_control_related());
            {
                let mut job = JOB.lock();
                job.completed += 1;
                job.results.push(JobResult {
                    page_id: id,
                    success: result.is_ok(),
                    message: result.unwrap_or_else(|e| format!("{e:#}")),
                });
            }
            if risk_control {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        JOB.lock().running = false;
    });
    Ok(())
}

async fn baseline(video: &video::Model, page: &page::Model, config: &Config) -> Result<FileReceipt> {
    if let Some(mut receipt) = library::load(video.id, page.cid).await? {
        if receipt.quality.width.is_none() || receipt.quality.duration.is_none() {
            let target = if receipt.cloud {
                let cd2 = crate::cd2::Cd2Client::connection(config)?.context("CD2 未配置")?;
                cd2.download_url(&receipt.storage_path).await?.to_string()
            } else {
                receipt.storage_path.clone()
            };
            receipt.quality = library::probe(Path::new(&target), receipt.quality.clone()).await?;
            library::save(&receipt).await?;
        }
        return Ok(receipt);
    }
    let metadata = PathBuf::from(page.path.as_ref().context("尚未保存视频路径")?);
    let (storage_path, cloud, bytes, quality) = if metadata.is_file() {
        (
            metadata.to_string_lossy().into_owned(),
            false,
            tokio::fs::metadata(&metadata).await?.len(),
            library::probe(&metadata, SavedQuality::default()).await?,
        )
    } else if let Some(cd2) = crate::cd2::Cd2Client::configured(config)? {
        // Explicit manual checks inspect only the selected file, never a recursive directory scan.
        let path = cd2.remote_path(&metadata)?;
        let bytes = cd2.existing_size(&path).await?;
        let url = cd2.download_url(&path).await?;
        let quality = library::probe(Path::new(url.as_str()), SavedQuality::default()).await?;
        (path, true, bytes, quality)
    } else {
        let local = crate::storage::StorageLayout::video_path_for(&metadata)?;
        ensure!(local.is_file(), "找不到已保存视频，无法确认实际画质");
        (
            local.to_string_lossy().into_owned(),
            false,
            tokio::fs::metadata(&local).await?.len(),
            library::probe(&local, SavedQuality::default()).await?,
        )
    };
    let receipt = FileReceipt {
        video_id: video.id,
        cid: page.cid,
        storage_path,
        cloud,
        cloud_file_id: None,
        bytes,
        metadata_path: metadata,
        quality,
        uploaded_at: String::new(),
        playback_token: uuid::Uuid::new_v4().simple().to_string(),
    };
    library::save(&receipt).await?;
    Ok(receipt)
}

async fn candidate(
    client: &BiliClient,
    config: &Config,
    video: &video::Model,
    page: &page::Model,
) -> Result<(BestStream, SavedQuality)> {
    let bili = Video::new(client, &video.bvid, &config.credential);
    let info = PageInfo {
        cid: page.cid,
        duration: page.duration,
        ..Default::default()
    };
    let mut analyzer = bili.get_page_analyzer(&info).await?;
    let details = analyzer.info.clone();
    let mut filter = config.filter_option.clone();
    filter.video_max_quality = VideoQuality::Quality8k;
    let streams = analyzer.best_stream(&filter)?;
    let mut quality = SavedQuality::default();
    if let BestStream::VideoAudio {
        video: Stream::DashVideo { quality: qn, url, .. },
        ..
    } = &streams
    {
        quality.qn = Some(qn.clone() as u32);
        if let Some(stream) = details["dash"]["video"]
            .as_array()
            .and_then(|list| list.iter().find(|item| item["baseUrl"].as_str() == Some(url.as_str())))
        {
            quality.width = stream["width"].as_u64().map(|n| n as u32);
            quality.height = stream["height"].as_u64().map(|n| n as u32);
            quality.bitrate = stream["bandwidth"].as_u64();
            quality.frame_rate = stream["frameRate"].as_str().map(str::to_owned);
            quality.codec = stream["codecid"].as_u64().and_then(|n| match n {
                7 => Some("h264".into()),
                12 => Some("hevc".into()),
                13 => Some("av1".into()),
                _ => None,
            });
        }
    }
    quality.duration = Some(page.duration as f64);
    Ok((streams, quality))
}

async fn process(
    db: &sea_orm::DatabaseConnection,
    client: &BiliClient,
    config: &Config,
    id: i32,
    action: &str,
) -> Result<String> {
    let page = page::Entity::find_by_id(id).one(db).await?.context("分 P 不存在")?;
    let video = video::Entity::find_by_id(page.video_id)
        .one(db)
        .await?
        .context("视频不存在")?;
    ensure!(((page.download_status >> 3) & 7) == 7, "视频尚未下载完成");
    let old = baseline(&video, &page, config).await?;
    if action == "strm" {
        ensure!(
            old.cloud && !config.strm_base_url.is_empty(),
            "请先启用 STRM，且视频需要有云端上传记录"
        );
        library::write_strm(&old, config).await?;
        return Ok("STRM 已补写".into());
    }
    let (streams, proposed) = candidate(client, config, &video, &page).await?;
    let mut comparison = Comparison {
        checked_at: chrono::Utc::now().to_rfc3339(),
        current: old.quality.clone(),
        candidate: proposed.clone(),
        upgradeable: improves(&old.quality, &proposed),
        message: String::new(),
    };
    comparison.message = if comparison.upgradeable {
        "可升级；升级时将再次验证实际文件"
    } else {
        "未找到可确认的提升，保留当前版本"
    }
    .into();
    tokio::fs::create_dir_all(CONFIG_DIR.join("quality")).await?;
    library::atomic_write(&comparison_path(id), &serde_json::to_vec(&comparison)?).await?;
    if action == "check" {
        return Ok(comparison.message);
    }
    ensure!(comparison.upgradeable, "当前账号可获取的视频流没有可确认的画质提升");
    let downloader = crate::downloader::Downloader::new(client.client.clone());
    let limit = &config.concurrent_limit.download;
    let temporary = match streams {
        BestStream::Mixed(stream)
        | BestStream::VideoAudio {
            video: stream,
            audio: None,
        } => {
            downloader
                .multi_fetch_to_temp(&stream.urls(config.cdn_sorting), limit)
                .await?
        }
        BestStream::VideoAudio {
            video,
            audio: Some(audio),
        } => {
            downloader
                .multi_fetch_and_merge_to_temp(&video.urls(config.cdn_sorting), &audio.urls(config.cdn_sorting), limit)
                .await?
        }
    };
    let result = async {
        let actual = library::probe(temporary.file_path(), proposed).await?;
        ensure!(
            verified(&old.quality, &actual),
            "候选文件的时长或画质未通过验证，保留原视频"
        );
        let mut new = old.clone();
        new.quality = actual;
        new.bytes = tokio::fs::metadata(temporary.file_path()).await?.len();
        new.uploaded_at = chrono::Utc::now().to_rfc3339();
        // Keep the old receipt and file for rollback, including cloud versions.
        let backup_receipt = CONFIG_DIR.join("library").join(format!(
            "{}-{}-backup-{}.json",
            video.id,
            page.cid,
            uuid::Uuid::new_v4()
        ));
        library::atomic_write(&backup_receipt, &serde_json::to_vec(&old)?).await?;
        if old.cloud {
            ensure!(!config.strm_base_url.is_empty(), "云端升级需要先配置本服务 STRM 地址");
            let cd2 = crate::cd2::Cd2Client::connection(config)?.context("CD2 未配置")?;
            let version_path = old.metadata_path.with_file_name(format!(
                "bili-{}-{}-{}.mp4",
                video.id,
                page.cid,
                uuid::Uuid::new_v4().simple()
            ));
            cd2.upload(temporary.file_path(), &version_path).await?;
            new.storage_path = cd2.remote_path(&version_path)?;
            new.cloud_file_id = cd2.file_id(&new.storage_path).await.ok();
            // Commit only after upload is confirmed; existing STRM points to the same stable token.
            library::write_strm(&old, config).await?;
            library::save(&new).await?;
        } else {
            let target = Path::new(&old.storage_path);
            let staged = target.with_extension(format!("{}.new", uuid::Uuid::new_v4()));
            let backup = target.with_extension(format!("{}.mp4.backup", uuid::Uuid::new_v4()));
            tokio::fs::copy(temporary.file_path(), &staged).await?;
            tokio::fs::rename(target, &backup).await?;
            if let Err(error) = tokio::fs::rename(&staged, target).await {
                tokio::fs::rename(&backup, target).await?;
                return Err(error.into());
            }
            if let Err(error) = library::save(&new).await {
                let _ = tokio::fs::rename(target, &staged).await;
                tokio::fs::rename(&backup, target).await?;
                return Err(error);
            }
        }
        comparison.current = new.quality;
        comparison.upgradeable = false;
        comparison.message = "升级完成，已保留旧版本".into();
        library::atomic_write(&comparison_path(id), &serde_json::to_vec(&comparison)?).await?;
        Ok(comparison.message)
    }
    .await;
    temporary.drop_async().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn quality() -> SavedQuality {
        SavedQuality {
            width: Some(1920),
            height: Some(1080),
            duration: Some(120.0),
            codec: Some("h264".into()),
            bitrate: Some(1_000_000),
            frame_rate: Some("30/1".into()),
            ..Default::default()
        }
    }
    #[test]
    fn rejects_codec_only_and_duration_regression() {
        let old = quality();
        let mut new = old.clone();
        new.codec = Some("hevc".into());
        new.bitrate = Some(2_000_000);
        assert!(!verified(&old, &new));
        new.width = Some(3840);
        new.height = Some(2160);
        assert!(verified(&old, &new));
        new.duration = Some(60.0);
        assert!(!verified(&old, &new));
    }
    #[test]
    fn same_codec_requires_real_bitrate_or_fps_improvement() {
        let old = quality();
        let mut new = old.clone();
        new.bitrate = Some(1_050_000);
        assert!(!verified(&old, &new));
        new.bitrate = Some(1_200_000);
        assert!(verified(&old, &new));
        new.frame_rate = Some("24/1".into());
        assert!(!verified(&old, &new));
    }
}
