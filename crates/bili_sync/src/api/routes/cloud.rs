use std::sync::LazyLock;

use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::api::wrapper::{ApiError, ApiResponse};
use crate::cd2::{Cd2Client, SpaceInfo};
use crate::config::VersionedConfig;
use crate::p115;

#[derive(Clone, Default, Serialize)]
struct LoginState {
    running: bool,
    authorized: bool,
    qrcode: String,
    status: String,
    error: Option<String>,
}
static LOGIN: LazyLock<Mutex<LoginState>> = LazyLock::new(|| Mutex::new(LoginState::default()));
#[derive(Deserialize)]
struct LoginRequest {
    client_id: String,
}
pub fn router() -> Router {
    Router::new()
        .route("/cloud/space", get(space))
        .route("/cloud/login", post(start_login).get(login_state))
        .route("/cloud/account/check", post(check_account))
}
async fn space() -> Result<ApiResponse<SpaceInfo>, ApiError> {
    let cd2 = Cd2Client::configured(&VersionedConfig::get().read())?
        .ok_or_else(|| anyhow::anyhow!("请先保存 CD2 地址、令牌和目录"))?;
    Ok(ApiResponse::ok(cd2.space().await?))
}
async fn check_account() -> Result<ApiResponse<bool>, ApiError> {
    p115::check_account().await?;
    Ok(ApiResponse::ok(true))
}
async fn login_state() -> ApiResponse<LoginState> {
    let authorized = p115::authorized().await;
    let mut state = LOGIN.lock().clone();
    state.authorized = authorized;
    ApiResponse::ok(state)
}
async fn start_login(Json(request): Json<LoginRequest>) -> Result<ApiResponse<bool>, ApiError> {
    {
        let mut state = LOGIN.lock();
        if state.running {
            return Ok(ApiResponse::ok(false));
        }
        *state = LoginState {
            running: true,
            status: "正在向 115 获取二维码".into(),
            ..Default::default()
        };
    }
    tokio::spawn(async move {
        let result = tokio::time::timeout(std::time::Duration::from_secs(300), async {
            let pending = p115::start(request.client_id.trim()).await?;
            LOGIN.lock().qrcode.clone_from(&pending.qrcode);
            loop {
                LOGIN.lock().status = match p115::poll(&pending).await? {
                    0 => "请使用 115 扫码".into(),
                    1 => "已扫码，请在手机确认".into(),
                    2 => break,
                    -1 => anyhow::bail!("二维码已过期，请重新生成"),
                    -2 => anyhow::bail!("已取消 115 授权"),
                    _ => "等待 115 确认".into(),
                };
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
            p115::finish(pending).await?;
            LOGIN.lock().status = "115 已登录，授权保存在 bili-sync".into();
            anyhow::Result::<()>::Ok(())
        })
        .await;
        let mut state = LOGIN.lock();
        state.running = false;
        state.qrcode.clear();
        state.error = match result {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(format!("{e:#}")),
            Err(_) => Some("二维码已过期，请重新生成".into()),
        };
    });
    Ok(ApiResponse::ok(true))
}
