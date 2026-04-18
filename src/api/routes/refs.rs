//! Per-repo refs (branches/tags) endpoints.

use axum::{
    extract::Path,
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::guards::{load_owned_repo, parse_object_id};
use crate::api::{ApiResult, RequestContext};
use crate::auth::AuthUser;
use crate::core;
use crate::db::models::r#ref::Ref;

#[derive(Debug, Serialize)]
struct RefView {
    id: String,
    repo_id: String,
    name: String,
    commit: String,
}

impl From<Ref> for RefView {
    fn from(r: Ref) -> Self {
        Self {
            id: r.id.map(|o| o.to_hex()).unwrap_or_default(),
            repo_id: r.repo_id.to_hex(),
            name: r.name,
            commit: r.commit,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CreateRefBody {
    name: String,
    commit: String,
}

#[derive(Debug, Deserialize)]
struct UpdateRefBody {
    commit: String,
}

async fn list(
    ctx: RequestContext,
    auth: AuthUser,
    Path(repo_id): Path<String>,
) -> ApiResult<Vec<RefView>> {
    let repo_oid = parse_object_id(&repo_id, "repo id")?;
    load_owned_repo(&auth, repo_oid).await?;
    let refs = core::repo::refs::list(repo_oid).await?;
    Ok(ctx.ok(refs.into_iter().map(RefView::from).collect()))
}

async fn create(
    ctx: RequestContext,
    auth: AuthUser,
    Path(repo_id): Path<String>,
    Json(body): Json<CreateRefBody>,
) -> ApiResult<RefView> {
    let repo_oid = parse_object_id(&repo_id, "repo id")?;
    load_owned_repo(&auth, repo_oid).await?;
    let r = core::repo::refs::create(repo_oid, body.name.trim(), body.commit.trim()).await?;
    Ok(ctx.respond(StatusCode::CREATED, RefView::from(r)))
}

async fn get_one(
    ctx: RequestContext,
    auth: AuthUser,
    Path((repo_id, name)): Path<(String, String)>,
) -> ApiResult<RefView> {
    let repo_oid = parse_object_id(&repo_id, "repo id")?;
    load_owned_repo(&auth, repo_oid).await?;
    let r = core::repo::refs::get(repo_oid, &name).await?;
    Ok(ctx.ok(RefView::from(r)))
}

async fn update(
    ctx: RequestContext,
    auth: AuthUser,
    Path((repo_id, name)): Path<(String, String)>,
    Json(body): Json<UpdateRefBody>,
) -> ApiResult<RefView> {
    let repo_oid = parse_object_id(&repo_id, "repo id")?;
    load_owned_repo(&auth, repo_oid).await?;
    core::repo::refs::update(repo_oid, &name, body.commit.trim()).await?;
    let refreshed = core::repo::refs::get(repo_oid, &name).await?;
    Ok(ctx.ok(RefView::from(refreshed)))
}

async fn delete_one(
    ctx: RequestContext,
    auth: AuthUser,
    Path((repo_id, name)): Path<(String, String)>,
) -> ApiResult<Value> {
    let repo_oid = parse_object_id(&repo_id, "repo id")?;
    load_owned_repo(&auth, repo_oid).await?;
    core::repo::refs::delete(repo_oid, &name).await?;
    Ok(ctx.ok(json!({ "deleted": true, "name": name })))
}

fn register(router: Router) -> Router {
    router
        .route("/v1/repos/{id}/refs", get(list).post(create))
        .route(
            "/v1/repos/{id}/refs/{name}",
            get(get_one).patch(update).delete(delete_one),
        )
}

crate::register_routes!(register);
