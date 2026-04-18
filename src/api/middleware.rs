//! Per-request middleware.
//!
//! `request_context` installs a [`RequestContext`] on every incoming request
//! (reusing a client-supplied `x-request-id` header when present) and stamps
//! the response with the same `x-request-id`. All handlers and the error
//! envelope observe the same ID, which makes request tracing trivial.

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};

use super::context::RequestContext;

const REQUEST_ID_HEADER: &str = "x-request-id";

pub async fn request_context(mut req: Request, next: Next) -> Response {
    let incoming = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);

    let mut ctx = RequestContext::new();
    if let Some(id) = incoming {
        if !id.is_empty() && id.len() <= 128 {
            ctx.request_id = id;
        }
    }

    req.extensions_mut().insert(ctx.clone());

    let mut response = next.run(req).await;
    if let Ok(value) = HeaderValue::from_str(&ctx.request_id) {
        response.headers_mut().insert(
            HeaderName::from_static(REQUEST_ID_HEADER),
            value,
        );
    }
    response
}
