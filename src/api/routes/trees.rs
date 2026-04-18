//! Tree endpoints. Trees are content-addressed directory snapshots; any
//! authenticated caller may create/read them by hash.

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::api::{ApiResult, RequestContext};
use crate::auth::AuthUser;
use crate::core;
use crate::db::models::tree::{Tree, TreeEntry};

#[derive(Debug, Deserialize)]
struct TreeEntryInput {
    name: String,
    entry_type: String,
    hash: String,
}

impl From<TreeEntryInput> for TreeEntry {
    fn from(v: TreeEntryInput) -> Self {
        TreeEntry {
            name: v.name,
            entry_type: v.entry_type,
            hash: v.hash,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CreateTreeBody {
    entries: Vec<TreeEntryInput>,
}

#[derive(Debug, Serialize)]
struct TreeView {
    hash: String,
    entries: Vec<TreeEntry>,
}

impl From<Tree> for TreeView {
    fn from(t: Tree) -> Self {
        Self {
            hash: t.id,
            entries: t.entries,
        }
    }
}

#[derive(Debug, Serialize)]
struct FlatEntry {
    path: String,
    blob_hash: String,
}

#[derive(Debug, Serialize)]
struct ResolveResult {
    entry_type: String,
    hash: String,
}

#[derive(Debug, Deserialize)]
struct ResolveQuery {
    path: String,
}

async fn create(
    ctx: RequestContext,
    _auth: AuthUser,
    Json(body): Json<CreateTreeBody>,
) -> ApiResult<TreeView> {
    let entries: Vec<TreeEntry> = body.entries.into_iter().map(TreeEntry::from).collect();
    let tree = core::tree::put(entries).await?;
    Ok(ctx.respond(StatusCode::CREATED, TreeView::from(tree)))
}

async fn get_one(
    ctx: RequestContext,
    _auth: AuthUser,
    Path(hash): Path<String>,
) -> ApiResult<TreeView> {
    let tree = core::tree::get(&hash).await?;
    Ok(ctx.ok(TreeView::from(tree)))
}

async fn flatten(
    ctx: RequestContext,
    _auth: AuthUser,
    Path(hash): Path<String>,
) -> ApiResult<Vec<FlatEntry>> {
    let leaves = core::tree::flatten(&hash).await?;
    Ok(ctx.ok(leaves
        .into_iter()
        .map(|(path, blob_hash)| FlatEntry { path, blob_hash })
        .collect()))
}

async fn resolve(
    ctx: RequestContext,
    _auth: AuthUser,
    Path(hash): Path<String>,
    Query(q): Query<ResolveQuery>,
) -> ApiResult<ResolveResult> {
    let (entry_type, hash) = core::tree::resolve_path(&hash, &q.path).await?;
    Ok(ctx.ok(ResolveResult { entry_type, hash }))
}

fn register(router: Router) -> Router {
    router
        .route("/v1/trees", post(create))
        .route("/v1/trees/{hash}", get(get_one))
        .route("/v1/trees/{hash}/flat", get(flatten))
        .route("/v1/trees/{hash}/path", get(resolve))
}

crate::register_routes!(register);
