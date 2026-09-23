use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// An optional mirror for sidecar files. Video paths and database paths remain
/// on the media mount; only NFO, images, danmaku and subtitles use this mirror.
pub struct StorageLayout {
    media_root: PathBuf,
    metadata_root: PathBuf,
}

impl StorageLayout {
    pub fn from_env() -> Result<Option<Self>> {
        let media = std::env::var_os("BILI_SYNC_MEDIA_ROOT").filter(|s| !s.is_empty());
        let metadata = std::env::var_os("BILI_SYNC_METADATA_ROOT").filter(|s| !s.is_empty());
        let (media, metadata) = match (media, metadata) {
            (Some(media), Some(metadata)) => (media, metadata),
            (None, None) => return Ok(None),
            _ => bail!("BILI_SYNC_MEDIA_ROOT and BILI_SYNC_METADATA_ROOT must be set together"),
        };
        let media_root = PathBuf::from(media);
        let metadata_root = PathBuf::from(metadata);
        if !media_root.is_absolute() || !metadata_root.is_absolute() {
            bail!("media and metadata roots must be absolute paths");
        }
        let media_root = dunce::canonicalize(media_root).context("resolve media root")?;
        if metadata_root.starts_with(&media_root) {
            bail!("metadata root must not be inside the media mount");
        }
        std::fs::create_dir_all(&metadata_root).context("create metadata root")?;
        let metadata_root = dunce::canonicalize(metadata_root).context("resolve metadata root")?;
        if media_root.starts_with(&metadata_root) || metadata_root.starts_with(&media_root) {
            bail!("media and metadata roots must not overlap");
        }
        Ok(Some(Self {
            media_root,
            metadata_root,
        }))
    }

    pub fn metadata_path(&self, media_path: &Path) -> Result<PathBuf> {
        let relative = media_path
            .strip_prefix(&self.media_root)
            .with_context(|| format!("media path {} is outside BILI_SYNC_MEDIA_ROOT", media_path.display()))?;
        if relative.as_os_str().is_empty() {
            bail!("refusing to use the metadata root as a video directory");
        }
        Ok(self.metadata_root.join(relative))
    }

    pub fn metadata_path_for(media_path: &Path) -> Result<PathBuf> {
        match Self::from_env()? {
            Some(layout) => layout.metadata_path(media_path),
            None => Ok(media_path.to_path_buf()),
        }
    }
}

/// Remove both halves of a video directory when the UI explicitly requests
/// deletion. A missing half is harmless (for example, an interrupted download).
pub async fn remove_video_files(media_path: &Path) -> Result<()> {
    let metadata_path = StorageLayout::from_env()?
        .map(|layout| layout.metadata_path(media_path))
        .transpose()?;
    if let Some(metadata_path) = metadata_path {
        match tokio::fs::remove_dir_all(&metadata_path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).with_context(|| format!("remove {}", metadata_path.display())),
        }
    }
    match tokio::fs::remove_dir_all(media_path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("remove {}", media_path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_relative_structure_without_changing_filename() {
        let layout = StorageLayout {
            media_root: PathBuf::from("/media"),
            metadata_root: PathBuf::from("/metadata"),
        };
        assert_eq!(
            layout
                .metadata_path(Path::new("/media/earth/BV1/Season 1/BV1.mp4"))
                .unwrap(),
            PathBuf::from("/metadata/earth/BV1/Season 1/BV1.mp4")
        );
        assert!(layout.metadata_path(Path::new("/other/BV1.mp4")).is_err());
        assert!(layout.metadata_path(Path::new("/media")).is_err());
    }
}
