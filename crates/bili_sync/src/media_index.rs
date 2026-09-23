use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::fs;
use tokio::sync::Mutex;

use crate::config::CONFIG_DIR;

// A completion signal is sufficient: MediaIndex chooses the cloud provider and
// scan directory from its own saved configuration. Keep an on-disk pending flag
// so a failed request is retried on a later download cycle.
static PENDING_LOCK: Mutex<()> = Mutex::const_new(());

struct Webhook {
    url: String,
    token: String,
}

impl Webhook {
    fn from_env() -> Result<Option<Self>> {
        let url = std::env::var("BILI_SYNC_MEDIA_INDEX_WEBHOOK_URL").unwrap_or_default();
        let token = std::env::var("BILI_SYNC_MEDIA_INDEX_WEBHOOK_TOKEN").unwrap_or_default();
        if url.is_empty() && token.is_empty() {
            return Ok(None);
        }
        if url.is_empty() || token.is_empty() {
            bail!("BILI_SYNC_MEDIA_INDEX_WEBHOOK_URL and BILI_SYNC_MEDIA_INDEX_WEBHOOK_TOKEN must be set together");
        }
        let parsed = reqwest::Url::parse(&url).context("invalid MediaIndex webhook URL")?;
        if !matches!(parsed.scheme(), "http" | "https") {
            bail!("MediaIndex webhook URL must use HTTP or HTTPS");
        }
        Ok(Some(Self { url, token }))
    }
}

fn pending_path() -> PathBuf {
    CONFIG_DIR.join("media-index-webhook.pending")
}

pub fn validate() -> Result<()> {
    Webhook::from_env()?;
    Ok(())
}

pub async fn mark_pending() -> Result<()> {
    if Webhook::from_env()?.is_none() {
        return Ok(());
    }
    let _guard = PENDING_LOCK.lock().await;
    fs::create_dir_all(&*CONFIG_DIR).await?;
    fs::write(pending_path(), b"pending\n").await?;
    Ok(())
}

pub async fn flush_pending() -> Result<()> {
    let Some(webhook) = Webhook::from_env()? else {
        return Ok(());
    };
    let _guard = PENDING_LOCK.lock().await;
    let path = pending_path();
    if !fs::try_exists(&path).await? {
        return Ok(());
    }
    let client = reqwest::Client::builder().timeout(Duration::from_secs(15)).build()?;
    client
        .post(&webhook.url)
        .header("X-MediaIndex-Webhook", &webhook.token)
        .json(&serde_json::json!({ "event": "finished" }))
        .send()
        .await?
        .error_for_status()
        .context("MediaIndex rejected the completion webhook")?;
    fs::remove_file(path).await?;
    info!("MediaIndex 入库通知已发送");
    Ok(())
}
