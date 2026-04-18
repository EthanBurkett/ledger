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
//! ### Ignore rules
//!
//! The walker respects (in order of precedence, later overrides earlier):
//!
//! 1. A project-level `.gitignore` anywhere in the repo tree, plus the
//!    user's global gitignore and `.git/info/exclude` if present. This
//!    means a fresh Rust checkout with `target/` in `.gitignore`
//!    already "just works".
//! 2. A `.ledgerignore` file, which follows the exact same format as
//!    `.gitignore` but is specific to Ledger. Use this for patterns
//!    you only want Ledger to ignore.
//! 3. A built-in safety list that is **never** overridable: `.git/`
//!    and `.ledger/` are always skipped so the working-copy metadata
//!    can never be uploaded.
//!
//! Hidden files are included (matching `git add .`), symlinks are not
//! followed, and overlapping arguments are de-duplicated.

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use clap::{Arg, ArgAction, ArgMatches, Command};
use clap_action_command::vec1::Vec1;
use ignore::WalkBuilder;
use serde::Deserialize;
use serde_json::Value;

use super::ActionCommand;
use crate::cli::client::Client;
use crate::cli::workdir::{self, Workdir, WORKDIR_DIRNAME};

#[derive(Debug, Deserialize)]
struct BlobMeta {
    hash: String,
    #[allow(dead_code)]
    size: i64,
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
        let mut staged: usize = 0;

        for file in files {
            let rel = wd.posix_relative(&file)?;
            let bytes = fs::read(&file)
                .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
            let size = bytes.len();
            let encoded = B64.encode(&bytes);

            let blob: BlobMeta = client.post(
                "/v1/blobs",
                &serde_json::json!({ "content_base64": encoded }),
            )?;

            let _: Value = client.post(
                &format!("/v1/repos/{}/index", wd.config.repo_id),
                &serde_json::json!({
                    "path": rel,
                    "blob_hash": blob.hash,
                }),
            )?;

            println!("+ {rel}  {}  ({} bytes)", short_hash(&blob.hash), size);
            staged += 1;
        }

        if staged > 1 {
            println!("Staged {staged} files.");
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
///
/// Each file is canonicalised so the same file reached via two
/// different argument paths is only staged once.
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
        // Ensure the directory is actually inside the workdir.
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
        .hidden(false) // include dotfiles (matches `git add .`)
        .git_ignore(true) // honor .gitignore
        .git_global(true) // honor ~/.config/git/ignore
        .git_exclude(true) // honor .git/info/exclude
        .parents(true) // walk up to find repo-root .gitignore
        .follow_links(false)
        .add_custom_ignore_filename(LEDGERIGNORE);

    for result in builder.build() {
        let entry = match result {
            Ok(e) => e,
            Err(e) => {
                // Permission denied / transient IO on a single entry —
                // warn and keep going rather than abort the whole add.
                eprintln!("warning: {e}");
                continue;
            }
        };

        let path = entry.path();

        // Hard safety rail: never stage anything inside `.git/` or
        // `.ledger/`, regardless of what ignore rules say.
        if has_excluded_component(path) {
            continue;
        }

        // `file_type()` is `None` only for the synthetic stdin entry,
        // which we never produce here.
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

fn short_hash(h: &str) -> String {
    if h.len() > 12 {
        h[..12].to_string()
    } else {
        h.to_string()
    }
}
