//! The `.ledger/` working directory.
//!
//! Analogous to `.git/`. When the user runs `ledger init <name>` we create
//! a `.ledger/` folder in the current working directory and drop a
//! `config.json` in it recording the repo the directory is bound to.
//!
//! Subsequent commands walk up from the CWD until they find a `.ledger/`
//! (the "workdir root"), so the CLI behaves the same regardless of which
//! sub-directory the user invokes it from.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const WORKDIR_DIRNAME: &str = ".ledger";
pub const WORKDIR_CONFIG: &str = "config.json";

/// Persisted per-workdir configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkdirConfig {
    /// Repo ObjectId (24 hex chars) this workdir targets.
    pub repo_id: String,
    /// Repo name — stored for convenience / `status`, not authoritative.
    pub repo_name: String,
    /// API URL that was in effect when `init` ran. Informational.
    #[serde(default)]
    pub api_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Workdir {
    pub root: PathBuf,
    pub config: WorkdirConfig,
}

impl Workdir {
    /// Path to the `.ledger/` directory.
    pub fn dir(&self) -> PathBuf {
        self.root.join(WORKDIR_DIRNAME)
    }

    /// Path to the `.ledger/config.json` file.
    #[allow(dead_code)]
    pub fn config_path(&self) -> PathBuf {
        self.dir().join(WORKDIR_CONFIG)
    }

    /// Resolves an on-disk path (absolute or relative to CWD) into the
    /// POSIX-style relative path used by the API (`a/b/c.txt`).
    pub fn posix_relative(&self, input: &Path) -> io::Result<String> {
        let abs = if input.is_absolute() {
            input.to_path_buf()
        } else {
            std::env::current_dir()?.join(input)
        };
        let canon = fs::canonicalize(&abs).unwrap_or(abs);
        let root_canon = fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone());
        let rel = canon.strip_prefix(&root_canon).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "path {} is outside the workdir {}",
                    canon.display(),
                    root_canon.display()
                ),
            )
        })?;
        Ok(rel
            .components()
            .filter_map(|c| c.as_os_str().to_str().map(|s| s.to_string()))
            .collect::<Vec<_>>()
            .join("/"))
    }
}

/// Walks up from `start` looking for a `.ledger/config.json`. Returns
/// `Ok(None)` if none is found all the way to the filesystem root.
pub fn find_from(start: &Path) -> io::Result<Option<Workdir>> {
    let mut cursor = Some(start.to_path_buf());
    while let Some(dir) = cursor {
        let candidate = dir.join(WORKDIR_DIRNAME).join(WORKDIR_CONFIG);
        if candidate.is_file() {
            let bytes = fs::read(&candidate)?;
            let config: WorkdirConfig = serde_json::from_slice(&bytes).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("corrupt {}: {e}", candidate.display()),
                )
            })?;
            return Ok(Some(Workdir { root: dir, config }));
        }
        cursor = dir.parent().map(Path::to_path_buf);
    }
    Ok(None)
}

/// Same as [`find_from`] but starts at the current working directory and
/// returns a helpful error if no workdir is found.
pub fn require() -> io::Result<Workdir> {
    let cwd = std::env::current_dir()?;
    find_from(&cwd)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "not a ledger workdir (and none found in parents); run `ledger init <name>` first",
        )
    })
}

/// Creates `.ledger/config.json` at `root`. Errors if one already exists.
pub fn init_at(root: &Path, config: &WorkdirConfig) -> io::Result<Workdir> {
    let dir = root.join(WORKDIR_DIRNAME);
    let path = dir.join(WORKDIR_CONFIG);
    if path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} already exists", path.display()),
        ));
    }
    fs::create_dir_all(&dir)?;
    let json = serde_json::to_vec_pretty(config)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    fs::write(&path, json)?;
    Ok(Workdir {
        root: root.to_path_buf(),
        config: config.clone(),
    })
}
