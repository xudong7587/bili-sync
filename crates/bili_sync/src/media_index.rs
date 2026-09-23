use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::fs;
use tokio::sync::Mutex;

use crate::config::{CONFIG_DIR, Config, VersionedConfig};

// A completion signal is sufficient: MediaIndex chooses the cloud provider and
// scan directory from its own saved configuration. Keep an on-disk pending flag
// so a failed request is retried on a later download cycle.
static PENDING_LOCK: Mutex<()> = Mutex::const_new(());

struct Webhook {
    url: String,
    token: String,
}

impl Webhook {
    fn from_config(config: &Config) -> Result<Option<Self>> {
        let url = config.media_index_webhook_url.trim();
        let token = config.media_index_webhook_token.trim();
        if url.is_empty() && token.is_empty() {
            return Ok(None);
        }
        if url.is_empty() || token.is_empty() {
            bail!("MediaIndex Webhook 地址和令牌必须同时填写");
        }
        let parsed = reqwest::Url::parse(&url).context("invalid MediaIndex webhook URL")?;
        if !matches!(parsed.scheme(), "http" | "https") {
            bail!("MediaIndex webhook URL must use HTTP or HTTPS");
        }
        Ok(Some(Self {
            url: url.to_owned(),
            token: token.to_owned(),
        }))
    }
}

fn pending_path() -> PathBuf {
    CONFIG_DIR.join("media-index-webhook.pending")
}

pub fn validate(config: &Config) -> Result<()> {
    Webhook::from_config(config)?;
    Ok(())
}

pub async fn mark_pending() -> Result<()> {
    if Webhook::from_config(&VersionedConfig::get().read())?.is_none() {
        return Ok(());
    }
    let _guard = PENDING_LOCK.lock().await;
    fs::create_dir_all(&*CONFIG_DIR).await?;
    fs::write(pending_path(), b"pending\n").await?;
    Ok(())
}

pub async fn flush_pending() -> Result<()> {
    let Some(webhook) = Webhook::from_config(&VersionedConfig::get().read())? else {
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
        .bearer_auth(&webhook.token)
        .json(&serde_json::json!({ "event": "finished" }))
        .send()
        .await?
        .error_for_status()
        .context("MediaIndex rejected the completion webhook")?;
    fs::remove_file(path).await?;
    info!("MediaIndex 入库通知已发送");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_config_without_webhook_fields_stays_disabled() {
        let mut saved = serde_json::to_value(Config::default()).unwrap();
        let fields = saved.as_object_mut().unwrap();
        fields.remove("media_index_webhook_url");
        fields.remove("media_index_webhook_token");
        let loaded: Config = serde_json::from_value(saved).unwrap();
        assert!(Webhook::from_config(&loaded).unwrap().is_none());
    }

    #[test]
    fn webhook_requires_both_valid_fields() {
        let mut config = Config {
            media_index_webhook_url: "https://media.example.com/api/webhooks/in/test".into(),
            ..Config::default()
        };
        assert!(Webhook::from_config(&config).is_err());
        config.media_index_webhook_token = "secret".into();
        assert!(Webhook::from_config(&config).unwrap().is_some());
        config.media_index_webhook_url = "file:///tmp/test".into();
        assert!(Webhook::from_config(&config).is_err());
    }
}
