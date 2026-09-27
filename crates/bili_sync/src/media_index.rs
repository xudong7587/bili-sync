use std::path::{Path, PathBuf};
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
        // Legacy playback URLs must not suppress completion notifications.
        if !config.media_index_webhook_enabled {
            return Ok(None);
        }
        let url = config.media_index_webhook_url.trim();
        let token = config.media_index_webhook_token.trim();
        if url.is_empty() {
            bail!("请填写 MediaIndex Webhook 地址");
        }
        let parsed = reqwest::Url::parse(url).context("invalid MediaIndex webhook URL")?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
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
    queue_pending_at(&pending_path()).await
}

async fn queue_pending_at(path: &Path) -> Result<()> {
    // Coalesce uploads in the same source batch and retain the ID of failed deliveries.
    if !fs::try_exists(path).await? {
        crate::library::atomic_write(path, uuid::Uuid::new_v4().to_string().as_bytes()).await?;
    }
    Ok(())
}

pub async fn flush_pending() -> Result<()> {
    let Some(webhook) = Webhook::from_config(&VersionedConfig::get().read())? else {
        return Ok(());
    };
    let _guard = PENDING_LOCK.lock().await;
    deliver_pending(&webhook, &pending_path()).await
}

async fn deliver_pending(webhook: &Webhook, path: &Path) -> Result<()> {
    if !fs::try_exists(path).await? {
        return Ok(());
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()?;
    let event_id = fs::read_to_string(path).await?;
    let mut request = client.post(&webhook.url).json(
        &serde_json::json!({ "event": "finished", "type": "bili-sync.download.finished", "id": event_id.trim() }),
    );
    if !webhook.token.is_empty() {
        request = request
            .header("X-MediaIndex-Webhook", &webhook.token)
            .bearer_auth(&webhook.token);
    }
    let response = request.send().await.map_err(|error| {
        anyhow::anyhow!(
            "MediaIndex 请求失败：{}",
            if error.is_timeout() {
                "超时"
            } else {
                "连接或协议错误"
            }
        )
    })?;
    anyhow::ensure!(
        response.status().is_success(),
        "MediaIndex 返回 HTTP {}；通知已保留，稍后重试",
        response.status().as_u16()
    );
    fs::remove_file(path).await?;
    info!("MediaIndex 入库通知已发送");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_cycles_do_not_notify_and_multiple_uploads_share_one_event() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use axum::Router;
        use axum::routing::post;
        let requests = Arc::new(AtomicUsize::new(0));
        let seen = requests.clone();
        let app = Router::new().route(
            "/notify",
            post(move || {
                let seen = seen.clone();
                async move {
                    seen.fetch_add(1, Ordering::SeqCst);
                    axum::http::StatusCode::ACCEPTED
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let webhook = Webhook {
            url: format!("http://{}/notify", listener.local_addr().unwrap()),
            token: String::new(),
        };
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let dir = std::env::temp_dir().join(format!("bili-no-spam-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("pending");
        for _ in 0..3 {
            deliver_pending(&webhook, &path).await.unwrap();
        }
        assert_eq!(requests.load(Ordering::SeqCst), 0);
        queue_pending_at(&path).await.unwrap();
        let id = fs::read(&path).await.unwrap();
        queue_pending_at(&path).await.unwrap();
        assert_eq!(fs::read(&path).await.unwrap(), id);
        deliver_pending(&webhook, &path).await.unwrap();
        for _ in 0..3 {
            deliver_pending(&webhook, &path).await.unwrap();
        }
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        server.abort();
        fs::remove_dir_all(dir).await.unwrap();
    }

    #[tokio::test]
    async fn failed_or_redirected_delivery_remains_pending_and_success_clears_it() {
        use axum::http::{HeaderMap, StatusCode};
        use axum::routing::post;
        use axum::{Json, Router};
        let dir = std::env::temp_dir().join(format!("bili-webhook-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("notification.pending");
        fs::write(&path, b"stable-event-id").await.unwrap();
        for status in [
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::FOUND,
            StatusCode::ACCEPTED,
        ] {
            let app = Router::new().route(
                "/notify",
                post(
                    move |headers: HeaderMap, Json(body): Json<serde_json::Value>| async move {
                        assert_eq!(headers["authorization"], "Bearer test-token");
                        assert_eq!(body["event"], "finished");
                        assert_eq!(body["id"], "stable-event-id");
                        status
                    },
                ),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/notify?token=never-log-this", listener.local_addr().unwrap());
            let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            let result = deliver_pending(
                &Webhook {
                    url,
                    token: "test-token".into(),
                },
                &path,
            )
            .await;
            assert_eq!(result.is_ok(), status.is_success());
            assert_eq!(fs::try_exists(&path).await.unwrap(), !status.is_success());
            if let Err(error) = result {
                assert!(!error.to_string().contains("never-log-this"));
            }
            server.abort();
        }
        fs::remove_dir(&dir).await.unwrap();
    }

    #[test]
    fn old_config_without_webhook_fields_stays_disabled() {
        let mut saved = serde_json::to_value(Config::default()).unwrap();
        let fields = saved.as_object_mut().unwrap();
        fields.remove("media_index_webhook_enabled");
        fields.remove("media_index_webhook_url");
        fields.remove("media_index_webhook_token");
        let loaded: Config = serde_json::from_value(saved).unwrap();
        assert!(Webhook::from_config(&loaded).unwrap().is_none());
    }

    #[test]
    fn retained_legacy_credentials_do_not_enable_hidden_notifications() {
        let mut fields = serde_json::to_value(Config::default()).unwrap();
        fields.as_object_mut().unwrap().remove("media_index_webhook_enabled");
        fields["media_index_webhook_url"] = "https://unused.example.com/notify".into();
        fields["media_index_webhook_token"] = "old-secret".into();
        let loaded: Config = serde_json::from_value(fields).unwrap();
        assert!(Webhook::from_config(&loaded).unwrap().is_none());
    }

    #[test]
    fn legacy_playback_configuration_does_not_disable_webhooks() {
        let config = Config {
            media_index_webhook_enabled: true,
            media_index_webhook_url: "https://example.com/notify?token=test".into(),
            strm_base_url: "https://bili.example.com".into(),
            ..Config::default()
        };
        assert!(Webhook::from_config(&config).unwrap().is_some());
    }
    #[test]
    fn webhook_accepts_url_token_and_optional_header_token() {
        let mut config = Config {
            media_index_webhook_enabled: true,
            media_index_webhook_url: "https://media.example.com/api/webhooks/in/test".into(),
            ..Config::default()
        };
        assert!(Webhook::from_config(&config).unwrap().is_some());
        config.media_index_webhook_token = "secret".into();
        assert!(Webhook::from_config(&config).unwrap().is_some());
        config.media_index_webhook_url = "file:///tmp/test".into();
        assert!(Webhook::from_config(&config).is_err());
    }
}
