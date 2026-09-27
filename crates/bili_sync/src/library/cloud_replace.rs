//! Keep the media filename stable for external STRM generators. A durable journal
//! allows retries after either rename; cloud originals are retained as non-video backups.
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::{FileReceipt, atomic_write};
use crate::cd2::Cd2Client;
use crate::config::{CONFIG_DIR, Config};

#[derive(Serialize, Deserialize)]
struct Journal {
    old: FileReceipt,
    new: FileReceipt,
    staged: String,
    backup: String,
}
fn file_name(path: &str) -> Result<&str> {
    path.rsplit_once('/')
        .map(|(_, name)| name)
        .context("invalid cloud filename")
}
#[derive(Debug, PartialEq)]
enum Step {
    Backup,
    Promote,
    Commit,
}
fn next_step(target: Option<&str>, backup: Option<&str>, staged: Option<&str>, old: &str, new: &str) -> Result<Step> {
    ensure!(!old.is_empty() && !new.is_empty() && old != new, "替换文件身份无效");
    if target == Some(new) && backup == Some(old) && staged.is_none() {
        return Ok(Step::Commit);
    }
    ensure!(staged == Some(new), "新版本文件身份变化，停止替换");
    if target == Some(old) && backup.is_none() {
        return Ok(Step::Backup);
    }
    if target.is_none() && backup == Some(old) {
        return Ok(Step::Promote);
    }
    anyhow::bail!("云端路径与替换记录不符，请检查保留的恢复日志")
}
async fn finish(path: &Path, journal: &Journal, cd2: &Cd2Client) -> Result<()> {
    let old_id = journal.old.cloud_file_id.as_deref().context("旧版本缺少文件标识")?;
    let new_id = journal.new.cloud_file_id.as_deref().context("新版本缺少文件标识")?;
    loop {
        let target = cd2.confirmed_id(&journal.new.storage_path).await?;
        let backup = cd2.confirmed_id(&journal.backup).await?;
        let staged = cd2.confirmed_id(&journal.staged).await?;
        match next_step(target.as_deref(), backup.as_deref(), staged.as_deref(), old_id, new_id)? {
            Step::Backup => {
                cd2.rename_confirmed(&journal.old.storage_path, file_name(&journal.backup)?, old_id)
                    .await?
            }
            Step::Promote => {
                cd2.rename_confirmed(&journal.staged, file_name(&journal.new.storage_path)?, new_id)
                    .await?
            }
            Step::Commit => break,
        }
    }
    super::save(&journal.new).await?;
    crate::media_index::mark_pending().await?;
    tokio::fs::remove_file(path).await?;
    super::sync_parent(path).await?;
    Ok(())
}
pub async fn replace(source: &Path, old: &FileReceipt, new: &mut FileReceipt, config: &Config) -> Result<()> {
    let cd2 = Cd2Client::connection(config)?.context("CD2 未配置")?;
    // A changed destination must not relocate an existing library file implicitly.
    ensure!(
        cd2.remote_path(&old.metadata_path)? == old.storage_path,
        "保存路径已变化，请恢复原 CD2 路径后再升级此视频"
    );
    let expected_id = old
        .cloud_file_id
        .as_deref()
        .context("旧版本缺少云端标识，请先重新比对")?;
    ensure!(
        cd2.confirmed_id(&old.storage_path).await?.as_deref() == Some(expected_id),
        "原云端文件已变化，请重新比对"
    );
    let id = uuid::Uuid::new_v4();
    let staged_metadata = old.metadata_path.with_file_name(format!("bili-upgrade-{id}.pending"));
    cd2.upload(source, &staged_metadata).await?;
    let staged = cd2.remote_path(&staged_metadata)?;
    new.cloud_file_id = Some(cd2.file_id(&staged).await?);
    let original = old.clone();
    let backup = format!(
        "{}/bili-{id}.backup",
        old.storage_path.rsplit_once('/').context("invalid cloud path")?.0
    );
    let journal = Journal {
        old: original,
        new: new.clone(),
        staged,
        backup,
    };
    let directory = CONFIG_DIR.join("library");
    tokio::fs::create_dir_all(&directory).await?;
    let path = directory.join(format!("cloud-commit-{}-{}.pending", old.video_id, old.cid));
    ensure!(!tokio::fs::try_exists(&path).await?, "存在未完成的云端替换，请先恢复");
    let mut backup_receipt = journal.old.clone();
    backup_receipt.storage_path.clone_from(&journal.backup);
    atomic_write(
        &directory.join(format!("{}-{}-backup-{id}.json", old.video_id, old.cid)),
        &serde_json::to_vec(&backup_receipt)?,
    )
    .await?;
    atomic_write(&path, &serde_json::to_vec(&journal)?).await?;
    finish(&path, &journal, &cd2).await
}
pub async fn ensure_no_pending() -> Result<()> {
    let mut entries = match tokio::fs::read_dir(CONFIG_DIR.join("library")).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        ensure!(
            !(name.starts_with("cloud-commit-") && name.ends_with(".pending")),
            "云端替换尚未完成，请先检查更新恢复后再清空或移除视频源"
        );
    }
    Ok(())
}

pub async fn recover(config: &Config) -> Result<()> {
    let mut entries = match tokio::fs::read_dir(CONFIG_DIR.join("library")).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("cloud-commit-") || !name.ends_with(".pending") {
            continue;
        }
        let journal: Journal = serde_json::from_slice(&tokio::fs::read(entry.path()).await?)?;
        let cd2 = Cd2Client::connection(config)?.context("云端替换待恢复，请先配置 CD2")?;
        ensure!(
            cd2.remote_path(&journal.old.metadata_path)? == journal.old.storage_path,
            "云端替换待恢复，请恢复原 CD2 保存目录"
        );
        finish(&entry.path(), &journal, &cd2).await?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovers_each_rename_boundary_and_refuses_unrelated_files() {
        assert_eq!(
            next_step(Some("old"), None, Some("new"), "old", "new").unwrap(),
            Step::Backup
        );
        assert_eq!(
            next_step(None, Some("old"), Some("new"), "old", "new").unwrap(),
            Step::Promote
        );
        assert_eq!(
            next_step(Some("new"), Some("old"), None, "old", "new").unwrap(),
            Step::Commit
        );
        assert!(next_step(Some("unrelated"), Some("old"), Some("new"), "old", "new").is_err());
        assert!(next_step(None, None, Some("new"), "old", "new").is_err());
        assert!(next_step(Some("old"), None, Some("unrelated"), "old", "new").is_err());
    }
}
