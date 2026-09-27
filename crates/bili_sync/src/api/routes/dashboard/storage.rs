use std::collections::BTreeMap;

use axum::Router;
use axum::routing::get;
use serde::Serialize;

use crate::api::wrapper::{ApiError, ApiResponse};
use crate::config::{CONFIG_DIR, VersionedConfig};
use crate::library::FileReceipt;
#[derive(Default, Serialize)]
struct Folder {
    name: String,
    bytes: u64,
    count: usize,
}
pub fn router() -> Router {
    Router::new().route("/dashboard/storage", get(storage))
}
async fn storage() -> Result<ApiResponse<Vec<Folder>>, ApiError> {
    let mut folders: BTreeMap<String, Folder> = BTreeMap::new();
    let root = VersionedConfig::get().read().cd2_save_path.clone();
    let mut entries = match tokio::fs::read_dir(CONFIG_DIR.join("library")).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ApiResponse::ok(vec![])),
        Err(e) => return Err(anyhow::Error::from(e).into()),
    };
    while let Some(entry) = entries.next_entry().await.map_err(anyhow::Error::from)? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".json") || name.contains("backup") {
            continue;
        }
        let bytes = tokio::fs::read(entry.path()).await.map_err(anyhow::Error::from)?;
        let Ok(receipt) = serde_json::from_slice::<FileReceipt>(&bytes) else {
            continue;
        };
        if !receipt.cloud {
            continue;
        }
        let relative = receipt
            .storage_path
            .strip_prefix(root.trim_end_matches('/'))
            .unwrap_or(&receipt.storage_path)
            .trim_start_matches('/');
        let name = relative.split('/').next().unwrap_or("根目录").to_string();
        let group = folders.entry(name.clone()).or_insert_with(|| Folder {
            name,
            ..Default::default()
        });
        group.bytes += receipt.bytes;
        group.count += 1;
    }
    Ok(ApiResponse::ok(folders.into_values().collect()))
}
