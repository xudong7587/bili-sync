//! Durable per-file receipts. Upload acknowledgement is persisted before STRM generation.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::config::{CONFIG_DIR, Config};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct SavedQuality {
    pub qn: Option<u32>,
    pub codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration: Option<f64>,
    pub bitrate: Option<u64>,
    pub frame_rate: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct FileReceipt {
    pub video_id: i32,
    pub cid: i64,
    pub metadata_path: PathBuf,
    pub storage_path: String,
    pub cloud: bool,
    pub bytes: u64,
    pub quality: SavedQuality,
    pub uploaded_at: String,
    pub playback_token: String,
}

fn receipt_path(video_id: i32, cid: i64) -> PathBuf {
    CONFIG_DIR.join("library").join(format!("{video_id}-{cid}.json"))
}
pub async fn load(video_id: i32, cid: i64) -> Result<Option<FileReceipt>> {
    match tokio::fs::read(receipt_path(video_id, cid)).await {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
pub async fn save(receipt: &FileReceipt) -> Result<()> {
    let path = receipt_path(receipt.video_id, receipt.cid);
    tokio::fs::create_dir_all(path.parent().context("missing receipt parent")?).await?;
    atomic_write(&path, &serde_json::to_vec_pretty(receipt)?).await
}
pub async fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = async {
        tokio::fs::write(&temporary, bytes).await?;
        let file = tokio::fs::OpenOptions::new().write(true).open(&temporary).await?;
        file.sync_all().await?;
        tokio::fs::rename(&temporary, path).await?;
        Result::<()>::Ok(())
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(temporary).await;
    }
    result
}
pub fn validate(config: &Config) -> Result<()> {
    if config.strm_base_url.is_empty() {
        return Ok(());
    }
    let url = reqwest::Url::parse(&config.strm_base_url).context("STRM 播放服务地址无效")?;
    ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "STRM 地址应为播放器可访问的 bili-sync HTTP(S) 地址，不含凭据或查询参数"
    );
    Ok(())
}
pub async fn write_strm(receipt: &FileReceipt, config: &Config) -> Result<()> {
    if config.strm_base_url.is_empty() || !receipt.cloud {
        return Ok(());
    }
    validate(config)?;
    let content = format!(
        "{}/stream/{}/{}/{}\n",
        config.strm_base_url.trim_end_matches('/'),
        receipt.video_id,
        receipt.cid,
        receipt.playback_token
    );
    atomic_write(&receipt.metadata_path.with_extension("strm"), content.as_bytes())
        .await
        .context("视频上传已完成，但 STRM 写入失败；重试将只补写 STRM")
}
pub async fn probe(path: &Path, mut quality: SavedQuality) -> Result<SavedQuality> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=width,height,codec_name,bit_rate,r_frame_rate:format=duration,bit_rate",
                "-of",
                "json",
            ])
            .arg(path)
            .kill_on_drop(true)
            .output(),
    )
    .await??;
    ensure!(output.status.success(), "无法读取视频实际画质，原文件未改动");
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let stream = &value["streams"][0];
    quality.width = stream["width"].as_u64().map(|v| v as u32);
    quality.height = stream["height"].as_u64().map(|v| v as u32);
    quality.codec = stream["codec_name"].as_str().map(str::to_owned);
    quality.frame_rate = stream["r_frame_rate"].as_str().map(str::to_owned);
    quality.duration = value["format"]["duration"].as_str().and_then(|v| v.parse().ok());
    quality.bitrate = stream["bit_rate"]
        .as_str()
        .or_else(|| value["format"]["bit_rate"].as_str())
        .and_then(|v| v.parse().ok());
    Ok(quality)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn strm_targets_one_file_with_stable_authorization() {
        let dir = std::env::temp_dir().join(format!("bili-strm-test-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let receipt = FileReceipt {
            video_id: 12,
            cid: 34,
            metadata_path: dir.join("中文视频.mp4"),
            storage_path: "/115/视频.mp4".into(),
            cloud: true,
            bytes: 100,
            quality: SavedQuality::default(),
            uploaded_at: String::new(),
            playback_token: "file-specific-token".into(),
        };
        let config = Config {
            strm_base_url: "https://bili.example.com".into(),
            ..Default::default()
        };
        write_strm(&receipt, &config).await.unwrap();
        write_strm(&receipt, &config).await.unwrap();
        let path = receipt.metadata_path.with_extension("strm");
        assert_eq!(
            tokio::fs::read_to_string(&path).await.unwrap(),
            "https://bili.example.com/stream/12/34/file-specific-token\n"
        );
        assert!(!receipt.metadata_path.exists());
        tokio::fs::remove_file(path).await.unwrap();
        tokio::fs::remove_dir(dir).await.unwrap();
    }
    #[test]
    fn rejects_credentials_and_expiring_query_urls() {
        for value in [
            "file:///media",
            "https://user:password@example.com",
            "https://example.com?token=secret",
        ] {
            assert!(
                validate(&Config {
                    strm_base_url: value.into(),
                    ..Default::default()
                })
                .is_err()
            );
        }
    }
}
