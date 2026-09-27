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
        let metadata_root = std::env::var("BILI_SYNC_METADATA_ROOT").unwrap_or_else(|_| "/media".to_string());
        let path = if receipt.cloud {
            std::path::Path::new(&receipt.storage_path)
        } else {
            receipt.metadata_path.as_path()
        };
        let base = std::path::Path::new(if receipt.cloud { &root } else { &metadata_root });
        let relative = path.strip_prefix(base).unwrap_or(path);
        let folder = relative
            .components()
            .next()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_else(|| "根目录".into());
        let name = format!("{} · {}", if receipt.cloud { "115" } else { "本地" }, folder);
        let group = folders.entry(name.clone()).or_insert_with(|| Folder {
            name,
            ..Default::default()
        });
        group.bytes += receipt.bytes;
        group.count += 1;
    }
    Ok(ApiResponse::ok(folders.into_values().collect()))
}
