//! Diff between two trees / two commits.
//!
//! Diffs are computed on flattened trees (path → blob_hash). A change is:
//! - **Added**    when a path is present only in `right`.
//! - **Deleted**  when a path is present only in `left`.
//! - **Modified** when the blob hash differs.
//!
//! Output is sorted by path for stable ordering.

use std::collections::HashMap;

use serde::Serialize;

use crate::core::CoreResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub path: String,
    pub kind: ChangeKind,
    pub old_hash: Option<String>,
    pub new_hash: Option<String>,
}

/// Diffs two trees by hash. Either hash may be `""` to mean "empty tree".
pub async fn trees(left: &str, right: &str) -> CoreResult<Vec<Change>> {
    let left_map = flatten_or_empty(left).await?;
    let right_map = flatten_or_empty(right).await?;

    let mut out: Vec<Change> = Vec::new();

    for (path, new_hash) in &right_map {
        match left_map.get(path) {
            None => out.push(Change {
                path: path.clone(),
                kind: ChangeKind::Added,
                old_hash: None,
                new_hash: Some(new_hash.clone()),
            }),
            Some(old_hash) if old_hash != new_hash => out.push(Change {
                path: path.clone(),
                kind: ChangeKind::Modified,
                old_hash: Some(old_hash.clone()),
                new_hash: Some(new_hash.clone()),
            }),
            _ => {}
        }
    }
    for (path, old_hash) in &left_map {
        if !right_map.contains_key(path) {
            out.push(Change {
                path: path.clone(),
                kind: ChangeKind::Deleted,
                old_hash: Some(old_hash.clone()),
                new_hash: None,
            });
        }
    }

    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Diffs two commits by resolving each to its tree, then delegating to
/// [`trees`].
pub async fn commits(left: &str, right: &str) -> CoreResult<Vec<Change>> {
    let l = crate::core::commit::get(left).await?;
    let r = crate::core::commit::get(right).await?;
    trees(&l.tree, &r.tree).await
}

async fn flatten_or_empty(tree_hash: &str) -> CoreResult<HashMap<String, String>> {
    if tree_hash.is_empty() {
        return Ok(HashMap::new());
    }
    Ok(crate::core::tree::flatten(tree_hash)
        .await?
        .into_iter()
        .collect())
}
