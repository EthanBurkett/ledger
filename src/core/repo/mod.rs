//! Repositories: the top-level container. Each repo has a HEAD commit
//! (its current tip) and zero or more named refs (branches/tags).

pub mod refs;

use mongodb::bson::{doc, oid::ObjectId};

use crate::core::{CoreError, CoreResult};
use crate::db::models::repo::Repo;
use crate::db::MongoModel;

const NAME_MAX: usize = 128;

fn validate_name(name: &str) -> CoreResult<()> {
    if name.is_empty() || name.len() > NAME_MAX {
        return Err(CoreError::Validation(format!(
            "repo name must be 1-{NAME_MAX} characters"
        )));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))
    {
        return Err(CoreError::Validation(
            "repo name may only contain letters, digits, and '_' '-' '.' '/'".into(),
        ));
    }
    Ok(())
}

/// Creates a new repo owned by `owner_id`. Name uniqueness is enforced
/// globally (matches the `unique("name")` index on the model).
pub async fn create(owner_id: ObjectId, name: &str) -> CoreResult<Repo> {
    let name = name.trim();
    validate_name(name)?;

    let repos = Repo::repository();
    if repos
        .find_one(doc! { "name": name })
        .await?
        .is_some()
    {
        return Err(CoreError::Conflict(format!(
            "repo '{name}' already exists"
        )));
    }

    let mut repo = Repo {
        id: None,
        owner_id,
        name: name.to_string(),
        head_commit: None,
    };
    let result = repos.insert_one(&repo).await?;
    repo.id = result.inserted_id.as_object_id();
    Ok(repo)
}

pub async fn get(id: ObjectId) -> CoreResult<Repo> {
    Repo::repository()
        .find_by_id(id)
        .await?
        .ok_or_else(|| CoreError::NotFound {
            entity: "repo",
            id: id.to_hex(),
        })
}

pub async fn get_by_name(name: &str) -> CoreResult<Repo> {
    Repo::repository()
        .find_one(doc! { "name": name })
        .await?
        .ok_or_else(|| CoreError::NotFound {
            entity: "repo",
            id: name.to_string(),
        })
}

pub async fn list() -> CoreResult<Vec<Repo>> {
    let mut repos = Repo::repository().find(doc! {}).await?;
    repos.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(repos)
}

pub async fn list_for_owner(owner_id: ObjectId) -> CoreResult<Vec<Repo>> {
    let mut repos = Repo::repository()
        .find(doc! { "owner_id": owner_id })
        .await?;
    repos.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(repos)
}

/// Deletes the repo document. Does **not** cascade-delete refs, commits,
/// or the staging index — those are left behind intentionally so content
/// can be recovered or GC'd separately.
pub async fn delete(id: ObjectId) -> CoreResult<()> {
    let result = Repo::repository().delete_by_id(id).await?;
    if result.deleted_count == 0 {
        return Err(CoreError::NotFound {
            entity: "repo",
            id: id.to_hex(),
        });
    }
    Ok(())
}

/// Advances the repo's HEAD to the given commit hash.
pub async fn set_head(id: ObjectId, commit: &str) -> CoreResult<()> {
    if commit.is_empty() {
        return Err(CoreError::Validation("commit hash must not be empty".into()));
    }
    let result = Repo::repository()
        .update_by_id(id, doc! { "$set": { "head_commit": commit } })
        .await?;
    if result.matched_count == 0 {
        return Err(CoreError::NotFound {
            entity: "repo",
            id: id.to_hex(),
        });
    }
    Ok(())
}

pub async fn clear_head(id: ObjectId) -> CoreResult<()> {
    let result = Repo::repository()
        .update_by_id(id, doc! { "$set": { "head_commit": mongodb::bson::Bson::Null } })
        .await?;
    if result.matched_count == 0 {
        return Err(CoreError::NotFound {
            entity: "repo",
            id: id.to_hex(),
        });
    }
    Ok(())
}
