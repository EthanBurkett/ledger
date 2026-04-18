//! Authentication layer.
//!
//! JWT-based, no cookies. Two tokens per session:
//!
//! - **Access token** (short-lived): presented as `Authorization: Bearer <t>`
//!   on every request. Validated by the [`extractor::AuthUser`] extractor.
//! - **Refresh token** (long-lived): presented to `POST /v1/auth/refresh`
//!   to mint a new pair. Stored server-side in Mongo and rotated on every
//!   use; a reused refresh token revokes every outstanding refresh for the
//!   user (presumed token theft).
//!
//! The `stay_logged_in` flag on login multiplies both TTLs by
//! [`AuthConfig::stay_logged_in_multiplier`] so clients can opt into longer
//! sessions at login time.

pub mod config;
pub mod extractor;
pub mod jwt;
pub mod password;
pub mod service;

#[allow(unused_imports)]
pub use config::AuthConfig;
#[allow(unused_imports)]
pub use extractor::AuthUser;
#[allow(unused_imports)]
pub use service::{TokenPair, UserView};
