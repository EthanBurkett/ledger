use axum::{routing::get, Router};
use serde::Serialize;

use crate::api::{ApiResult, RequestContext};

#[derive(Serialize)]
struct Health {
    status: &'static str,
    service: &'static str,
    version: &'static str,
}

async fn health(ctx: RequestContext) -> ApiResult<Health> {
    Ok(ctx.ok(Health {
        status: "ok",
        service: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
    }))
}

fn register(router: Router) -> Router {
    router.route("/v1/health", get(health))
}

crate::register_routes!(register);
