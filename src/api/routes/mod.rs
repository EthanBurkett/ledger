//! API route modules.
//!
//! To add a new set of routes, create a file in this directory, implement a
//! `fn register(router: axum::Router) -> axum::Router` inside it, call
//! `crate::register_routes!(register);` at the bottom of the file, and then
//! add `pub mod my_file;` below so the registration runs.

mod guards;

pub mod auth;
pub mod blobs;
pub mod commits;
pub mod diff;
pub mod health;
pub mod index;
pub mod refs;
pub mod repos;
pub mod trees;
