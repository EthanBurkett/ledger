//! Process-wide application state.
//!
//! `App` is initialized once (typically from the `start` command after the
//! MongoDB connection is established) and then accessible from anywhere via
//! [`App::get`] or [`App::db`].

use std::sync::OnceLock;

use mongodb::Database;

/// Shared state for the running process.
#[derive(Clone)]
pub struct App {
    db: Database,
}

static APP: OnceLock<App> = OnceLock::new();

#[derive(Debug)]
pub struct AlreadyInitialized;

impl std::fmt::Display for AlreadyInitialized {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("App has already been initialized")
    }
}

impl std::error::Error for AlreadyInitialized {}

impl App {
    /// Installs the global `App`. Returns an error if called more than once.
    pub fn init(db: Database) -> Result<&'static App, AlreadyInitialized> {
        let app = App { db };
        APP.set(app).map_err(|_| AlreadyInitialized)?;
        Ok(APP.get().expect("App was just set"))
    }

    /// Returns the global `App` if it has been initialized.
    pub fn try_get() -> Option<&'static App> {
        APP.get()
    }

    /// Returns the global `App`. Panics if [`App::init`] has not been called.
    ///
    /// Use this from code paths that can only run after `start` has booted
    /// the application (background tasks, HTTP handlers, etc.).
    pub fn get() -> &'static App {
        APP.get().expect(
            "App is not initialized; call App::init() in the `start` command before using it",
        )
    }

    pub fn db(&self) -> &Database {
        &self.db
    }
}

/// Convenience free function equivalent to [`App::get`].
#[allow(dead_code)]
pub fn app() -> &'static App {
    App::get()
}

/// Convenience free function equivalent to `App::get().db()`.
#[allow(dead_code)]
pub fn db() -> &'static Database {
    App::get().db()
}
