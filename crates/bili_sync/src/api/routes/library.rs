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
    let Ok(Some(mut receipt)) = library::load(video_id, cid).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // These independent 128-bit file tokens do not grant access to settings or other files.
    if !receipt.cloud || receipt.playback_token != token {
        return StatusCode::NOT_FOUND.into_response();
    }
    let config = VersionedConfig::get().snapshot();
    let Ok(Some(cd2)) = Cd2Client::connection(&config) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let range = headers.get("range").and_then(|v| v.to_str().ok());
    let result = if crate::p115::authorized().await {
        if receipt.cloud_file_id.is_none()
            && let Ok(id) = cd2.file_id(&receipt.storage_path).await
        {
            receipt.cloud_file_id = Some(id);
        }
        match receipt.cloud_file_id.as_deref() {
            Some(id) => crate::p115::download(id, range, method == Method::HEAD).await,
            None => cd2.download(&receipt.storage_path, range, method == Method::HEAD).await,
        }
    } else {
        cd2.download(&receipt.storage_path, range, method == Method::HEAD).await
    };
    let response = match result {
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

#[derive(serde::Deserialize)]
struct LibraryQuery {
    favorite: Option<i32>,
    watch_later: Option<i32>,
    submission: Option<i32>,
    collection: Option<i32>,
    query: Option<String>,
    page: Option<u64>,
}
#[derive(serde::Serialize)]
struct LibraryRow {
    id: i32,
    video_id: i32,
    bvid: String,
    title: String,
    part: String,
    favorite_time: String,
    downloaded: bool,
    metadata_path: Option<String>,
    storage_path: Option<String>,
    storage: String,
    quality: Option<library::SavedQuality>,
    comparison: Option<crate::quality::Comparison>,
}

pub fn router() -> Router {
    Router::new()
        .route("/library/videos", get(list))
        .route("/library/jobs", axum::routing::post(start_job).get(job_status))
}
async fn list(
    axum::extract::Extension(db): axum::extract::Extension<sea_orm::DatabaseConnection>,
    axum::extract::Query(query): axum::extract::Query<LibraryQuery>,
) -> Result<crate::api::wrapper::ApiResponse<serde_json::Value>, crate::api::wrapper::ApiError> {
    use bili_sync_entity::{page, video};
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};
    let mut videos = video::Entity::find();
    for (id, column) in [
        (query.favorite, video::Column::FavoriteId),
        (query.watch_later, video::Column::WatchLaterId),
        (query.collection, video::Column::CollectionId),
        (query.submission, video::Column::SubmissionId),
    ] {
        if let Some(id) = id {
            videos = videos.filter(column.eq(id));
        }
    }
    if let Some(word) = query.query.filter(|q| !q.is_empty()) {
        videos = videos.filter(video::Column::Name.contains(word));
    }
    let pages = videos.order_by_desc(video::Column::Favtime).paginate(&db, 25);
    let total = pages.num_items().await?;
    let mut rows = Vec::new();
    for video in pages.fetch_page(query.page.unwrap_or(0)).await? {
        let parts = page::Entity::find()
            .filter(page::Column::VideoId.eq(video.id))
            .order_by_asc(page::Column::Pid)
            .all(&db)
            .await?;
        if parts.is_empty() {
            rows.push(LibraryRow {
                id: -video.id,
                video_id: video.id,
                bvid: video.bvid.clone(),
                title: video.name.clone(),
                part: "尚未获取分 P 信息".into(),
                favorite_time: video.favtime.to_string(),
                downloaded: false,
                metadata_path: Some(video.path.clone()),
                storage_path: None,
                storage: "unknown".into(),
                quality: None,
                comparison: None,
            });
        }
        for part in parts {
            let receipt = library::load(video.id, part.cid).await?;
            let downloaded = ((part.download_status >> 3) & 7) == 7;
            rows.push(LibraryRow {
                id: part.id,
                video_id: video.id,
                bvid: video.bvid.clone(),
                title: video.name.clone(),
                part: part.name,
                favorite_time: video.favtime.to_string(),
                downloaded,
                metadata_path: part.path,
                storage_path: receipt.as_ref().map(|r| r.storage_path.clone()),
                storage: receipt
                    .as_ref()
                    .map(|r| if r.cloud { "115" } else { "local" })
                    .unwrap_or("unknown")
                    .to_owned(),
                quality: receipt.map(|r| r.quality),
                comparison: crate::quality::load_comparison(part.id).await?,
            });
        }
    }
    Ok(crate::api::wrapper::ApiResponse::ok(
        serde_json::json!({ "rows": rows, "total": total }),
    ))
}
async fn start_job(
    axum::extract::Extension(db): axum::extract::Extension<sea_orm::DatabaseConnection>,
    axum::extract::Extension(client): axum::extract::Extension<std::sync::Arc<crate::bilibili::BiliClient>>,
    axum::Json(request): axum::Json<crate::quality::BatchRequest>,
) -> Result<crate::api::wrapper::ApiResponse<bool>, crate::api::wrapper::ApiError> {
    crate::quality::start(db, client, request).await?;
    Ok(crate::api::wrapper::ApiResponse::ok(true))
}
async fn job_status() -> crate::api::wrapper::ApiResponse<crate::quality::JobState> {
    crate::api::wrapper::ApiResponse::ok(crate::quality::status())
}
