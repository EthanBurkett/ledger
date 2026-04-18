//! Staging index: per-repo pending changes waiting to become a commit.
//!
//! Each repo has at most one index document (`unique(repo_id)`). Entries are
//! flat `(path, blob_hash)` pairs. [`commit_staged`] turns the flat index
//! into a tree of trees and ships it as a new commit, advancing HEAD.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use mongodb::bson::{doc, oid::ObjectId, to_bson};

use crate::core::{CoreError, CoreResult};
use crate::db::models::commit::Commit;
use crate::db::models::index::{Index, IndexEntry};
use crate::db::models::tree::TreeEntry;
use crate::db::MongoModel;

/// Returns the repo's staging index, or an empty (unsaved) one if none
/// exists yet.
pub async fn get(repo_id: ObjectId) -> CoreResult<Index> {
    match Index::repository()
        .find_one(doc! { "repo_id": repo_id })
        .await?
    {
        Some(i) => Ok(i),
        None => Ok(Index {
            id: None,
            repo_id,
            entries: vec![],
        }),
    }
}

fn normalize_path(path: &str) -> CoreResult<String> {
    let trimmed = path.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Err(CoreError::Validation("path must not be empty".into()));
    }
    for segment in trimmed.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(CoreError::Validation(format!(
                "path contains illegal segment: {segment:?}"
            )));
        }
    }
    Ok(trimmed.to_string())
}

async fn ensure_index(repo_id: ObjectId) -> CoreResult<()> {
    Index::collection()
        .update_one(
            doc! { "repo_id": repo_id },
            doc! { "$setOnInsert": { "repo_id": repo_id, "entries": [] } },
        )
        .upsert(true)
        .await?;
    Ok(())
}

/// Adds or replaces the entry for `path` in the staging index.
pub async fn stage(repo_id: ObjectId, path: &str, blob_hash: &str) -> CoreResult<()> {
    let path = normalize_path(path)?;
    if blob_hash.is_empty() {
        return Err(CoreError::Validation("blob_hash must not be empty".into()));
    }

    ensure_index(repo_id).await?;

    let coll = Index::collection();
    coll.update_one(
        doc! { "repo_id": repo_id },
        doc! { "$pull": { "entries": { "path": &path } } },
    )
    .await?;

    let entry = IndexEntry {
        path,
        blob_hash: blob_hash.to_string(),
    };
    coll.update_one(
        doc! { "repo_id": repo_id },
        doc! { "$push": { "entries": to_bson(&entry)? } },
    )
    .await?;

    Ok(())
}

/// Removes a path from the staging index. No-ops if absent.
pub async fn unstage(repo_id: ObjectId, path: &str) -> CoreResult<()> {
    let path = normalize_path(path)?;
    Index::collection()
        .update_one(
            doc! { "repo_id": repo_id },
            doc! { "$pull": { "entries": { "path": path } } },
        )
        .await?;
    Ok(())
}

/// Empties the staging index (keeps the index document).
pub async fn clear(repo_id: ObjectId) -> CoreResult<()> {
    Index::collection()
        .update_one(
            doc! { "repo_id": repo_id },
            doc! { "$set": { "entries": [] } },
        )
        .await?;
    Ok(())
}

/// Materializes the staging index as `tree → commit`, chains onto the
/// current HEAD as parent, and advances HEAD.
///
/// Fails if nothing is staged.
pub async fn commit_staged(repo_id: ObjectId, message: &str) -> CoreResult<Commit> {
    if message.trim().is_empty() {
        return Err(CoreError::Validation("commit message must not be empty".into()));
    }

    let index = get(repo_id).await?;
    if index.entries.is_empty() {
        return Err(CoreError::Validation(
            "nothing to commit; staging index is empty".into(),
        ));
    }

    let tree_hash = build_tree(&index.entries).await?;

    let repo = crate::core::repo::get(repo_id).await?;
    let parents: Vec<String> = repo.head_commit.into_iter().collect();

    let commit =
        crate::core::commit::create(repo_id, &tree_hash, parents, message.trim()).await?;
    crate::core::repo::set_head(repo_id, &commit.id).await?;
    clear(repo_id).await?;

    Ok(commit)
}

// --- flat → nested tree construction -----------------------------------

enum Node {
    Blob(String),
    Dir(BTreeMap<String, Node>),
}

fn insert_node(root: &mut BTreeMap<String, Node>, path: &str, blob: &str) -> CoreResult<()> {
    let mut components = path.split('/');
    let Some(first) = components.next() else {
        return Err(CoreError::Validation(format!("invalid path: {path}")));
    };
    let remaining: Vec<&str> = components.collect();

    if remaining.is_empty() {
        if let Some(existing) = root.get(first) {
            if matches!(existing, Node::Dir(_)) {
                return Err(CoreError::Validation(format!(
                    "path '{path}' conflicts with an existing directory"
                )));
            }
        }
        root.insert(first.to_string(), Node::Blob(blob.to_string()));
        return Ok(());
    }

    let child = root
        .entry(first.to_string())
        .or_insert_with(|| Node::Dir(BTreeMap::new()));
    match child {
        Node::Dir(children) => {
            insert_node(children, &remaining.join("/"), blob)
        }
        Node::Blob(_) => Err(CoreError::Validation(format!(
            "path '{path}' conflicts with an existing file at '{first}'"
        ))),
    }
}

async fn build_tree(entries: &[IndexEntry]) -> CoreResult<String> {
    let mut root = BTreeMap::new();
    for e in entries {
        insert_node(&mut root, &e.path, &e.blob_hash)?;
    }
    materialize(root).await
}

fn materialize(
    children: BTreeMap<String, Node>,
) -> Pin<Box<dyn Future<Output = CoreResult<String>> + Send>> {
    Box::pin(async move {
        let mut tree_entries: Vec<TreeEntry> = Vec::with_capacity(children.len());
        for (name, node) in children {
            match node {
                Node::Blob(hash) => tree_entries.push(TreeEntry {
                    name,
                    entry_type: crate::core::tree::ENTRY_BLOB.to_string(),
                    hash,
                }),
                Node::Dir(sub) => {
                    let sub_hash = materialize(sub).await?;
                    tree_entries.push(TreeEntry {
                        name,
                        entry_type: crate::core::tree::ENTRY_TREE.to_string(),
                        hash: sub_hash,
                    });
                }
            }
        }
        let tree = crate::core::tree::put(tree_entries).await?;
        Ok(tree.id)
    })
}
