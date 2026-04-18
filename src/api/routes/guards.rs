//! Small helpers shared by every route module.
//!
//! Keeps path-param parsing, owner checks, and ObjectId coercion in one
//! place so individual route files stay focused on their own shape.

use mongodb::bson::oid::ObjectId;

use crate::api::ApiError;
use crate::auth::AuthUser;
use crate::core;
use crate::db::models::repo::Repo;

/// Parses a hex ObjectId from a path/query parameter. Returns a 400 on failure.
pub(crate) fn parse_object_id(raw: &str, label: &str) -> Result<ObjectId, ApiError> {
    ObjectId::parse_str(raw).map_err(|_| {
        ApiError::bad_request(format!("{label} must be a valid ObjectId; got {raw:?}"))
            .with_source(serde_json::json!({ "parameter": label }))
    })
}

/// Extracts the authenticated user's ObjectId, or 500 if it's missing
/// (should never happen; the extractor loads a persisted user).
pub(crate) fn auth_user_id(auth: &AuthUser) -> Result<ObjectId, ApiError> {
    auth.user
        .id
        .ok_or_else(|| ApiError::internal("authenticated user is missing an id"))
}

/// Loads a repo by id and verifies the authenticated user owns it.
///
/// Returns `ApiError::not_found` when either the repo doesn't exist **or**
/// the caller isn't its owner — keeping the two indistinguishable hides the
/// existence of other users' repos.
pub(crate) async fn load_owned_repo(
    auth: &AuthUser,
    repo_id: ObjectId,
) -> Result<Repo, ApiError> {
    let owner = auth_user_id(auth)?;
    let repo = core::repo::get(repo_id).await?;
    if repo.owner_id != owner {
        return Err(ApiError::not_found(format!("repo {}", repo_id.to_hex())));
    }
    Ok(repo)
}
