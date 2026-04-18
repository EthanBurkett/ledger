//! Trees: directory snapshots, content-addressed.
//!
//! Canonical serialization sorts entries by name and emits one line per
//! entry of the form `<type> <hash> <name>\n`. The tree hash is SHA-256
//! of that canonical form.

use std::future::Future;
use std::pin::Pin;

use crate::core::{
    errors::is_duplicate_key,
    hash::sha256_hex,
    CoreError, CoreResult,
};
use crate::db::models::tree::{Tree, TreeEntry};
use crate::db::MongoModel;

pub const ENTRY_BLOB: &str = "blob";
pub const ENTRY_TREE: &str = "tree";

fn canonical(entries: &[TreeEntry]) -> String {
    let mut buf = String::with_capacity(entries.len() * 96);
    for e in entries {
        buf.push_str(&e.entry_type);
        buf.push(' ');
        buf.push_str(&e.hash);
        buf.push(' ');
        buf.push_str(&e.name);
        buf.push('\n');
    }
    buf
}

fn validate_entries(entries: &[TreeEntry]) -> CoreResult<()> {
    for e in entries {
        if e.name.is_empty() || e.name.contains('/') || e.name == "." || e.name == ".." {
            return Err(CoreError::Validation(format!(
                "illegal tree entry name: {:?}",
                e.name
            )));
        }
        if e.entry_type != ENTRY_BLOB && e.entry_type != ENTRY_TREE {
            return Err(CoreError::Validation(format!(
                "tree entry type must be '{ENTRY_BLOB}' or '{ENTRY_TREE}', got {:?}",
                e.entry_type
            )));
        }
        if e.hash.is_empty() {
            return Err(CoreError::Validation(
                "tree entry hash must not be empty".into(),
            ));
        }
    }
    Ok(())
}

/// Stores a tree. Entries are normalized (sorted by name, de-duplicated) and
/// the tree is content-addressed by the canonical form's SHA-256. Idempotent.
pub async fn put(mut entries: Vec<TreeEntry>) -> CoreResult<Tree> {
    validate_entries(&entries)?;
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    // Reject duplicate names.
    if entries.windows(2).any(|w| w[0].name == w[1].name) {
        return Err(CoreError::Validation(
            "tree contains duplicate entry names".into(),
        ));
    }

    let id = sha256_hex(canonical(&entries).as_bytes());
    let tree = Tree { id, entries };

    match Tree::repository().insert_one(&tree).await {
        Ok(_) => Ok(tree),
        Err(e) if is_duplicate_key(&e) => Ok(tree),
        Err(e) => Err(e.into()),
    }
}

pub async fn get(hash: &str) -> CoreResult<Tree> {
    Tree::repository()
        .find_by_id(hash.to_string())
        .await?
        .ok_or_else(|| CoreError::NotFound {
            entity: "tree",
            id: hash.to_string(),
        })
}

pub async fn exists(hash: &str) -> CoreResult<bool> {
    Ok(Tree::repository()
        .count(mongodb::bson::doc! { "_id": hash })
        .await?
        > 0)
}

/// Walks the tree rooted at `hash` and returns every `(path, blob_hash)`
/// leaf. Paths are POSIX-style (`a/b/c.txt`).
pub async fn flatten(hash: &str) -> CoreResult<Vec<(String, String)>> {
    let mut out = Vec::new();
    walk(hash.to_string(), String::new(), &mut out).await?;
    Ok(out)
}

fn walk<'a>(
    hash: String,
    prefix: String,
    out: &'a mut Vec<(String, String)>,
) -> Pin<Box<dyn Future<Output = CoreResult<()>> + Send + 'a>> {
    Box::pin(async move {
        let tree = get(&hash).await?;
        for entry in tree.entries {
            let path = if prefix.is_empty() {
                entry.name.clone()
            } else {
                format!("{prefix}/{}", entry.name)
            };
            match entry.entry_type.as_str() {
                ENTRY_BLOB => out.push((path, entry.hash)),
                ENTRY_TREE => walk(entry.hash, path, out).await?,
                other => {
                    return Err(CoreError::Internal(format!(
                        "unknown tree entry type: {other}"
                    )))
                }
            }
        }
        Ok(())
    })
}

/// Resolves a POSIX path within a tree to its leaf target, returning
/// `(entry_type, hash)`.
pub async fn resolve_path(root_hash: &str, path: &str) -> CoreResult<(String, String)> {
    // Collect the path components up-front so no iterator/closure lives across
    // an `.await` boundary — some of those combinators trip HRTB inference on
    // axum handler bounds.
    let mut parts: Vec<String> = Vec::new();
    for segment in path.split('/') {
        if !segment.is_empty() {
            parts.push(segment.to_string());
        }
    }

    let mut current = root_hash.to_string();
    let mut current_type = ENTRY_TREE.to_string();

    if parts.is_empty() {
        return Ok((current_type, current));
    }

    let last = parts.len() - 1;
    for (idx, component) in parts.iter().enumerate() {
        if current_type != ENTRY_TREE {
            return Err(CoreError::NotFound {
                entity: "tree entry",
                id: path.to_string(),
            });
        }
        let tree = get(&current).await?;
        let mut next: Option<(String, String)> = None;
        for entry in tree.entries {
            if entry.name == *component {
                next = Some((entry.entry_type, entry.hash));
                break;
            }
        }
        let (next_type, next_hash) = next.ok_or(CoreError::NotFound {
            entity: "tree entry",
            id: path.to_string(),
        })?;
        current = next_hash;
        current_type = next_type;
        if idx == last {
            return Ok((current_type, current));
        }
    }

    Ok((current_type, current))
}
