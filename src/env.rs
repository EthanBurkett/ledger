//! Loads environment variables from a `.env` file, if one exists.
//!
//! Called once at the very top of `main()` so that both clap's `Arg::env`
//! lookups and any `std::env::var` reads downstream see the file's values.
//!
//! Lookup order (first hit wins, subsequent files are ignored):
//! 1. `LEDGER_ENV_FILE` env var, if set, points at an explicit path.
//! 2. `./.env`, then walk up the directory tree to the filesystem root.
//!
//! Values already present in the process environment are **not** overridden,
//! so shell exports / CI secrets always beat the file.

use std::path::PathBuf;
use std::sync::OnceLock;

static LOADED: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Returns the `.env` path that was loaded by [`load`], if any.
pub fn loaded_path() -> Option<&'static PathBuf> {
    LOADED.get().and_then(|o| o.as_ref())
}

/// Loads `.env` if discoverable. Returns the path that was loaded, if any.
pub fn load() -> Option<PathBuf> {
    let result = discover();
    let _ = LOADED.set(result.clone());
    result
}

fn discover() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("LEDGER_ENV_FILE") {
        let path = PathBuf::from(explicit);
        match dotenvy::from_path(&path) {
            Ok(()) => return Some(path),
            Err(e) => {
                eprintln!(
                    "ledger: LEDGER_ENV_FILE={} could not be loaded: {e}",
                    path.display()
                );
                return None;
            }
        }
    }

    match dotenvy::dotenv() {
        Ok(path) => Some(path),
        Err(e) if e.not_found() => None,
        Err(e) => {
            eprintln!("ledger: .env was found but could not be parsed: {e}");
            None
        }
    }
}
