use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Keep existing subscription and database paths under /media; only MP4 bytes
/// are written to a separate mount with the same relative directory layout.
pub struct StorageLayout {
    metadata_root: PathBuf,
    video_root: PathBuf,
}

impl StorageLayout {
    pub fn from_env() -> Result<Option<Self>> {
        let metadata = std::env::var_os("BILI_SYNC_METADATA_ROOT").filter(|s| !s.is_empty());
        let video = std::env::var_os("BILI_SYNC_VIDEO_ROOT").filter(|s| !s.is_empty());
        let (metadata, video) = match (metadata, video) {
            (Some(metadata), Some(video)) => (metadata, video),
            (None, None) => return Ok(None),
            _ => bail!("BILI_SYNC_METADATA_ROOT and BILI_SYNC_VIDEO_ROOT must be set together"),
        };
        let metadata_root = PathBuf::from(metadata);
        let video_root = PathBuf::from(video);
        if !metadata_root.is_absolute() || !video_root.is_absolute() {
            bail!("metadata and video roots must be absolute paths");
        }
        // Both mounts must already exist. A missing mount must never cause
        // video data to be written to the container overlay by accident.
        let metadata_root = dunce::canonicalize(metadata_root).context("resolve metadata root")?;
        let video_root = dunce::canonicalize(video_root).context("resolve video root")?;
        if metadata_root.starts_with(&video_root) || video_root.starts_with(&metadata_root) {
            bail!("metadata and video roots must not overlap");
        }
        Ok(Some(Self {
            metadata_root,
            video_root,
        }))
    }

    pub fn video_path(&self, metadata_path: &Path) -> Result<PathBuf> {
        let relative = metadata_path
            .strip_prefix(&self.metadata_root)
            .with_context(|| format!("path {} is outside BILI_SYNC_METADATA_ROOT", metadata_path.display()))?;
        if relative.as_os_str().is_empty() {
            bail!("refusing to use the video root as a video directory");
        }
        Ok(self.video_root.join(relative))
    }

    pub fn metadata_path_for_video(&self, video_path: &Path) -> Result<PathBuf> {
        let relative = video_path.strip_prefix(&self.video_root)
            .with_context(|| format!("path {} is outside BILI_SYNC_VIDEO_ROOT", video_path.display()))?;
        Ok(self.metadata_root.join(relative))
    }

    pub fn video_path_for(metadata_path: &Path) -> Result<PathBuf> {
        match Self::from_env()? {
            Some(layout) => layout.video_path(metadata_path),
            None => Ok(metadata_path.to_path_buf()),
        }
    }
}

/// Delete both halves only when the UI explicitly requests video removal.
pub async fn remove_video_files(metadata_path: &Path) -> Result<()> {
    let video_path = StorageLayout::video_path_for(metadata_path)?;
    // Direct CD2 uploads are independent of the old /video FUSE mount. Never
    // remove that mount's contents as a side effect of a local reset.
    let direct_cd2 = !crate::config::VersionedConfig::get().read().cd2_url.trim().is_empty();
    if video_path != metadata_path && !direct_cd2 {
        match tokio::fs::remove_dir_all(&video_path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).with_context(|| format!("remove {}", video_path.display())),
        }
    }
    match tokio::fs::remove_dir_all(metadata_path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("remove {}", metadata_path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_existing_media_paths_to_video_mount() {
        let layout = StorageLayout {
            metadata_root: PathBuf::from("/media"),
            video_root: PathBuf::from("/video"),
        };
        assert_eq!(
            layout
                .video_path(Path::new("/media/earth/BV1/Season 1/BV1.mp4"))
                .unwrap(),
            PathBuf::from("/video/earth/BV1/Season 1/BV1.mp4")
        );
        assert!(layout.video_path(Path::new("/other/BV1.mp4")).is_err());
        assert!(layout.video_path(Path::new("/media")).is_err());
    }
}
