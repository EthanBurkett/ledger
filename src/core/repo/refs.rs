//! Named refs (branches / tags) pointing at commits within a repo.

use mongodb::bson::{doc, oid::ObjectId};

use crate::core::{errors::is_duplicate_key, CoreError, CoreResult};
use crate::db::models::r#ref::Ref;
use crate::db::MongoModel;

const REF_NAME_MAX: usize = 128;

fn validate_name(name: &str) -> CoreResult<()> {
    if name.is_empty() || name.len() > REF_NAME_MAX {
        return Err(CoreError::Validation(format!(
            "ref name must be 1-{REF_NAME_MAX} characters"
        )));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))
    {
        return Err(CoreError::Validation(
            "ref name may only contain letters, digits, and '_' '-' '.' '/'".into(),
        ));
    }
    Ok(())
}

/// Creates a new ref. Fails with [`CoreError::Conflict`] if `(repo_id, name)`
/// already exists.
pub async fn create(repo_id: ObjectId, name: &str, commit: &str) -> CoreResult<Ref> {
    let name = name.trim();
    validate_name(name)?;
    if commit.is_empty() {
        return Err(CoreError::Validation("ref commit must not be empty".into()));
    }

    let mut r = Ref {
        id: None,
        repo_id,
        name: name.to_string(),
        commit: commit.to_string(),
    };

    match Ref::repository().insert_one(&r).await {
        Ok(res) => {
            r.id = res.inserted_id.as_object_id();
            Ok(r)
        }
        Err(e) if is_duplicate_key(&e) => Err(CoreError::Conflict(format!(
            "ref '{name}' already exists for this repo"
        ))),
        Err(e) => Err(e.into()),
    }
}

pub async fn get(repo_id: ObjectId, name: &str) -> CoreResult<Ref> {
    Ref::repository()
        .find_one(doc! { "repo_id": repo_id, "name": name })
        .await?
        .ok_or_else(|| CoreError::NotFound {
            entity: "ref",
            id: format!("{}:{name}", repo_id.to_hex()),
        })
}

pub async fn list(repo_id: ObjectId) -> CoreResult<Vec<Ref>> {
    let mut refs = Ref::repository()
        .find(doc! { "repo_id": repo_id })
        .await?;
    refs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(refs)
}

/// Moves an existing ref to a new commit. Errors if the ref does not exist.
pub async fn update(repo_id: ObjectId, name: &str, commit: &str) -> CoreResult<()> {
    if commit.is_empty() {
        return Err(CoreError::Validation("ref commit must not be empty".into()));
    }
    let result = Ref::repository()
        .update_one(
            doc! { "repo_id": repo_id, "name": name },
            doc! { "$set": { "commit": commit } },
        )
        .await?;
    if result.matched_count == 0 {
        return Err(CoreError::NotFound {
            entity: "ref",
            id: format!("{}:{name}", repo_id.to_hex()),
        });
    }
    Ok(())
}

pub async fn delete(repo_id: ObjectId, name: &str) -> CoreResult<()> {
    let result = Ref::repository()
        .delete_one(doc! { "repo_id": repo_id, "name": name })
        .await?;
    if result.deleted_count == 0 {
        return Err(CoreError::NotFound {
            entity: "ref",
            id: format!("{}:{name}", repo_id.to_hex()),
        });
    }
    Ok(())
}
