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

use axum::middleware as axum_mw;
use tower_http::trace::TraceLayer;

/// Builds the router, binds to `addr`, and serves until `shutdown` resolves.
pub async fn serve<F>(addr: SocketAddr, shutdown: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let router = build_router()
        .layer(axum_mw::from_fn(middleware::request_context))
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;
    tracing::info!(%local_addr, "ledger-api: listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}
