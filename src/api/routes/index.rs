//! Per-repo staging-index endpoints.
//!
//! GET    /v1/repos/{id}/index              list staged entries
//! POST   /v1/repos/{id}/index              stage { path, blob_hash }
//! DELETE /v1/repos/{id}/index?path=…       unstage a single path
//! DELETE /v1/repos/{id}/index/all          empty the staging index
//! POST   /v1/repos/{id}/index/commit       turn staged changes into a commit

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::guards::{load_owned_repo, parse_object_id};
use crate::api::{ApiResult, RequestContext};
use crate::auth::AuthUser;
use crate::core;
use crate::db::models::commit::Commit;
use crate::db::models::index::{Index, IndexEntry};

#[derive(Debug, Serialize)]
struct IndexView {
    id: Option<String>,
    repo_id: String,
    entries: Vec<IndexEntry>,
}

impl From<Index> for IndexView {
    fn from(i: Index) -> Self {
        Self {
            id: i.id.map(|o| o.to_hex()),
            repo_id: i.repo_id.to_hex(),
            entries: i.entries,
        }
    }
}

#[derive(Debug, Serialize)]
struct CommitView {
    hash: String,
    repo_id: String,
    tree: String,
    parents: Vec<String>,
    message: String,
    timestamp: i64,
}

impl From<Commit> for CommitView {
    fn from(c: Commit) -> Self {
        Self {
            hash: c.id,
            repo_id: c.repo_id.to_hex(),
            tree: c.tree,
            parents: c.parents,
            message: c.message,
            timestamp: c.timestamp,
        }
    }
}

#[derive(Debug, Deserialize)]
struct StageBody {
    path: String,
    blob_hash: String,
}

#[derive(Debug, Deserialize)]
struct UnstageQuery {
    path: String,
}

#[derive(Debug, Deserialize)]
struct CommitStagedBody {
    message: String,
}

async fn show(
    ctx: RequestContext,
    auth: AuthUser,
    Path(repo_id): Path<String>,
) -> ApiResult<IndexView> {
    let repo_oid = parse_object_id(&repo_id, "repo id")?;
    load_owned_repo(&auth, repo_oid).await?;
    let idx = core::index::get(repo_oid).await?;
    Ok(ctx.ok(IndexView::from(idx)))
}

async fn stage(
    ctx: RequestContext,
    auth: AuthUser,
    Path(repo_id): Path<String>,
    Json(body): Json<StageBody>,
) -> ApiResult<IndexView> {
    let repo_oid = parse_object_id(&repo_id, "repo id")?;
    load_owned_repo(&auth, repo_oid).await?;
    core::index::stage(repo_oid, &body.path, body.blob_hash.trim()).await?;
    let idx = core::index::get(repo_oid).await?;
    Ok(ctx.respond(StatusCode::ACCEPTED, IndexView::from(idx)))
}

async fn unstage(
    ctx: RequestContext,
    auth: AuthUser,
    Path(repo_id): Path<String>,
    Query(q): Query<UnstageQuery>,
) -> ApiResult<IndexView> {
    let repo_oid = parse_object_id(&repo_id, "repo id")?;
    load_owned_repo(&auth, repo_oid).await?;
    core::index::unstage(repo_oid, &q.path).await?;
    let idx = core::index::get(repo_oid).await?;
    Ok(ctx.ok(IndexView::from(idx)))
}

async fn clear(
    ctx: RequestContext,
    auth: AuthUser,
    Path(repo_id): Path<String>,
) -> ApiResult<Value> {
    let repo_oid = parse_object_id(&repo_id, "repo id")?;
    load_owned_repo(&auth, repo_oid).await?;
    core::index::clear(repo_oid).await?;
    Ok(ctx.ok(json!({ "cleared": true })))
}

async fn commit_staged(
    ctx: RequestContext,
    auth: AuthUser,
    Path(repo_id): Path<String>,
    Json(body): Json<CommitStagedBody>,
) -> ApiResult<CommitView> {
    let repo_oid = parse_object_id(&repo_id, "repo id")?;
    load_owned_repo(&auth, repo_oid).await?;
    let commit = core::index::commit_staged(repo_oid, &body.message).await?;
    Ok(ctx.respond(StatusCode::CREATED, CommitView::from(commit)))
}

fn register(router: Router) -> Router {
    router
        .route(
            "/v1/repos/{id}/index",
            get(show).post(stage).delete(unstage),
        )
        .route("/v1/repos/{id}/index/all", delete(clear))
        .route("/v1/repos/{id}/index/commit", post(commit_staged))
}

crate::register_routes!(register);
