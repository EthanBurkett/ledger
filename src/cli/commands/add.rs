//! `ledger add <paths...>` — upload files as blobs and stage them at
//! their workdir-relative paths.
//!
//! Each argument may be:
//! * a regular file — staged directly;
//! * a directory — walked recursively with `.gitignore` / `.ledgerignore`
//!   semantics; every regular file that survives the filter is staged;
//! * the special `.` form — equivalent to "the current working
//!   directory", so `ledger add .` stages every tracked file under the
//!   CWD.
//!
//! ### Change detection
//!
//! Before uploading anything, `add` builds a baseline of what the repo
//! currently believes about each path by overlaying the staging index
//! on top of HEAD's flattened tree. Every candidate file is hashed
//! locally with `sha256(content)` — the same algorithm the server uses
//! for blob IDs — and compared to the baseline. Unchanged files are
//! silently skipped, so repeated `ledger add .` calls only transmit
//! real changes.
//!
//! ### Ignore rules
//!
//! Directory walks respect (later overrides earlier):
//!
//! 1. `.gitignore`, the global gitignore, and `.git/info/exclude`.
//! 2. `.ledgerignore` (same format as `.gitignore`, Ledger-specific).
//! 3. A built-in safety list: `.git/` and `.ledger/` are always skipped.
//!
//! Hidden files are included (matching `git add .`), symlinks are not
//! followed, and overlapping arguments are de-duplicated.

use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use clap::{Arg, ArgAction, ArgMatches, Command};
use clap_action_command::vec1::Vec1;
use ignore::WalkBuilder;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::ActionCommand;
use crate::cli::client::Client;
use crate::cli::workdir::{self, Workdir, WORKDIR_DIRNAME};

#[derive(Debug, Deserialize)]
struct BlobMeta {
    hash: String,
    #[allow(dead_code)]
    size: i64,
}

#[derive(Debug, Deserialize)]
struct RepoView {
    #[allow(dead_code)]
    id: String,
    #[serde(default)]
    head_commit: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CommitView {
    tree: String,
}

#[derive(Debug, Deserialize)]
struct FlatEntry {
    path: String,
    blob_hash: String,
}

#[derive(Debug, Deserialize)]
struct IndexView {
    entries: Vec<FlatEntry>,
}

/// Custom ignore-file name, analogous to `.gitignore`.
const LEDGERIGNORE: &str = ".ledgerignore";

pub struct AddCommand {}

impl ActionCommand for AddCommand {
    fn name(&self) -> &'static str {
        "add"
    }

    fn command(&self, command: Command) -> Command {
        command
            .about("Upload one or more files (or every file under a directory) and stage them")
            .long_about(
                "Upload files and stage them in the current repo's index.\n\
                 \n\
                 PATH may be a file, a directory (walked recursively), or `.` \
                 to add every file under the current working directory.\n\
                 \n\
                 Files whose content matches the current repo state (HEAD tree \
                 + staged index) are skipped silently — only real changes are \
                 uploaded.\n\
                 \n\
                 Directory walks honor `.gitignore` and `.ledgerignore` files \
                 (same syntax). `.git/` and `.ledger/` are always excluded. \
                 Symlinks are not followed.",
            )
            .alias("a")
            .arg(
                Arg::new("paths")
                    .required(true)
                    .num_args(1..)
                    .action(ArgAction::Append)
                    .value_name("PATH"),
            )
    }

    fn action(&self, matches: Vec1<&ArgMatches>) -> Result<(), Box<dyn Error>> {
        let leaf = matches.last();
        let inputs: Vec<PathBuf> = leaf
            .get_many::<String>("paths")
            .map(|iter| iter.map(PathBuf::from).collect())
            .unwrap_or_default();
        if inputs.is_empty() {
            return Err("at least one PATH is required".into());
        }

        let wd = workdir::require()?;

        // Collect every concrete file to stage, de-duplicated by
        // canonical path so overlapping args (`.`, `src`) don't double up.
        let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
        let mut files: Vec<PathBuf> = Vec::new();
        for input in &inputs {
            collect_files(&wd, input, &mut seen, &mut files)?;
        }

        if files.is_empty() {
            return Err(
                "no files matched the given paths (everything was filtered by .gitignore/.ledgerignore?)"
                    .into(),
            );
        }

        let mut client = Client::authed()?;
        let baseline = load_baseline(&mut client, &wd.config.repo_id)?;

        let mut added = 0usize;
        let mut modified = 0usize;
        let mut unchanged = 0usize;

        for file in files {
            let rel = wd.posix_relative(&file)?;
            let bytes = fs::read(&file)
                .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
            let size = bytes.len();
            let local_hash = sha256_hex(&bytes);

            let previous = baseline.get(&rel);

            // Unchanged file — nothing to do.
            if previous.is_some_and(|h| h == &local_hash) {
                unchanged += 1;
                continue;
            }

            let mark = if previous.is_some() { 'M' } else { '+' };
            let encoded = B64.encode(&bytes);

            let blob: BlobMeta = client.post(
                "/v1/blobs",
                &serde_json::json!({ "content_base64": encoded }),
            )?;

            // Defensive: the server-computed hash should match ours.
            if blob.hash != local_hash {
                return Err(format!(
                    "hash mismatch for {rel}: local {local_hash} vs server {}",
                    blob.hash
                )
                .into());
            }

            let _: Value = client.post(
                &format!("/v1/repos/{}/index", wd.config.repo_id),
                &serde_json::json!({
                    "path": rel,
                    "blob_hash": blob.hash,
                }),
            )?;

            println!("{mark} {rel}  {}  ({size} bytes)", short_hash(&blob.hash));
            if previous.is_some() {
                modified += 1;
            } else {
                added += 1;
            }
        }

        let touched = added + modified;
        if touched == 0 {
            println!("No changes to stage ({unchanged} file(s) already up to date).");
        } else {
            println!(
                "Staged {touched} change(s): {added} new, {modified} modified; {unchanged} unchanged."
            );
        }

        Ok(())
    }
}

