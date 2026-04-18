//! Commit endpoints.
//!
//! Creating a commit is scoped to a repo the caller owns; reading a commit
//! by hash is available to any authenticated caller (commits are
//! content-addressed and therefore shareable).

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use super::guards::{load_owned_repo, parse_object_id};
use crate::api::{ApiResult, RequestContext};
use crate::auth::AuthUser;
use crate::core;
use crate::db::models::commit::Commit;

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
struct CreateCommitBody {
    tree: String,
    #[serde(default)]
    parents: Vec<String>,
    message: String,
}

#[derive(Debug, Deserialize, Default)]
struct LimitQuery {
    #[serde(default)]
    limit: Option<i64>,
}

async fn create_for_repo(
    ctx: RequestContext,
    auth: AuthUser,
    Path(repo_id): Path<String>,
    Json(body): Json<CreateCommitBody>,
) -> ApiResult<CommitView> {
    let repo_oid = parse_object_id(&repo_id, "repo id")?;
    load_owned_repo(&auth, repo_oid).await?;
    let commit =
        core::commit::create(repo_oid, body.tree.trim(), body.parents, body.message.trim())
            .await?;
    Ok(ctx.respond(StatusCode::CREATED, CommitView::from(commit)))
}

async fn list_for_repo(
    ctx: RequestContext,
    auth: AuthUser,
    Path(repo_id): Path<String>,
    Query(q): Query<LimitQuery>,
) -> ApiResult<Vec<CommitView>> {
    let repo_oid = parse_object_id(&repo_id, "repo id")?;
    load_owned_repo(&auth, repo_oid).await?;
    let commits = core::commit::list_for_repo(repo_oid, q.limit).await?;
    Ok(ctx.ok(commits.into_iter().map(CommitView::from).collect()))
}

async fn get_one(
    ctx: RequestContext,
    _auth: AuthUser,
    Path(hash): Path<String>,
) -> ApiResult<CommitView> {
    let commit = core::commit::get(&hash).await?;
    Ok(ctx.ok(CommitView::from(commit)))
}

async fn history(
    ctx: RequestContext,
    _auth: AuthUser,
    Path(hash): Path<String>,
    Query(q): Query<LimitQuery>,
) -> ApiResult<Vec<CommitView>> {
    let limit = q.limit.unwrap_or(100).max(1) as usize;
    let commits = core::commit::history(&hash, limit).await?;
    Ok(ctx.ok(commits.into_iter().map(CommitView::from).collect()))
}

fn register(router: Router) -> Router {
    router
        .route(
            "/v1/repos/{id}/commits",
            get(list_for_repo).post(create_for_repo),
        )
        .route("/v1/commits/{hash}", get(get_one))
        .route("/v1/commits/{hash}/history", get(history))
}

crate::register_routes!(register);
