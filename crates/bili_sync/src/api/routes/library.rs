use axum::Router;
use axum::body::Body;
use axum::extract::Path;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::cd2::Cd2Client;
use crate::config::VersionedConfig;
use crate::library;

pub fn playback_router() -> Router {
    Router::new().route("/stream/{video_id}/{cid}/{token}", get(play))
}

async fn play(Path((video_id, cid, token)): Path<(i32, i64, String)>, headers: HeaderMap, method: Method) -> Response {
    let Ok(Some(receipt)) = library::load(video_id, cid).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // These independent 128-bit file tokens do not grant access to settings or other files.
    if !receipt.cloud || receipt.playback_token != token {
        return StatusCode::NOT_FOUND.into_response();
    }
    let config = VersionedConfig::get().snapshot();
    let Ok(Some(cd2)) = Cd2Client::configured(&config) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let range = headers.get("range").and_then(|v| v.to_str().ok());
    let response = match cd2.download(&receipt.storage_path, range, method == Method::HEAD).await {
        Ok(response) => response,
        Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
    };
    let status = response.status();
    if !status.is_success() && status != StatusCode::RANGE_NOT_SATISFIABLE {
        return StatusCode::BAD_GATEWAY.into_response();
    }
    let mut builder = Response::builder().status(status);
    for name in [
        "content-type",
        "content-length",
        "content-range",
        "accept-ranges",
        "etag",
        "last-modified",
    ] {
        if let Some(value) = response.headers().get(name) {
            builder = builder.header(name, value);
        }
    }
    builder
        .body(Body::from_stream(response.bytes_stream()))
        .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}
