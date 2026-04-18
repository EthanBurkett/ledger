//! Response envelope and JSON wrapper.
//!
//! All successful responses follow the same shape:
//!
//! ```json
//! {
//!   "meta": {
//!     "request_id": "01924f..-7b94-...",
//!     "timestamp": "2026-04-17T12:34:56.789Z",
//!     "api_version": "v1",
//!     "duration_ms": 4
//!   },
//!   "data": { ... }
//! }
//! ```
//!
//! Errors use the same envelope with an `errors` array (see [`super::error`]).

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use super::context::RequestContext;
use super::error::ApiErrorObject;

pub const API_VERSION: &str = "v1";

#[derive(Debug, Serialize)]
pub struct Meta {
    pub request_id: String,
    pub timestamp: String,
    pub api_version: &'static str,
    pub duration_ms: u128,
}

impl Meta {
    pub fn from_context(ctx: &RequestContext) -> Self {
        Self {
            request_id: ctx.request_id.clone(),
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            api_version: API_VERSION,
            duration_ms: ctx.started_at.elapsed().as_millis(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Envelope<T: Serialize> {
    pub meta: Meta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<ApiErrorObject>>,
}

/// Successful JSON response wrapper. Handlers return `ApiJson` (or
/// [`ApiResult`]) and the envelope is built automatically.
#[derive(Debug)]
pub struct ApiJson<T: Serialize> {
    ctx: RequestContext,
    status: StatusCode,
    data: T,
}

#[allow(dead_code)]
impl<T: Serialize> ApiJson<T> {
    pub fn new(ctx: RequestContext, data: T) -> Self {
        Self {
            ctx,
            status: StatusCode::OK,
            data,
        }
    }

    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }
}

impl<T: Serialize> IntoResponse for ApiJson<T> {
    fn into_response(self) -> Response {
        let meta = Meta::from_context(&self.ctx);
        let request_id = meta.request_id.clone();
        let envelope = Envelope {
            meta,
            data: Some(self.data),
            errors: None,
        };
        let mut resp = (self.status, Json(envelope)).into_response();
        if let Ok(header_val) = request_id.parse() {
            resp.headers_mut().insert("x-request-id", header_val);
        }
        resp
    }
}

/// Handler return type. Combine with the [`RequestContext`] extractor:
///
/// ```ignore
/// async fn list(ctx: RequestContext) -> ApiResult<Vec<Thing>> {
///     let things = load().await.map_err(ApiError::internal)?;
///     Ok(ctx.ok(things))
/// }
/// ```
pub type ApiResult<T> = std::result::Result<ApiJson<T>, super::error::ApiError>;
