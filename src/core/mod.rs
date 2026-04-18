//! Core business logic.
//!
//! Pure-ish functions (they hit Mongo; they don't know anything else) for
//! every ledger domain. The API layer calls into this module; this module
//! never knows about HTTP, sessions, tokens, or response envelopes.
//!
//! Every function returns a [`CoreResult<T>`]; [`CoreError`] has a
//! `From<CoreError> for ApiError` so handlers can `?` through cleanly when
//! the API routes are wired up later.

#![allow(dead_code)] // API wiring lands later; don't noise up the build.

pub mod blob;
pub mod commit;
pub mod diff;
pub mod errors;
pub mod index;
pub mod repo;
pub mod tree;

pub use errors::{CoreError, CoreResult};

/// Hashing helpers. Blobs, trees, and commits are content-addressed with
/// SHA-256. Canonical forms (for trees/commits) live alongside the domain
/// that owns them; this module only exposes the primitive.
pub(crate) mod hash {
    use sha2::{Digest, Sha256};

    pub fn sha256_hex(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hex::encode(hasher.finalize())
    }
}
