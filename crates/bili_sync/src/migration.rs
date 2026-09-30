//! Local inventory only; durable sequential uploads with explicit pause/resume.
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use anyhow::{Context, Result, ensure};
use bili_sync_entity::page;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::cd2::Cd2Client;
use crate::config::{CONFIG_DIR, VersionedConfig};
use crate::library::{self, FileReceipt, SavedQuality};

static STATE: Mutex<Option<Journal>> = Mutex::const_new(None);

#[derive(Clone, Serialize, Deserialize)]
pub struct Options {
    pub source_root: String,
    pub min_interval: u64,
    pub max_interval: u64,
    pub batch_size: usize,
    pub batch_cooldown: u64,
}
impl Options {
    fn validate(&self) -> Result<()> {
        ensure!(
            Path::new(&self.source_root).is_absolute(),
            "本地视频根目录必须是容器内绝对路径"
        );
        ensure!(
            self.min_interval >= 5 && self.max_interval >= self.min_interval && self.max_interval <= 3600,
            "文件间隔应为 5–3600 秒，最长间隔不能小于最短间隔"
        );
        ensure!((1..=100).contains(&self.batch_size), "每批应为 1–100 个视频文件");
        ensure!(self.batch_cooldown <= 3600, "批次休息不能超过 3600 秒");
        Ok(())
    }
    fn delay(&self, completed: usize) -> u64 {
        let jitter = self.min_interval
            + (uuid::Uuid::new_v4().as_u128() % u128::from(self.max_interval - self.min_interval + 1)) as u64;
        jitter
            + if completed.is_multiple_of(self.batch_size) {
                self.batch_cooldown
            } else {
                0
            }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Status {
    pub phase: String,
    pub total: usize,
    pub completed: usize,
    pub total_bytes: u64,
    pub completed_bytes: u64,
    pub skipped_cloud: usize,
    pub skipped_missing: usize,
    pub skipped_invalid: usize,
    pub current_file: String,
    pub destination: String,
    pub error: Option<String>,
    pub notification_error: Option<String>,
    pub options: Option<Options>,
    pub updated_at: String,
    #[serde(default)]
    pub samples: Vec<PreviewFile>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct PreviewFile {
    pub source: String,
    pub target: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Item {
    source: PathBuf,
    modified_ms: u64,
    receipt: FileReceipt,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Journal {
    status: Status,
    items: Vec<Item>,
    cd2_url: String,
    #[serde(skip)]
    pause_token: Option<CancellationToken>,
}
fn journal_path() -> PathBuf {
    CONFIG_DIR.join("local-video-migration.json")
}
async fn load(state: &mut Option<Journal>) -> Result<&mut Journal> {
    if state.is_none() {
        *state = Some(read_journal(&journal_path()).await?);
    }
    Ok(state.as_mut().unwrap())
}
async fn read_journal(path: &Path) -> Result<Journal> {
    let mut journal = match tokio::fs::read(path).await {
        Ok(bytes) => serde_json::from_slice::<Journal>(&bytes).context("迁移进度文件无法读取")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Journal::default(),
        Err(e) => return Err(e.into()),
    };
    ensure!(
        journal.status.total == journal.items.len() && journal.status.completed <= journal.status.total,
        "迁移清单和完成数量不一致，请检查进度文件"
    );
    if !journal.items.is_empty() {
        journal
            .status
            .options
            .as_ref()
            .context("迁移缺少频率设置")?
            .validate()?;
    }
    if matches!(journal.status.phase.as_str(), "running" | "pausing") {
        journal.status.phase = "paused".into();
        journal.status.current_file.clear();
    }
    Ok(journal)
}
async fn persist(journal: &mut Journal) -> Result<()> {
    journal.status.updated_at = chrono::Local::now().to_rfc3339();
    tokio::fs::create_dir_all(&*CONFIG_DIR).await?;
    library::atomic_write(&journal_path(), &serde_json::to_vec(journal)?).await
}
pub async fn status() -> Result<Status> {
    Ok(load(&mut *STATE.lock().await).await?.status.clone())
}

fn relative_video(path: &Path, root: &Path) -> Result<PathBuf> {
    let relative = path.strip_prefix(root).context("数据库视频路径不在元数据根目录下")?;
    ensure!(
        !relative.as_os_str().is_empty() && relative.components().all(|c| matches!(c, Component::Normal(_))),
        "视频相对路径无效"
    );
    ensure!(
        relative.extension().and_then(|s| s.to_str()).is_some_and(|s| {
            ["mp4", "mkv", "m4v", "mov", "webm", "flv", "avi", "ts"].contains(&s.to_ascii_lowercase().as_str())
        }),
        "仅迁移视频文件"
    );
    Ok(relative.to_path_buf())
}
async fn identity(path: &Path, root: &Path) -> Result<(PathBuf, u64, u64)> {
    // Reject links escaping the selected local root and non-file entries.
    let canonical = tokio::fs::canonicalize(path).await?;
    ensure!(canonical.starts_with(root), "视频实际路径超出本地根目录");
    let metadata = tokio::fs::metadata(&canonical).await?;
    ensure!(metadata.is_file() && metadata.len() > 0, "本地视频为空或不是文件");
    let modified = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?;
    Ok((canonical, metadata.len(), modified))
}

pub async fn plan(db: &DatabaseConnection, options: Options) -> Result<Status> {
    options.validate()?;
    let _guard = crate::task::DownloadTaskManager::get().try_library_lock()?;
    let mut state = STATE.lock().await;
    let previous = load(&mut state).await?;
    ensure!(
        !matches!(previous.status.phase.as_str(), "running" | "pausing"),
        "请先暂停当前迁移再重新预览"
    );
    let config = VersionedConfig::get().snapshot();
    let cd2 = Cd2Client::connection(&config)?.context("请先保存 CD2 地址、令牌和网盘根目录")?;
    let root = tokio::fs::canonicalize(&options.source_root)
        .await
        .context("无法访问本地视频根目录")?;
    let metadata_root = dunce::canonicalize(
        std::env::var_os("BILI_SYNC_METADATA_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/media".into()),
    )?;
    let mut next = Journal {
        status: Status {
            phase: "ready".into(),
            destination: config.cd2_save_path.clone(),
            options: Some(options),
            ..Default::default()
        },
        cd2_url: config.cd2_url.clone(),
        ..Default::default()
    };
    let mut seen = HashSet::new();
    let previous_destinations = previous
        .items
        .iter()
        .map(|item| {
            (
                (item.receipt.video_id, item.receipt.cid),
                item.receipt.storage_path.as_str(),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    for part in page::Entity::find().all(db).await? {
        if !seen.insert((part.video_id, part.cid)) {
            continue;
        }
        let saved = library::load(part.video_id, part.cid).await?;
        if let Some(mut receipt) = saved.clone().filter(|r| r.cloud) {
            if previous_destinations
                .get(&(part.video_id, part.cid))
                .is_some_and(|path| *path == receipt.storage_path)
            {
                notify_upload(&mut receipt).await?;
            }
            next.status.skipped_cloud += 1;
            continue;
        }
        if ((part.download_status >> 3) & 7) != crate::utils::status::STATUS_OK {
            next.status.skipped_missing += 1;
            continue;
        }
        let metadata = saved
            .as_ref()
            .map(|r| r.metadata_path.clone())
            .or_else(|| part.path.as_ref().map(PathBuf::from));
        let Some(metadata) = metadata else {
            next.status.skipped_missing += 1;
            continue;
        };
        let Ok(relative) = relative_video(&metadata, &metadata_root) else {
            next.status.skipped_invalid += 1;
            continue;
        };
        let source = saved
            .as_ref()
            .map(|r| PathBuf::from(&r.storage_path))
            .filter(|p| p.starts_with(&root))
            .unwrap_or_else(|| root.join(relative));
        let (source, bytes, modified_ms) = match identity(&source, &root).await {
            Ok(identity) => identity,
            Err(_) => {
                next.status.skipped_missing += 1;
                continue;
            }
        };
        let storage_path = cd2.remote_path(&metadata)?;
        let receipt = FileReceipt {
            video_id: part.video_id,
            cid: part.cid,
            metadata_path: metadata,
            storage_path,
            cloud: true,
            upload_notification_pending: false,
            cloud_file_id: None,
            bytes,
            quality: saved
                .as_ref()
                .map(|r| r.quality.clone())
                .unwrap_or_else(|| SavedQuality {
                    width: part.width,
                    height: part.height,
                    duration: Some(f64::from(part.duration)),
                    ..Default::default()
                }),
            uploaded_at: String::new(),
            playback_token: saved
                .map(|r| r.playback_token)
                .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string()),
        };
        next.status.total_bytes += bytes;
        next.items.push(Item {
            source,
            modified_ms,
            receipt,
        });
    }
    next.items
        .sort_by(|a, b| a.receipt.metadata_path.cmp(&b.receipt.metadata_path));
    next.status.total = next.items.len();
    next.status.samples = next
        .items
        .iter()
        .take(3)
        .map(|item| PreviewFile {
            source: item.source.to_string_lossy().into_owned(),
            target: item.receipt.storage_path.clone(),
        })
        .collect();
    if next.items.is_empty() {
        next.status.phase = "completed".into();
    }
    persist(&mut next).await?;
    let status = next.status.clone();
    *previous = next;
    Ok(status)
}

pub async fn start(db: DatabaseConnection) -> Result<()> {
    let mut state = STATE.lock().await;
    let journal = load(&mut state).await?;
    ensure!(
        matches!(journal.status.phase.as_str(), "ready" | "paused" | "failed"),
        "没有可继续的迁移任务"
    );
    ensure!(journal.status.completed < journal.status.total, "迁移已经完成");
    let config = VersionedConfig::get().snapshot();
    ensure!(
        journal.cd2_url == config.cd2_url && journal.status.destination == config.cd2_save_path,
        "CD2 目标已变化，请恢复预览时的地址和目录后继续"
    );
    let cd2 = Cd2Client::connection(&config)?.context("CD2 未配置")?;
    let pause_token = CancellationToken::new();
    journal.pause_token = Some(pause_token.clone());
    journal.status.phase = "running".into();
    journal.status.error = None;
    persist(journal).await?;
    tokio::spawn(async move {
        let result = run(&db, &cd2, &pause_token).await;
        if let Err(error) = result {
            error!("存量视频迁移暂停：{error:#}");
            let mut state = STATE.lock().await;
            if let Some(journal) = state.as_mut() {
                journal.status.phase = "failed".into();
                journal.status.error = Some(format!("{error:#}"));
                if let Err(e) = persist(journal).await {
                    error!("保存迁移失败状态：{e:#}");
                }
            }
        }
    });
    Ok(())
}
pub async fn pause() -> Result<()> {
    let mut state = STATE.lock().await;
    let journal = load(&mut state).await?;
    if journal.status.phase == "running" {
        journal.status.phase = "pausing".into();
        persist(journal).await?;
        if let Some(token) = &journal.pause_token {
            token.cancel();
        }
    }
    Ok(())
}
async fn notify_upload(receipt: &mut FileReceipt) -> Result<()> {
    if receipt.upload_notification_pending {
        crate::media_index::mark_pending().await?;
        receipt.upload_notification_pending = false;
        library::save(receipt).await?;
    }
    Ok(())
}
async fn run(db: &DatabaseConnection, cd2: &Cd2Client, pause_token: &CancellationToken) -> Result<()> {
    loop {
        let item = {
            let mut state = STATE.lock().await;
            let journal = load(&mut state).await?;
            if journal.status.phase == "pausing" || journal.status.completed == journal.status.total {
                journal.status.phase = if journal.status.completed == journal.status.total {
                    "completed"
                } else {
                    "paused"
                }
                .into();
                journal.status.current_file.clear();
                persist(journal).await?;
                break;
            }
            let item = journal.items[journal.status.completed].clone();
            journal.status.current_file = item.receipt.metadata_path.to_string_lossy().into_owned();
            persist(journal).await?;
            item
        };
        // Release the shared library lock during pacing so regular updates can run.
        let guard = tokio::select! {
            guard = crate::task::DownloadTaskManager::get().library_lock() => guard,
            _ = pause_token.cancelled() => continue,
        };
        if pause_token.is_cancelled() {
            drop(guard);
            continue;
        }
        let part = page::Entity::find()
            .filter(page::Column::VideoId.eq(item.receipt.video_id))
            .filter(page::Column::Cid.eq(item.receipt.cid))
            .one(db)
            .await?
            .context("视频已从数据库移除，请重新预览迁移清单")?;
        ensure!(
            ((part.download_status >> 3) & 7) == crate::utils::status::STATUS_OK,
            "视频下载状态已被重置，请重新预览迁移清单"
        );
        ensure!(
            cd2.remote_path(&item.receipt.metadata_path)? == item.receipt.storage_path,
            "迁移目标路径已变化"
        );
        if let Some(mut existing) = library::load(item.receipt.video_id, item.receipt.cid)
            .await?
            .filter(|r| r.cloud)
        {
            // Receipt saved before a crash: never send file data twice or lose its pending event.
            ensure!(
                existing.storage_path == item.receipt.storage_path && existing.bytes == item.receipt.bytes,
                "已有云端记录与迁移目标不符"
            );
            notify_upload(&mut existing).await?;
        } else {
            let source_root = {
                let mut state = STATE.lock().await;
                PathBuf::from(&load(&mut state).await?.status.options.as_ref().unwrap().source_root)
            };
            let root = tokio::fs::canonicalize(source_root).await?;
            let (_, bytes, modified) = identity(&item.source, &root).await?;
            ensure!(
                bytes == item.receipt.bytes && modified == item.modified_ms,
                "本地视频在预览后改变，请核对当前文件"
            );
            cd2.upload(&item.source, &item.receipt.metadata_path).await?;
            let (_, after_bytes, after_modified) = identity(&item.source, &root).await?;
            ensure!(
                after_bytes == bytes && after_modified == modified,
                "上传期间本地视频改变，停止登记"
            );
            let mut receipt = item.receipt.clone();
            receipt.cloud_file_id = Some(cd2.file_id(&receipt.storage_path).await?);
            receipt.uploaded_at = chrono::Local::now().to_rfc3339();
            // A formerly local receipt is newly registered in the cloud library.
            // Reuse also covers a crash after CD2 finished but before this receipt was saved.
            receipt.upload_notification_pending = true;
            library::save(&receipt).await?;
            notify_upload(&mut receipt).await?;
        }
        drop(guard);
        let (delay, flush) = {
            let mut state = STATE.lock().await;
            let journal = load(&mut state).await?;
            journal.status.completed += 1;
            journal.status.completed_bytes += item.receipt.bytes;
            let options = journal.status.options.as_ref().unwrap();
            let flush = journal.status.completed.is_multiple_of(options.batch_size)
                || journal.status.completed == journal.status.total
                || journal.status.phase == "pausing";
            let delay = options.delay(journal.status.completed);
            persist(journal).await?;
            (delay, flush)
        };
        if flush {
            flush_notification().await?;
        }
        let current = status().await?;
        if current.phase == "running" && current.completed < current.total {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(delay)) => {},
                _ = pause_token.cancelled() => {},
            }
        }
    }
    flush_notification().await
}
async fn flush_notification() -> Result<()> {
    let error = crate::media_index::flush_pending()
        .await
        .err()
        .map(|e| format!("{e:#}"));
    let mut state = STATE.lock().await;
    let journal = load(&mut state).await?;
    journal.status.notification_error = error;
    persist(journal).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn video_only_and_safe_relative_layout() {
        let root = Path::new("/media");
        assert_eq!(
            relative_video(Path::new("/media/收藏/合集/Season 1/a.MP4"), root).unwrap(),
            Path::new("收藏/合集/Season 1/a.MP4")
        );
        for path in [
            "/media/a.nfo",
            "/media/poster.jpg",
            "/media/a.strm",
            "/media/../a.mp4",
            "/other/a.mp4",
        ] {
            assert!(relative_video(Path::new(path), root).is_err());
        }
    }
    #[test]
    fn pace_has_lower_bound_and_batch_rest() {
        let options = Options {
            source_root: "/media".into(),
            min_interval: 15,
            max_interval: 30,
            batch_size: 10,
            batch_cooldown: 120,
        };
        assert!(options.validate().is_ok());
        for _ in 0..100 {
            assert!((15..=30).contains(&options.delay(1)));
            assert!((135..=150).contains(&options.delay(10)));
        }
        assert!(
            Options {
                min_interval: 0,
                ..options.clone()
            }
            .validate()
            .is_err()
        );
        assert!(
            Options {
                batch_size: 0,
                ..options
            }
            .validate()
            .is_err()
        );
    }
    #[tokio::test]
    async fn refuses_symlink_escape_and_empty_files() {
        let dir = std::env::temp_dir().join(format!("bili-migrate-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let root = tokio::fs::canonicalize(&dir).await.unwrap();
        let path = root.join("a.mp4");
        tokio::fs::write(&path, b"abc").await.unwrap();
        assert_eq!(identity(&path, &root).await.unwrap().1, 3);
        assert!(identity(&path, &root.join("other")).await.is_err());
        tokio::fs::write(&path, b"").await.unwrap();
        assert!(identity(&path, &root).await.is_err());
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[tokio::test]
    async fn restart_keeps_queue_and_progress_paused_and_rejects_corruption() {
        let file = async_tempfile::TempFile::new().await.unwrap();
        let options = Options {
            source_root: "/media".into(),
            min_interval: 5,
            max_interval: 15,
            batch_size: 10,
            batch_cooldown: 120,
        };
        let item = Item {
            source: "/media/收藏夹/a.mp4".into(),
            modified_ms: 123,
            receipt: FileReceipt {
                video_id: 1,
                cid: 2,
                metadata_path: "/media/收藏夹/a.mp4".into(),
                storage_path: "/网盘/收藏夹/a.mp4".into(),
                cloud: true,
                upload_notification_pending: true,
                cloud_file_id: Some("confirmed".into()),
                bytes: 99,
                quality: SavedQuality::default(),
                uploaded_at: String::new(),
                playback_token: "file-token".into(),
            },
        };
        let mut journal = Journal {
            status: Status {
                phase: "running".into(),
                total: 2,
                completed: 1,
                completed_bytes: 99,
                options: Some(options),
                ..Default::default()
            },
            items: vec![item.clone(), item],
            cd2_url: "http://cd2:19798/".into(),
            ..Default::default()
        };
        tokio::fs::write(file.file_path(), serde_json::to_vec(&journal).unwrap())
            .await
            .unwrap();
        let restarted = read_journal(file.file_path()).await.unwrap();
        assert_eq!(restarted.status.phase, "paused");
        assert_eq!(restarted.status.completed, 1);
        assert_eq!(restarted.status.completed_bytes, 99);
        assert!(restarted.items[1].receipt.upload_notification_pending);
        assert_eq!(restarted.items[1].receipt.storage_path, "/网盘/收藏夹/a.mp4");
        journal.status.completed = 3;
        tokio::fs::write(file.file_path(), serde_json::to_vec(&journal).unwrap())
            .await
            .unwrap();
        assert!(read_journal(file.file_path()).await.is_err());
    }
}
