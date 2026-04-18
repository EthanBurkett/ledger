//! Uniform error type for API handlers.
//!
//! Handlers return [`ApiResult<T>`](super::response::ApiResult); any `Err`
//! is rendered as an envelope with an `errors` array:
//!
//! ```json
//! {
//!   "meta": { ... },
//!   "errors": [
//!     {
//!       "status": 422,
//!       "code": "validation_failed",
//!       "title": "Request validation failed",
//!       "detail": "field 'name' is required",
//!       "source": { "pointer": "/name" }
//!     }
//!   ]
//! }
//! ```

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::Value;

use super::context::RequestContext;
use super::response::{Envelope, Meta};

#[derive(Debug, Serialize, Clone)]
pub struct ApiErrorObject {
    pub status: u16,
    pub code: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Value>,
}

/// Rich API error. Use the constructors; most handlers will call
/// [`ApiError::internal`]/[`ApiError::bad_request`]/etc., or use `?` with
/// `ApiError::from(e)` on a `mongodb::error::Error`.
#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: String,
    pub title: String,
    pub detail: Option<String>,
    pub source: Option<Value>,
    /// When set, overrides the request_id pulled from extensions.
    pub ctx: Option<RequestContext>,
}

#[allow(dead_code)]
impl ApiError {
    fn new(status: StatusCode, code: &str, title: &str) -> Self {
        Self {
            status,
            code: code.to_string(),
            title: title.to_string(),
            detail: None,
            source: None,
            ctx: None,
        }
    }

    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", "Bad request")
            .with_detail(detail)
    }

    pub fn unauthorized(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", "Unauthorized")
            .with_detail(detail)
    }

    pub fn forbidden(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", "Forbidden").with_detail(detail)
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", "Resource not found")
            .with_detail(detail)
    }

    pub fn conflict(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", "Conflict").with_detail(detail)
    }

    pub fn validation(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_failed",
            "Request validation failed",
        )
        .with_detail(detail)
    }

    pub fn internal(err: impl std::fmt::Display) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Internal server error",
        )
        .with_detail(err.to_string())
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_source(mut self, source: Value) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_context(mut self, ctx: RequestContext) -> Self {
        self.ctx = Some(ctx);
        self
    }

    fn to_object(&self) -> ApiErrorObject {
        ApiErrorObject {
            status: self.status.as_u16(),
            code: self.code.clone(),
            title: self.title.clone(),
            detail: self.detail.clone(),
            source: self.source.clone(),
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.detail {
            Some(d) => write!(f, "{} ({}): {}", self.title, self.code, d),
            None => write!(f, "{} ({})", self.title, self.code),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<mongodb::error::Error> for ApiError {
    fn from(err: mongodb::error::Error) -> Self {
        ApiError::internal(err).with_code("database_error", "Database error")
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::bad_request(err.to_string()).with_code("invalid_json", "Malformed JSON body")
    }
}

impl ApiError {
    fn with_code(mut self, code: &str, title: &str) -> Self {
        self.code = code.to_string();
        self.title = title.to_string();
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let ctx = self.ctx.clone().unwrap_or_else(RequestContext::orphan);
        let meta = Meta::from_context(&ctx);
        let request_id = meta.request_id.clone();

        if self.status.is_server_error() {
            tracing::error!(
                request_id = %request_id,
                code = %self.code,
                status = self.status.as_u16(),
                detail = self.detail.as_deref().unwrap_or(""),
                "api error"
            );
        } else {
            tracing::debug!(
                request_id = %request_id,
                code = %self.code,
                status = self.status.as_u16(),
                detail = self.detail.as_deref().unwrap_or(""),
                "api error"
            );
        }

        let envelope = Envelope::<()> {
            meta,
            data: None,
            errors: Some(vec![self.to_object()]),
        };

        let mut resp = (self.status, Json(envelope)).into_response();
        if let Ok(header_val) = request_id.parse() {
            resp.headers_mut().insert("x-request-id", header_val);
        }
        resp
    }
}
