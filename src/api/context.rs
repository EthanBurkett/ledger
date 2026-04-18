//! Per-request context extractor.
//!
//! A `RequestContext` is attached to every incoming request by the
//! request-id middleware and is the canonical source of request metadata
//! (ID, start time). Handlers take it as an extractor:
//!
//! ```ignore
//! async fn handler(ctx: RequestContext) -> ApiResult<MyType> {
//!     Ok(ctx.ok(my_type))
//! }
//! ```

use std::convert::Infallible;
use std::time::Instant;

use axum::{extract::FromRequestParts, http::request::Parts};
use serde::Serialize;
use uuid::Uuid;

use super::response::ApiJson;

#[derive(Debug, Clone)]
pub struct RequestContext {
    pub request_id: String,
    pub started_at: Instant,
}

#[allow(dead_code)]
impl RequestContext {
    pub fn new() -> Self {
        Self {
            request_id: Uuid::now_v7().to_string(),
            started_at: Instant::now(),
        }
    }

    /// Context used when an error is rendered outside a real request (e.g.
    /// a panic recovery response).
    pub fn orphan() -> Self {
        Self::new()
    }

    /// Build a success response for the given body.
    pub fn ok<T: Serialize>(&self, data: T) -> ApiJson<T> {
        ApiJson::new(self.clone(), data)
    }

    /// Build a success response with a specific HTTP status, e.g. `201 Created`.
    pub fn respond<T: Serialize>(&self, status: axum::http::StatusCode, data: T) -> ApiJson<T> {
        ApiJson::new(self.clone(), data).with_status(status)
    }
}

impl Default for RequestContext {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> FromRequestParts<S> for RequestContext
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(existing) = parts.extensions.get::<RequestContext>() {
            return Ok(existing.clone());
        }
        let ctx = RequestContext::new();
        parts.extensions.insert(ctx.clone());
        Ok(ctx)
    }
}
