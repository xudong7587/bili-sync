use std::sync::{Arc, LazyLock};

use axum::Router;
use axum::routing::{get, post};
use parking_lot::Mutex;
use serde::Serialize;

use crate::api::wrapper::{ApiError, ApiResponse};
use crate::cd2::{Cd2Client, QrMessage, SpaceInfo};
use crate::config::VersionedConfig;

#[derive(Clone, Default, Serialize)]
struct LoginState {
    running: bool,
    messages: Vec<QrMessage>,
    error: Option<String>,
}
static LOGIN: LazyLock<Arc<Mutex<LoginState>>> = LazyLock::new(|| Arc::new(Mutex::new(LoginState::default())));

pub fn router() -> Router {
    Router::new()
        .route("/cloud/space", get(space))
        .route("/cloud/login", post(start_login).get(login_state))
}
fn client() -> anyhow::Result<Cd2Client> {
    Cd2Client::configured(&VersionedConfig::get().read())?
        .ok_or_else(|| anyhow::anyhow!("请先保存 CD2 地址、令牌和目录"))
}
async fn space() -> Result<ApiResponse<SpaceInfo>, ApiError> {
    Ok(ApiResponse::ok(client()?.space().await?))
}
async fn login_state() -> ApiResponse<LoginState> {
    ApiResponse::ok(LOGIN.lock().clone())
}
async fn start_login() -> Result<ApiResponse<bool>, ApiError> {
    let client = client()?;
    {
        let mut state = LOGIN.lock();
        if state.running {
            return Ok(ApiResponse::ok(false));
        }
        *state = LoginState {
            running: true,
            ..Default::default()
        };
    }
    tokio::spawn(async move {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let reader = tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                let mut state = LOGIN.lock();
                if state.messages.len() < 100 {
                    state.messages.push(message);
                }
            }
        });
        let result = tokio::time::timeout(std::time::Duration::from_secs(180), client.login_115(tx)).await;
        let _ = reader.await;
        let mut state = LOGIN.lock();
        state.running = false;
        state.error = match result {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(format!("{e:#}")),
            Err(_) => Some("二维码已过期，请重新生成".into()),
        };
    });
    Ok(ApiResponse::ok(true))
}
