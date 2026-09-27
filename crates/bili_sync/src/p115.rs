//! Independent 115 Open authorization. Tokens never travel through CD2 or the browser.
use std::sync::LazyLock;

use anyhow::{Context, Result, ensure};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::config::CONFIG_DIR;
use crate::library;

static AUTH_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(35))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("115 HTTP client")
});
#[derive(Debug, thiserror::Error)]
#[error("115 连接失败，请稍后重试")]
struct RequestFailure {
    timeout: bool,
}
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/131.0.0.0 Safari/537.36";
pub const CHANNELS: &[&str] = &[
    "alipaymini",
    "wechatmini",
    "web",
    "android",
    "ios",
    "tv",
    "qandroid",
    "linux",
    "mac",
    "windows",
    "open",
];
pub fn validate_channel(channel: &str) -> Result<()> {
    ensure!(CHANNELS.contains(&channel), "不支持的 115 登录渠道");
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Credentials {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    expires_at: i64,
    #[serde(default)]
    cookie: String,
    #[serde(default)]
    channel: String,
}
pub struct Pending {
    pub uid: String,
    pub time: String,
    pub sign: String,
    verifier: String,
    channel: String,
    pub qrcode: String,
}

async fn data(request: reqwest::RequestBuilder) -> Result<Value> {
    // Do not include reqwest's URL or provider response body in user-visible errors.
    let response = request.send().await.map_err(|e| RequestFailure {
        timeout: e.is_timeout(),
    })?;
    ensure!(response.status().is_success(), "115 HTTP 状态 {}", response.status());
    let value: Value = response.json().await.context("115 返回格式无效")?;
    ensure!(
        value.get("state") != Some(&Value::Bool(false)) && value.get("state") != Some(&Value::from(0)),
        "115 请求失败（错误码 {}）",
        value.get("code").or(value.get("errno")).unwrap_or(&Value::Null)
    );
    value.get("data").cloned().context("115 未返回数据")
}
fn string(value: &Value, key: &str) -> Result<String> {
    match value.get(key) {
        Some(Value::String(v)) if !v.is_empty() => Ok(v.clone()),
        Some(Value::Number(v)) => Ok(v.to_string()),
        _ => anyhow::bail!("115 返回缺少 {key}"),
    }
}
pub async fn start(client_id: &str, channel: &str) -> Result<Pending> {
    validate_channel(channel)?;
    if channel != "open" {
        let value = data(HTTP.get("https://qrcodeapi.115.com/api/1.0/web/1.0/token/")).await?;
        let uid = string(&value, "uid")?;
        return Ok(Pending {
            qrcode: string(&value, "qrcode").unwrap_or_else(|_| format!("https://115.com/scan/dg-{uid}")),
            uid,
            time: string(&value, "time")?,
            sign: string(&value, "sign")?,
            verifier: String::new(),
            channel: channel.into(),
        });
    }
    ensure!(
        !client_id.is_empty() && client_id.len() <= 32 && client_id.bytes().all(|b| b.is_ascii_digit()),
        "115 应用 ID 应为数字"
    );
    let verifier = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let challenge = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(verifier.as_bytes()));
    let value = data(HTTP.post("https://passportapi.115.com/open/authDeviceCode").form(&[
        ("client_id", client_id),
        ("code_challenge", &challenge),
        ("code_challenge_method", "sha256"),
    ]))
    .await?;
    Ok(Pending {
        uid: string(&value, "uid")?,
        time: string(&value, "time")?,
        sign: string(&value, "sign")?,
        qrcode: string(&value, "qrcode")?,
        verifier,
        channel: channel.into(),
    })
}
pub async fn poll(pending: &Pending) -> Result<i64> {
    let result = data(HTTP.get("https://qrcodeapi.115.com/get/status/").query(&[
        ("uid", &pending.uid),
        ("time", &pending.time),
        ("sign", &pending.sign),
    ]))
    .await;
    let value = match result {
        Ok(value) => value,
        Err(e) if e.downcast_ref::<RequestFailure>().is_some_and(|e| e.timeout) => return Ok(0),
        Err(e) => return Err(e),
    };
    value["status"]
        .as_i64()
        .or_else(|| value["status"].as_str().and_then(|s| s.parse().ok()))
        .context("115 扫码状态无效")
}
async fn save(value: Value) -> Result<Credentials> {
    let credentials = Credentials {
        cookie: String::new(),
        channel: "open".into(),
        access_token: string(&value, "access_token")?,
        refresh_token: string(&value, "refresh_token")?,
        expires_at: chrono::Utc::now().timestamp() + value["expires_in"].as_i64().unwrap_or(3600),
    };
    persist(&credentials).await?;
    Ok(credentials)
}
async fn persist(credentials: &Credentials) -> Result<()> {
    // A private directory also protects the temporary file during atomic replacement.
    let dir = CONFIG_DIR.join("private");
    tokio::fs::create_dir_all(&dir).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).await?;
    }
    library::atomic_write(&dir.join("115.json"), &serde_json::to_vec(&credentials)?).await?;
    Ok(())
}
pub async fn finish(pending: Pending) -> Result<()> {
    let _lock = AUTH_LOCK.lock().await;
    if pending.channel != "open" {
        let value = data(
            HTTP.post(format!(
                "https://passportapi.115.com/app/1.0/{}/1.0/login/qrcode/",
                pending.channel
            ))
            .form(&[("app", &pending.channel), ("account", &pending.uid)]),
        )
        .await?;
        let cookie = render_cookie(&value["cookie"])?;
        persist(&Credentials {
            cookie,
            channel: pending.channel,
            access_token: String::new(),
            refresh_token: String::new(),
            expires_at: 0,
        })
        .await?;
        return Ok(());
    }
    save(
        data(
            HTTP.post("https://passportapi.115.com/open/deviceCodeToToken")
                .form(&[("uid", &pending.uid), ("code_verifier", &pending.verifier)]),
        )
        .await?,
    )
    .await?;
    Ok(())
}
pub async fn authorized() -> bool {
    tokio::fs::try_exists(CONFIG_DIR.join("private/115.json"))
        .await
        .unwrap_or(false)
}
async fn token() -> Result<String> {
    let _lock = AUTH_LOCK.lock().await;
    let bytes = tokio::fs::read(CONFIG_DIR.join("private/115.json"))
        .await
        .context("请先扫码登录 115")?;
    let mut credentials: Credentials = serde_json::from_slice(&bytes)?;
    if credentials.expires_at < chrono::Utc::now().timestamp() + 120 {
        credentials = save(
            data(
                HTTP.post("https://passportapi.115.com/open/refreshToken")
                    .form(&[("refresh_token", &credentials.refresh_token)]),
            )
            .await?,
        )
        .await?;
    }
    Ok(credentials.access_token)
}
pub async fn channel() -> String {
    read_credentials().await.map(|c| c.channel).unwrap_or_default()
}
async fn read_credentials() -> Result<Credentials> {
    let bytes = tokio::fs::read(CONFIG_DIR.join("private/115.json"))
        .await
        .context("请先扫码登录 115")?;
    Ok(serde_json::from_slice(&bytes)?)
}
fn render_cookie(value: &Value) -> Result<String> {
    let mut parts = Vec::new();
    for name in ["UID", "CID", "SEID", "KID"] {
        if let Ok(v) = string(value, name) {
            ensure!(!v.contains(['\r', '\n', ';']), "115 Cookie 格式无效");
            parts.push(format!("{name}={v}"));
        } else {
            ensure!(name == "KID", "115 登录凭据不完整");
        }
    }
    Ok(parts.join("; "))
}
pub async fn check_account() -> Result<()> {
    let credentials = read_credentials().await?;
    if !credentials.cookie.is_empty() {
        data(
            HTTP.get("https://webapi.115.com/files")
                .header("Cookie", credentials.cookie)
                .query(&[("cid", "0"), ("limit", "1")]),
        )
        .await?;
        return Ok(());
    }
    data(
        HTTP.get("https://proapi.115.com/open/user/info")
            .bearer_auth(token().await?),
    )
    .await?;
    Ok(())
}
#[allow(dead_code)] // Reserved for the optional native 115 playback channel.
pub async fn download(file_id: &str, range: Option<&str>, head: bool) -> Result<reqwest::Response> {
    ensure!(
        !file_id.is_empty() && file_id.bytes().all(|b| b.is_ascii_digit()),
        "CD2 未返回可识别的 115 文件 ID"
    );
    let credentials = read_credentials().await?;
    let url = if !credentials.cookie.is_empty() {
        cookie_url(file_id, &credentials.cookie).await?
    } else {
        let token = token().await?;
        let info = data(
            HTTP.get("https://proapi.115.com/open/folder/get_info")
                .bearer_auth(&token)
                .query(&[("file_id", file_id)]),
        )
        .await?;
        let info = if info.is_array() {
            info.get(0).context("115 未找到文件")?
        } else {
            &info
        };
        let pick = string(info, "pick_code")?;
        let urls = data(
            HTTP.post("https://proapi.115.com/open/ufile/downurl")
                .bearer_auth(token)
                .header("User-Agent", UA)
                .form(&[("pick_code", pick)]),
        )
        .await?;
        let item = urls
            .as_object()
            .and_then(|v| v.get(file_id))
            .context("115 未返回该文件的播放地址")?;
        reqwest::Url::parse(&string(&item["url"], "url")?).context("115 播放地址无效")?
    };
    ensure!(
        url.scheme() == "https" || url.scheme() == "http",
        "115 播放地址协议无效"
    );
    // No OAuth credentials are forwarded to the CDN; use the same UA as downurl.
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;
    let mut request = client
        .request(
            if head {
                reqwest::Method::HEAD
            } else {
                reqwest::Method::GET
            },
            url,
        )
        .header("User-Agent", UA);
    if let Some(range) = range {
        request = request.header("Range", range);
    }
    request.send().await.map_err(|_| anyhow::anyhow!("115 视频读取失败"))
}

