use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use axum::extract::Query;
use axum::routing::get;
use axum::{Extension, Router};
use bili_sync_entity::{page, video};
use sea_orm::{DatabaseConnection, EntityTrait};
use serde::{Deserialize, Serialize};

use crate::api::wrapper::{ApiError, ApiResponse};
use crate::config::{StorageMode, VersionedConfig};

#[derive(Default, Serialize)]
struct Folder {
    name: String,
    bytes: u64,
    count: usize,
    unknown_count: usize,
    local_count: usize,
    cloud_count: usize,
    mixed_count: usize,
    unknown_location: usize,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Metric {
    #[default]
    Count,
    Bytes,
}
#[derive(Default, Deserialize)]
struct Request {
    #[serde(default)]
    metric: Metric,
}

pub fn router() -> Router {
    Router::new().route("/dashboard/storage", get(storage))
}
fn folder_name(path: &str, root: &str) -> String {
    let parent = Path::new(path).parent().unwrap_or_else(|| Path::new(""));
    let relative = parent.strip_prefix(root).unwrap_or(parent);
    if relative.as_os_str().is_empty() {
        "未归档 / 根目录".into()
    } else {
        relative.to_string_lossy().into_owned()
    }
}
async fn storage(
    Extension(db): Extension<DatabaseConnection>,
    Query(request): Query<Request>,
) -> Result<ApiResponse<Vec<Folder>>, ApiError> {
    // Only local receipts and exact local file metadata; never query a cloud directory.
    let videos = video::Entity::find().all(&db).await?;
    let mut folders: BTreeMap<String, Folder> = BTreeMap::new();
    let root = std::env::var("BILI_SYNC_METADATA_ROOT").unwrap_or_else(|_| "/media".into());
    let config = VersionedConfig::get().read().clone();
    let local = config.storage_mode == StorageMode::Local
        || (config.storage_mode == StorageMode::Auto
            && config.cd2_url.trim().is_empty()
            && std::env::var_os("BILI_SYNC_VIDEO_ROOT").is_none());
    let mut parts: HashMap<i32, Vec<page::Model>> = HashMap::new();
    for part in page::Entity::find().all(&db).await? {
        parts.entry(part.video_id).or_default().push(part);
    }
    for video in videos {
        let name = folder_name(&video.path, &root);
        let folder = folders.entry(name.clone()).or_insert_with(|| Folder {
            name,
            ..Default::default()
        });
        folder.count += 1;
        let Some(pages) = parts.get(&video.id).filter(|p| !p.is_empty()) else {
            folder.unknown_count += usize::from(matches!(request.metric, Metric::Bytes));
            folder.unknown_location += 1;
            continue;
        };
        let mut unknown = false;
        let mut has_local = false;
        let mut has_cloud = false;
        let mut unlocated = false;
        for part in pages {
            let mut located = false;
            let receipt = crate::library::load(video.id, part.cid).await.map_err(ApiError::from)?;
            let size = if let Some(receipt) = receipt {
                if receipt.cloud {
                    has_cloud = true;
                    located = true;
                    (receipt.bytes > 0).then_some(receipt.bytes)
                } else {
                    let size = tokio::fs::metadata(&receipt.storage_path)
                        .await
                        .ok()
                        .filter(|m| m.is_file())
                        .map(|m| m.len());
                    has_local |= size.is_some();
                    located = size.is_some();
                    size
                }
            } else if let Some(path) = &part.path {
                // /media is the local metadata mount in split mode. Old MP4s
                // may still be here after switching new downloads to CD2.
                if local || Path::new(path).starts_with(&root) {
                    let size = tokio::fs::metadata(path)
                        .await
                        .ok()
                        .filter(|m| m.is_file())
                        .map(|m| m.len());
                    has_local |= size.is_some();
                    located = size.is_some();
                    size
                } else {
                    None
                }
            } else {
                None
            };
            unlocated |= !located;
            if let Some(size) = size {
                if matches!(request.metric, Metric::Bytes) {
                    folder.bytes += size;
                }
            } else {
                unknown = true;
            }
        }
        folder.unknown_count += usize::from(unknown && matches!(request.metric, Metric::Bytes));
        match (has_local, has_cloud, unlocated) {
            (_, _, true) => folder.unknown_location += 1,
            (true, true, false) => folder.mixed_count += 1,
            (true, false, false) => folder.local_count += 1,
            (false, true, false) => folder.cloud_count += 1,
            (false, false, false) => folder.unknown_location += 1,
        }
    }
    Ok(ApiResponse::ok(folders.into_values().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn groups_by_subscription_directory_not_video_title() {
        assert_eq!(
            folder_name("/media/收藏夹/basketball/视频一", "/media"),
            "收藏夹/basketball"
        );
        assert_eq!(folder_name("/media/cook/视频二", "/media"), "cook");
        assert_eq!(folder_name("", "/media"), "未归档 / 根目录");
    }
}
