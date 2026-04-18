//! Repository endpoints.
//!
//! Every route here is scoped to the authenticated caller: list/create only
//! see their own repos; get/delete/set-head 404 for repos the caller does not
//! own (identical response to a missing repo, by design).

use axum::{
    extract::Path,
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::guards::{auth_user_id, load_owned_repo, parse_object_id};
use crate::api::{ApiResult, RequestContext};
use crate::auth::AuthUser;
use crate::core;
use crate::db::models::repo::Repo;

#[derive(Debug, Serialize)]
struct RepoView {
    id: String,
    owner_id: String,
    name: String,
    head_commit: Option<String>,
}

impl From<Repo> for RepoView {
    fn from(r: Repo) -> Self {
        Self {
            id: r.id.map(|o| o.to_hex()).unwrap_or_default(),
            owner_id: r.owner_id.to_hex(),
            name: r.name,
            head_commit: r.head_commit,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CreateRepoBody {
    name: String,
}

#[derive(Debug, Deserialize)]
struct SetHeadBody {
    commit: String,
}

async fn list(ctx: RequestContext, auth: AuthUser) -> ApiResult<Vec<RepoView>> {
    let owner = auth_user_id(&auth)?;
    let repos = core::repo::list_for_owner(owner).await?;
    Ok(ctx.ok(repos.into_iter().map(RepoView::from).collect()))
}

async fn create(
    ctx: RequestContext,
    auth: AuthUser,
    Json(body): Json<CreateRepoBody>,
) -> ApiResult<RepoView> {
    let owner = auth_user_id(&auth)?;
    let repo = core::repo::create(owner, &body.name).await?;
    Ok(ctx.respond(StatusCode::CREATED, RepoView::from(repo)))
}

async fn get_one(
    ctx: RequestContext,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<RepoView> {
    let oid = parse_object_id(&id, "repo id")?;
    let repo = load_owned_repo(&auth, oid).await?;
    Ok(ctx.ok(RepoView::from(repo)))
}

async fn delete_one(
    ctx: RequestContext,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    let oid = parse_object_id(&id, "repo id")?;
    load_owned_repo(&auth, oid).await?;
    core::repo::delete(oid).await?;
    Ok(ctx.ok(json!({ "deleted": true, "id": oid.to_hex() })))
}

async fn set_head(
    ctx: RequestContext,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<SetHeadBody>,
) -> ApiResult<RepoView> {
    let oid = parse_object_id(&id, "repo id")?;
    load_owned_repo(&auth, oid).await?;
    core::repo::set_head(oid, body.commit.trim()).await?;
    let refreshed = core::repo::get(oid).await?;
    Ok(ctx.ok(RepoView::from(refreshed)))
}

async fn clear_head(
    ctx: RequestContext,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<RepoView> {
    let oid = parse_object_id(&id, "repo id")?;
    load_owned_repo(&auth, oid).await?;
    core::repo::clear_head(oid).await?;
    let refreshed = core::repo::get(oid).await?;
    Ok(ctx.ok(RepoView::from(refreshed)))
}

fn register(router: Router) -> Router {
    router
        .route("/v1/repos", get(list).post(create))
        .route("/v1/repos/{id}", get(get_one).delete(delete_one))
        .route("/v1/repos/{id}/head", put(set_head).delete(clear_head))
}

crate::register_routes!(register);
