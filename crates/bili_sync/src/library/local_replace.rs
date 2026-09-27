//! Recoverable local video replacement. Keep the live filename present throughout commit.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use tokio::fs;

use super::{FileReceipt, atomic_write, sync_parent};
use crate::config::CONFIG_DIR;

#[derive(Serialize, Deserialize)]
struct Journal {
    old: FileReceipt,
    new: FileReceipt,
    staged: PathBuf,
    backup: PathBuf,
}
fn receipt_path(directory: &Path, receipt: &FileReceipt) -> PathBuf {
    directory.join(format!("{}-{}.json", receipt.video_id, receipt.cid))
}
async fn sync_file(path: &Path) -> Result<()> {
    fs::OpenOptions::new().write(true).open(path).await?.sync_all().await?;
    Ok(())
}
async fn prepare(directory: &Path, source: &Path, old: &FileReceipt, new: &FileReceipt) -> Result<(PathBuf, Journal)> {
    ensure!(
        !old.cloud
            && !new.cloud
            && old.video_id == new.video_id
            && old.cid == new.cid
            && old.storage_path == new.storage_path,
        "本地替换路径不一致"
    );
    let target = Path::new(&old.storage_path);
    ensure!(target.is_absolute(), "本地替换要求绝对路径");
    let id = uuid::Uuid::new_v4();
    let staged = target.with_extension(format!("{id}.new"));
    let backup = target.with_extension(format!("{id}.mp4.backup"));
    fs::create_dir_all(directory).await?;
    fs::copy(source, &staged).await?;
    sync_file(&staged).await?;
    // Hard link leaves the live path available; copy supports filesystems without links.
    if fs::hard_link(target, &backup).await.is_err() {
        fs::copy(target, &backup).await?;
    }
    sync_file(&backup).await?;
    sync_parent(&backup).await?;
    let mut backup_receipt = old.clone();
    backup_receipt.storage_path = backup.to_string_lossy().into_owned();
    atomic_write(
        &directory.join(format!("{}-{}-backup-{id}.json", old.video_id, old.cid)),
        &serde_json::to_vec(&backup_receipt)?,
    )
    .await?;
    let journal = Journal {
        old: old.clone(),
        new: new.clone(),
        staged,
        backup,
    };
    let path = directory.join(format!("local-commit-{}-{}-{id}.pending", old.video_id, old.cid));
    atomic_write(&path, &serde_json::to_vec(&journal)?).await?;
    Ok((path, journal))
}
async fn finish(directory: &Path, path: &Path, journal: &Journal) -> Result<()> {
    fs::rename(&journal.staged, &journal.new.storage_path).await?;
    sync_parent(Path::new(&journal.new.storage_path)).await?;
    atomic_write(
        &receipt_path(directory, &journal.new),
        &serde_json::to_vec(&journal.new)?,
    )
    .await?;
    fs::remove_file(path).await?;
    sync_parent(path).await?;
    Ok(())
}
async fn rollback(directory: &Path, path: &Path, journal: &Journal) -> Result<()> {
    let target = Path::new(&journal.old.storage_path);
    let restored = target.with_extension(format!("{}.restore", uuid::Uuid::new_v4()));
    if fs::hard_link(&journal.backup, &restored).await.is_err() {
        fs::copy(&journal.backup, &restored)
            .await
            .context("无法恢复旧视频；保留事务记录待下次启动重试")?;
    }
    sync_file(&restored).await?;
    fs::rename(&restored, target).await?;
    sync_parent(target).await?;
    atomic_write(
        &receipt_path(directory, &journal.old),
        &serde_json::to_vec(&journal.old)?,
    )
    .await?;
    match fs::remove_file(&journal.staged).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    match fs::remove_file(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    sync_parent(path).await?;
    Ok(())
}
pub async fn replace(source: &Path, old: &FileReceipt, new: &FileReceipt) -> Result<()> {
    let directory = CONFIG_DIR.join("library");
    let (path, journal) = prepare(&directory, source, old, new).await?;
    if let Err(error) = finish(&directory, &path, &journal).await {
        rollback(&directory, &path, &journal)
            .await
            .context("本地升级回滚未完成，启动时将再次恢复")?;
        return Err(error);
    }
    Ok(())
}
pub async fn recover() -> Result<()> {
    recover_at(&CONFIG_DIR.join("library")).await
}
async fn recover_at(directory: &Path) -> Result<()> {
    let mut entries = match fs::read_dir(directory).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("local-commit-") || !name.ends_with(".pending") {
            continue;
        }
        let journal: Journal = serde_json::from_slice(&fs::read(entry.path()).await?)?;
        rollback(directory, &entry.path(), &journal).await?;
        tracing::warn!(
            "已恢复中断的本地画质升级：视频 {}，分 P {}",
            journal.old.video_id,
            journal.old.cid
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::SavedQuality;
    #[tokio::test]
    async fn interruption_at_each_commit_boundary_restores_old_file_and_receipt() {
        for phase in 0..3 {
            let dir = std::env::temp_dir().join(format!("bili-recovery-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&dir).await.unwrap();
            let target = dir.join("video.mp4");
            let source = dir.join("candidate.tmp");
            fs::write(&target, b"old-video").await.unwrap();
            fs::write(&source, b"new-video-data").await.unwrap();
            let old = FileReceipt {
                video_id: 1,
                cid: 2,
                metadata_path: target.clone(),
                storage_path: target.to_string_lossy().into_owned(),
                cloud: false,
                cloud_file_id: None,
                bytes: 9,
                quality: SavedQuality::default(),
                uploaded_at: String::new(),
                playback_token: "stable".into(),
            };
            let mut new = old.clone();
            new.bytes = 14;
            let (path, journal) = prepare(&dir, &source, &old, &new).await.unwrap();
            assert_eq!(fs::read(&target).await.unwrap(), b"old-video");
            if phase >= 1 {
                fs::rename(&journal.staged, &target).await.unwrap();
            }
            if phase >= 2 {
                atomic_write(&receipt_path(&dir, &new), &serde_json::to_vec(&new).unwrap())
                    .await
                    .unwrap();
            }
            recover_at(&dir).await.unwrap();
            assert_eq!(fs::read(&target).await.unwrap(), b"old-video");
            let restored: FileReceipt =
                serde_json::from_slice(&fs::read(receipt_path(&dir, &old)).await.unwrap()).unwrap();
            assert_eq!(restored.bytes, 9);
            assert!(!path.exists());
            assert_eq!(fs::read(&journal.backup).await.unwrap(), b"old-video");
            recover_at(&dir).await.unwrap();
            fs::remove_dir_all(dir).await.unwrap();
        }
    }
    #[tokio::test]
    async fn completed_commit_keeps_new_file_across_recovery() {
        let dir = std::env::temp_dir().join(format!("bili-commit-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).await.unwrap();
        let target = dir.join("video.mp4");
        let source = dir.join("new.tmp");
        fs::write(&target, b"old").await.unwrap();
        fs::write(&source, b"new").await.unwrap();
        let old = FileReceipt {
            video_id: 1,
            cid: 2,
            metadata_path: target.clone(),
            storage_path: target.to_string_lossy().into_owned(),
            cloud: false,
            cloud_file_id: None,
            bytes: 3,
            quality: SavedQuality::default(),
            uploaded_at: String::new(),
            playback_token: "stable".into(),
        };
        let (path, journal) = prepare(&dir, &source, &old, &old).await.unwrap();
        finish(&dir, &path, &journal).await.unwrap();
        recover_at(&dir).await.unwrap();
        assert_eq!(fs::read(target).await.unwrap(), b"new");
        fs::remove_dir_all(dir).await.unwrap();
    }
}
