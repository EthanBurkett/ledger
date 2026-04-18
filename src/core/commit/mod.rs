//! Commits: immutable snapshots of a repo at a point in time.
//!
//! A commit is content-addressed by the SHA-256 of its canonical form:
//!
//! ```text
//! repo <repo_id>
//! tree <tree_hash>
//! parent <parent_hash>       (zero or more, sorted)
//! timestamp <unix_seconds>
//!
//! <message>
//! ```

use std::collections::{HashSet, VecDeque};

use mongodb::bson::{doc, oid::ObjectId};

use crate::core::{
    errors::is_duplicate_key,
    hash::sha256_hex,
    CoreError, CoreResult,
};
use crate::db::models::commit::Commit;
use crate::db::MongoModel;

fn canonical(
    repo_id: ObjectId,
    tree: &str,
    parents: &[String],
    timestamp: i64,
    message: &str,
) -> String {
    let mut parents = parents.to_vec();
    parents.sort();
    let mut out = String::new();
    out.push_str(&format!("repo {}\n", repo_id.to_hex()));
    out.push_str(&format!("tree {tree}\n"));
    for p in &parents {
        out.push_str(&format!("parent {p}\n"));
    }
    out.push_str(&format!("timestamp {timestamp}\n\n"));
    out.push_str(message);
    out
}

/// Creates a commit. The commit id (hash) is deterministic in
/// `(repo_id, tree, parents, timestamp, message)` — identical inputs
/// collapse to the same commit (idempotent).
pub async fn create(
    repo_id: ObjectId,
    tree: &str,
    parents: Vec<String>,
    message: &str,
) -> CoreResult<Commit> {
    if message.trim().is_empty() {
        return Err(CoreError::Validation("commit message must not be empty".into()));
    }
    if tree.is_empty() {
        return Err(CoreError::Validation("commit tree hash must not be empty".into()));
    }

    let timestamp = chrono::Utc::now().timestamp();
    let id = sha256_hex(canonical(repo_id, tree, &parents, timestamp, message).as_bytes());

    let commit = Commit {
        id: id.clone(),
        repo_id,
        tree: tree.to_string(),
        parents,
        message: message.to_string(),
        timestamp,
    };

    match Commit::repository().insert_one(&commit).await {
        Ok(_) => Ok(commit),
        Err(e) if is_duplicate_key(&e) => Ok(commit),
        Err(e) => Err(e.into()),
    }
}

pub async fn get(hash: &str) -> CoreResult<Commit> {
    Commit::repository()
        .find_by_id(hash.to_string())
        .await?
        .ok_or_else(|| CoreError::NotFound {
            entity: "commit",
            id: hash.to_string(),
        })
}

/// All commits for a repo in reverse-chronological order, up to `limit`.
pub async fn list_for_repo(repo_id: ObjectId, limit: Option<i64>) -> CoreResult<Vec<Commit>> {
    let mut commits = Commit::repository()
        .find(doc! { "repo_id": repo_id })
        .await?;
    commits.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    if let Some(l) = limit {
        commits.truncate(l.max(0) as usize);
    }
    Ok(commits)
}

/// Walks `head`'s ancestry (DFS over `parents`), returning commits in
/// reverse-chronological order up to `limit` entries.
pub async fn history(head: &str, limit: usize) -> CoreResult<Vec<Commit>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(head.to_string());

    while let Some(id) = queue.pop_front() {
        if out.len() >= limit {
            break;
        }
        if !seen.insert(id.clone()) {
            continue;
        }
        let commit = match get(&id).await {
            Ok(c) => c,
            Err(CoreError::NotFound { .. }) => continue,
            Err(e) => return Err(e),
        };
        for p in &commit.parents {
            queue.push_back(p.clone());
        }
        out.push(commit);
    }

    out.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(out)
}

/// True iff `maybe_ancestor` is reachable from `descendant` by walking parents.
pub async fn is_ancestor(maybe_ancestor: &str, descendant: &str) -> CoreResult<bool> {
    if maybe_ancestor == descendant {
        return Ok(true);
    }
    let mut seen = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(descendant.to_string());

    while let Some(id) = queue.pop_front() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let commit = match get(&id).await {
            Ok(c) => c,
            Err(CoreError::NotFound { .. }) => continue,
            Err(e) => return Err(e),
        };
        for p in &commit.parents {
            if p == maybe_ancestor {
                return Ok(true);
            }
            queue.push_back(p.clone());
        }
    }
    Ok(false)
}