async fn cookie_url(file_id: &str, cookie: &str) -> Result<reqwest::Url> {
    let info = data(
        HTTP.get("https://webapi.115.com/files/get_info")
            .header("Cookie", cookie)
            .query(&[("file_id", file_id)]),
    )
    .await?;
    let info = if info.is_array() {
        info.get(0).context("115 未找到文件")?
    } else {
        &info
    };
    let pick = string(info, "pick_code").or_else(|_| string(info, "pickcode"))?;
    let key = *uuid::Uuid::new_v4().as_bytes();
    let encoded = crate::p115_cipher::encode(&serde_json::to_vec(&serde_json::json!({"pickcode":pick}))?, &key);
    let payload = data(
        HTTP.post("https://proapi.115.com/app/chrome/downurl")
            .header("Cookie", cookie)
            .header("User-Agent", UA)
            .form(&[("data", encoded)]),
    )
    .await?;
    let decoded = crate::p115_cipher::decode(payload.as_str().context("115 播放响应无效")?, &key)?;
    let files: Value = serde_json::from_slice(&decoded)?;
    let file = files.get(file_id).context("115 未返回指定文件")?;
    Ok(reqwest::Url::parse(&string(&file["url"], "url")?)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_channels_and_credentials() {
        for app in CHANNELS {
            assert!(validate_channel(app).is_ok());
        }
        assert!(validate_channel("../web").is_err());
        assert!(render_cookie(&serde_json::json!({"UID":"1","CID":"2"})).is_err());
        assert!(render_cookie(&serde_json::json!({"UID":"1","CID":"2","SEID":"x\r\n"})).is_err());
        assert_eq!(
            render_cookie(&serde_json::json!({"UID":"1","CID":"2","SEID":"3"})).unwrap(),
            "UID=1; CID=2; SEID=3"
        );
    }
}
