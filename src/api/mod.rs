//! HTTP API layer.
//!
//! - [`response`] and [`error`] define a single, consistent response envelope.
//! - [`context`] provides the [`RequestContext`] extractor that every handler
//!   should accept, used to build responses with request-scoped metadata.
//! - [`middleware`] installs the request context/id on every request.
//! - [`registry`] collects route modules declared under [`routes`]; each
//!   module registers itself via [`crate::register_routes!`].
//! - [`serve`] starts the HTTP server with graceful shutdown.

pub mod context;
pub mod error;
pub mod middleware;
pub mod registry;
pub mod response;
pub mod routes;

#[allow(unused_imports)]
pub use context::RequestContext;
#[allow(unused_imports)]
pub use error::ApiError;
#[allow(unused_imports)]
pub use registry::{build_router, registered_modules};
#[allow(unused_imports)]
pub use response::{ApiJson, ApiResult};

use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderName, HeaderValue, Method};
use axum::middleware as axum_mw;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;

/// Default cap on request body size (bytes). Generous enough to cover
/// base64-encoded binary blobs well above MongoDB's 16 MiB document
/// limit, so the server rejects too-large blobs at the core layer
/// (where the error message can be specific) instead of tower's
/// length-limit layer (where it's just a string).
///
/// Override with the `LEDGER_MAX_BODY_BYTES` env var.
const DEFAULT_MAX_BODY_BYTES: usize = 32 * 1024 * 1024;

/// Builds the router, binds to `addr`, and serves until `shutdown` resolves.
pub async fn serve<F>(addr: SocketAddr, shutdown: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let max_body = max_body_bytes();
    tracing::info!(max_body_bytes = max_body, "ledger-api: body limit configured");

    let router = build_router()
        .layer(DefaultBodyLimit::max(max_body))
        .layer(axum_mw::from_fn(middleware::request_context))
        .layer(build_cors())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;
    tracing::info!(%local_addr, "ledger-api: listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

/// Resolves the request body size limit from `LEDGER_MAX_BODY_BYTES`,
/// falling back to [`DEFAULT_MAX_BODY_BYTES`].
fn max_body_bytes() -> usize {
    match std::env::var("LEDGER_MAX_BODY_BYTES") {
        Ok(raw) => match raw.trim().parse::<usize>() {
            Ok(n) if n > 0 => n,
            Ok(_) => {
                tracing::warn!("LEDGER_MAX_BODY_BYTES was 0; using default");
                DEFAULT_MAX_BODY_BYTES
            }
            Err(e) => {
                tracing::warn!(error = %e, "invalid LEDGER_MAX_BODY_BYTES; using default");
                DEFAULT_MAX_BODY_BYTES
            }
        },
        Err(_) => DEFAULT_MAX_BODY_BYTES,
    }
}

/// Builds the CORS layer used by the HTTP server.
///
/// Configuration is driven by `LEDGER_CORS_ORIGIN`:
///
/// * unset or `*` → allow any origin (dev default). No credentials mode,
///   since we authenticate via `Authorization: Bearer …`, not cookies.
/// * comma-separated list of origins (e.g. `https://app.example.com,https://admin.example.com`)
///   → only those exact origins are accepted; credentials mode is
///   enabled so browsers can send cookies if you ever add them.
///
/// The layer always:
/// * allows the HTTP methods the API uses (`GET, POST, PUT, PATCH, DELETE, OPTIONS`);
/// * allows the request headers the frontend needs (`content-type`,
///   `authorization`, `x-request-id`);
/// * exposes `x-request-id` to the frontend so it can surface the
///   server-assigned ID in error reports;
/// * caches the preflight response for 10 minutes.
fn build_cors() -> CorsLayer {
    let methods = [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::PATCH,
        Method::DELETE,
        Method::OPTIONS,
    ];
    let allowed_headers = [
        header::CONTENT_TYPE,
        header::AUTHORIZATION,
        header::ACCEPT,
        HeaderName::from_static("x-request-id"),
    ];
    let exposed_headers = [HeaderName::from_static("x-request-id")];

    let raw = std::env::var("LEDGER_CORS_ORIGIN").unwrap_or_default();
    let trimmed = raw.trim();

    let mut layer = CorsLayer::new()
        .allow_methods(methods)
        .allow_headers(allowed_headers)
        .expose_headers(exposed_headers)
        .max_age(Duration::from_secs(600));

    if trimmed.is_empty() || trimmed == "*" {
        tracing::info!("cors: allowing any origin (no credentials)");
        layer = layer.allow_origin(AllowOrigin::any());
    } else {
        let origins: Vec<HeaderValue> = trimmed
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .filter_map(|s| match HeaderValue::from_str(s) {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!(origin = s, error = %e, "cors: ignoring invalid origin");
                    None
                }
            })
            .collect();

        if origins.is_empty() {
            tracing::warn!(
                "cors: LEDGER_CORS_ORIGIN was set but no origins parsed; falling back to any"
            );
            layer = layer.allow_origin(AllowOrigin::any());
        } else {
            tracing::info!(count = origins.len(), "cors: allowing explicit origin list");
            layer = layer
                .allow_origin(AllowOrigin::list(origins))
                .allow_credentials(true);
        }
    }

    layer
}
