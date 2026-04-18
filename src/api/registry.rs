//! Route registration.
//!
//! Each file under `api/routes/` declares one or more routes and registers
//! them via [`crate::register_routes!`]. Registration uses the same
//! `inventory` pattern as the database models: each route file submits a
//! [`RouteModule`] describing itself, and [`build_router`] iterates every
//! submitted module at startup.
//!
//! # Adding a route
//!
//! ```ignore
//! // src/api/routes/example.rs
//! use axum::{routing::get, Router};
//! use crate::api::{ApiResult, RequestContext};
//!
//! async fn handler(ctx: RequestContext) -> ApiResult<&'static str> {
//!     Ok(ctx.ok("hello"))
//! }
//!
//! fn register(router: Router) -> Router {
//!     router.route("/v1/example", get(handler))
//! }
//!
//! crate::register_routes!(register);
//! ```
//!
//! Then add `pub mod example;` in `api/routes/mod.rs` so the registration
//! runs at link time.

use axum::{http::Uri, Router};

use crate::api::{context::RequestContext, error::ApiError};

#[derive(Clone, Copy)]
pub struct RouteModule {
    pub name: &'static str,
    pub register: fn(Router) -> Router,
}

inventory::collect!(RouteModule);

pub fn build_router() -> Router {
    let mut router = Router::new();
    for module in inventory::iter::<RouteModule> {
        router = (module.register)(router);
    }
    router.fallback(not_found)
}

async fn not_found(ctx: RequestContext, uri: Uri) -> ApiError {
    ApiError::not_found(format!("no route matches {}", uri.path())).with_context(ctx)
}

pub fn registered_modules() -> impl Iterator<Item = &'static RouteModule> {
    inventory::iter::<RouteModule>.into_iter()
}

/// Registers a route module. Accepts any `fn(axum::Router) -> axum::Router`.
#[macro_export]
macro_rules! register_routes {
    ($register_fn:path) => {
        ::inventory::submit! {
            $crate::api::registry::RouteModule {
                name: ::std::module_path!(),
                register: $register_fn,
            }
        }
    };
}
