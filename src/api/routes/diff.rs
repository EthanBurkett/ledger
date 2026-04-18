//! Diff endpoints.

use axum::{
    extract::Query,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};

use crate::api::{ApiResult, RequestContext};
use crate::auth::AuthUser;
use crate::core;
use crate::core::diff::{Change, ChangeKind};

#[derive(Debug, Deserialize)]
struct DiffQuery {
    /// Hash of the "before" side. Empty string means "empty tree/commit".
    #[serde(default)]
    left: String,
    right: String,
}

#[derive(Debug, Serialize)]
struct ChangeView {
    path: String,
    kind: ChangeKind,
    old_hash: Option<String>,
    new_hash: Option<String>,
}

impl From<Change> for ChangeView {
    fn from(c: Change) -> Self {
        Self {
            path: c.path,
            kind: c.kind,
            old_hash: c.old_hash,
            new_hash: c.new_hash,
        }
    }
}

async fn diff_trees(
    ctx: RequestContext,
    _auth: AuthUser,
    Query(q): Query<DiffQuery>,
) -> ApiResult<Vec<ChangeView>> {
    let changes = core::diff::trees(&q.left, &q.right).await?;
    Ok(ctx.ok(changes.into_iter().map(ChangeView::from).collect()))
}

async fn diff_commits(
    ctx: RequestContext,
    _auth: AuthUser,
    Query(q): Query<DiffQuery>,
) -> ApiResult<Vec<ChangeView>> {
    let changes = core::diff::commits(&q.left, &q.right).await?;
    Ok(ctx.ok(changes.into_iter().map(ChangeView::from).collect()))
}

fn register(router: Router) -> Router {
    router
        .route("/v1/diff/trees", get(diff_trees))
        .route("/v1/diff/commits", get(diff_commits))
}

crate::register_routes!(register);
