//! Persistent CLI configuration — API base URL and stored credentials.
//!
//! Credentials live in a small JSON file under the user's config dir
//! (`%APPDATA%\Roaming\ledger\credentials.json` on Windows,
//! `~/.config/ledger/credentials.json` on Linux). The file is created
//! with owner-only permissions where the platform allows it, and always
//! rewritten atomically via a temp file.

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const DEFAULT_API_URL: &str = "http://127.0.0.1:3030";
const ENV_API_URL: &str = "LEDGER_API_URL";
const CREDENTIALS_FILE: &str = "credentials.json";

/// Returns the API base URL the CLI should talk to.
///
/// Lookup order:
/// 1. `LEDGER_API_URL` environment variable, if set.
/// 2. Saved credentials' `api_url`, if we have any.
/// 3. [`DEFAULT_API_URL`] (`http://127.0.0.1:3030`).
pub fn api_url() -> String {
    if let Ok(v) = std::env::var(ENV_API_URL) {
        if !v.trim().is_empty() {
            return strip_trailing_slash(v);
        }
    }
    if let Some(creds) = Credentials::load_ok() {
        return strip_trailing_slash(creds.api_url);
    }
    DEFAULT_API_URL.to_string()
}

fn strip_trailing_slash(mut s: String) -> String {
    while s.ends_with('/') {
        s.pop();
    }
    s
}

/// What we remember between CLI invocations after the user has logged in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub api_url: String,
    pub username: String,
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default)]
    pub stay_logged_in: bool,
    /// Unix seconds when the access token expires (best-effort, for UX only).
    #[serde(default)]
    pub access_expires_at: Option<i64>,
    #[serde(default)]
    pub refresh_expires_at: Option<i64>,
}

impl Credentials {
    /// Returns the path where credentials are stored, creating the parent
    /// directory if needed. The file itself may or may not exist.
    pub fn path() -> io::Result<PathBuf> {
        let base = dirs::config_dir().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not locate a user config directory (set LEDGER_CONFIG_HOME?)",
            )
        })?;
        let dir = base.join("ledger");
        fs::create_dir_all(&dir)?;
        Ok(dir.join(CREDENTIALS_FILE))
    }

    /// Loads the credentials file, or returns `None` if the file is absent
    /// or can't be parsed. Callers that want to *require* a login should
    /// use [`Credentials::require`].
    pub fn load_ok() -> Option<Self> {
        let path = Self::path().ok()?;
        if !path.exists() {
            return None;
        }
        let bytes = fs::read(&path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Loads credentials or returns a friendly error suggesting `login`.
    pub fn require() -> io::Result<Self> {
        Self::load_ok().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "not logged in; run `ledger login <username>` first",
            )
        })
    }

    /// Writes to `credentials.json` atomically. On unix, the file is
    /// created with mode 0600.
    pub fn save(&self) -> io::Result<PathBuf> {
        let path = Self::path()?;
        let tmp = path.with_extension("json.tmp");
        {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                f.set_permissions(fs::Permissions::from_mode(0o600))?;
            }
            let json = serde_json::to_vec_pretty(self)?;
            f.write_all(&json)?;
            f.flush()?;
        }
        fs::rename(&tmp, &path)?;
        Ok(path)
    }

    /// Removes the credentials file if present. Returns true if it existed.
    pub fn clear() -> io::Result<bool> {
        let path = match Self::path() {
            Ok(p) => p,
            Err(_) => return Ok(false),
        };
        if path.exists() {
            fs::remove_file(&path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
