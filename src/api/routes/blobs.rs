//! Blob endpoints. Content is transported as base64 inside the standard
//! envelope so every route in the API speaks the same JSON shape.
//!
//! POST /v1/blobs               store bytes              → { hash, size }
//! GET  /v1/blobs/{hash}        fetch bytes              → { hash, size, content_base64 }
//! GET  /v1/blobs/{hash}/meta   metadata only (no body)  → { hash, size, exists }
//! DELETE /v1/blobs/{hash}      drop a blob              → { deleted: true }

use axum::{
    extract::Path,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::{ApiError, ApiResult, RequestContext};
use crate::auth::AuthUser;
use crate::core;

#[derive(Debug, Deserialize)]
struct UploadBody {
    /// Base64-encoded content (standard alphabet, padded).
    content_base64: String,
}

#[derive(Debug, Serialize)]
struct BlobMeta {
    hash: String,
    size: i64,
}

#[derive(Debug, Serialize)]
struct BlobPayload {
    hash: String,
    size: i64,
    content_base64: String,
}

fn decode_b64(input: &str) -> Result<Vec<u8>, ApiError> {
    B64.decode(input.as_bytes())
        .map_err(|e| ApiError::bad_request(format!("content_base64 is not valid base64: {e}")))
}

async fn upload(
    ctx: RequestContext,
    _auth: AuthUser,
    Json(body): Json<UploadBody>,
) -> ApiResult<BlobMeta> {
    let bytes = decode_b64(&body.content_base64)?;
    let blob = core::blob::put(bytes).await?;
    Ok(ctx.respond(
        StatusCode::CREATED,
        BlobMeta {
            hash: blob.id,
            size: blob.size,
        },
    ))
}

async fn fetch(
    ctx: RequestContext,
    _auth: AuthUser,
    Path(hash): Path<String>,
) -> ApiResult<BlobPayload> {
    let blob = core::blob::get(&hash).await?;
    let content_base64 = B64.encode(&blob.content);
    Ok(ctx.ok(BlobPayload {
        hash: blob.id,
        size: blob.size,
        content_base64,
    }))
}

async fn meta(
    ctx: RequestContext,
    _auth: AuthUser,
    Path(hash): Path<String>,
) -> ApiResult<Value> {
    let exists = core::blob::exists(&hash).await?;
    if !exists {
        return Err(ApiError::not_found(format!("blob not found: {hash}")));
    }
    let blob = core::blob::get(&hash).await?;
    Ok(ctx.ok(json!({
        "hash": blob.id,
        "size": blob.size,
        "exists": true,
    })))
}

async fn drop(
    ctx: RequestContext,
    _auth: AuthUser,
    Path(hash): Path<String>,
) -> ApiResult<Value> {
    core::blob::delete(&hash).await?;
    Ok(ctx.ok(json!({ "deleted": true, "hash": hash })))
}

fn register(router: Router) -> Router {
    router
        .route("/v1/blobs", post(upload))
        .route("/v1/blobs/{hash}", get(fetch).delete(drop))
        .route("/v1/blobs/{hash}/meta", get(meta))
}

crate::register_routes!(register);