/// Expand `input` into the list of regular files to stage.
///
/// * File → single entry, no ignore filtering (explicit user intent
///   wins; matches `git add <file>` behavior).
/// * Directory → walked with [`ignore::WalkBuilder`], honoring
///   `.gitignore` and `.ledgerignore`.
fn collect_files(
    wd: &Workdir,
    input: &Path,
    seen: &mut BTreeSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let meta = fs::symlink_metadata(input)
        .map_err(|e| format!("cannot stat {}: {e}", input.display()))?;

    if meta.file_type().is_symlink() {
        return Err(format!(
            "{} is a symlink; symlinks aren't supported",
            input.display()
        )
        .into());
    }

    if meta.is_file() {
        push_file(input, seen, out);
        return Ok(());
    }

    if meta.is_dir() {
        wd.posix_relative(input)
            .map_err(|e| format!("{}: {e}", input.display()))?;

        walk_dir(wd, input, seen, out)?;
        return Ok(());
    }

    Err(format!("{} is neither a file nor a directory", input.display()).into())
}

/// Walk a directory with `.gitignore` / `.ledgerignore` filtering.
fn walk_dir(
    _wd: &Workdir,
    dir: &Path,
    seen: &mut BTreeSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let mut builder = WalkBuilder::new(dir);
    builder
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .follow_links(false)
        .add_custom_ignore_filename(LEDGERIGNORE);

    for result in builder.build() {
        let entry = match result {
            Ok(e) => e,
            Err(e) => {
                eprintln!("warning: {e}");
                continue;
            }
        };

        let path = entry.path();
        if has_excluded_component(path) {
            continue;
        }

        let file_type = match entry.file_type() {
            Some(t) => t,
            None => continue,
        };

        if file_type.is_symlink() || file_type.is_dir() || !file_type.is_file() {
            continue;
        }

        push_file(path, seen, out);
    }

    Ok(())
}

/// Returns `true` if `path` has any `.git` or `.ledger` component.
fn has_excluded_component(path: &Path) -> bool {
    path.components().any(|c| {
        let name = c.as_os_str();
        name == WORKDIR_DIRNAME || name == ".git"
    })
}

fn push_file(path: &Path, seen: &mut BTreeSet<PathBuf>, out: &mut Vec<PathBuf>) {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if seen.insert(canonical) {
        out.push(path.to_path_buf());
    }
}

/// Build `path -> expected_blob_hash` for every file the repo already
/// believes it has. HEAD tree is the base layer; staged index entries
/// overlay on top (they represent the newer intent).
fn load_baseline(
    client: &mut Client,
    repo_id: &str,
) -> Result<HashMap<String, String>, Box<dyn Error>> {
    let mut baseline: HashMap<String, String> = HashMap::new();

    let repo: RepoView = client.get(&format!("/v1/repos/{repo_id}"))?;
    if let Some(head) = repo.head_commit.as_deref() {
        let commit: CommitView = client.get(&format!("/v1/commits/{head}"))?;
        let flat: Vec<FlatEntry> = client.get(&format!("/v1/trees/{}/flat", commit.tree))?;
        for entry in flat {
            baseline.insert(entry.path, entry.blob_hash);
        }
    }

    let index: IndexView = client.get(&format!("/v1/repos/{repo_id}/index"))?;
    for entry in index.entries {
        baseline.insert(entry.path, entry.blob_hash);
    }

    Ok(baseline)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn short_hash(h: &str) -> String {
    if h.len() > 12 {
        h[..12].to_string()
    } else {
        h.to_string()
    }
}
