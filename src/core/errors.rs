//! Error type shared by every `core::*` function.

use crate::api::ApiError;

pub type CoreResult<T> = std::result::Result<T, CoreError>;

#[derive(Debug)]
pub enum CoreError {
    /// The entity was not found (e.g. no blob with that hash).
    NotFound { entity: &'static str, id: String },
    /// Write rejected because a uniqueness invariant would be violated.
    Conflict(String),
    /// Caller-supplied input was malformed.
    Validation(String),
    /// Underlying MongoDB error.
    Db(mongodb::error::Error),
    /// Unexpected failure (serialization, invariants we couldn't encode).
    Internal(String),
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoreError::NotFound { entity, id } => write!(f, "{entity} not found: {id}"),
            CoreError::Conflict(m) => write!(f, "conflict: {m}"),
            CoreError::Validation(m) => write!(f, "validation: {m}"),
            CoreError::Db(e) => write!(f, "database error: {e}"),
            CoreError::Internal(m) => write!(f, "internal error: {m}"),
        }
    }
}

impl std::error::Error for CoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CoreError::Db(e) => Some(e),
            _ => None,
        }
    }
}

impl From<mongodb::error::Error> for CoreError {
    fn from(e: mongodb::error::Error) -> Self {
        CoreError::Db(e)
    }
}

impl From<mongodb::bson::ser::Error> for CoreError {
    fn from(e: mongodb::bson::ser::Error) -> Self {
        CoreError::Internal(format!("bson serialization failed: {e}"))
    }
}

impl From<CoreError> for ApiError {
    fn from(e: CoreError) -> Self {
        match e {
            CoreError::NotFound { entity, id } => {
                ApiError::not_found(format!("{entity} not found: {id}"))
                    .with_code("not_found", "Resource not found")
            }
            CoreError::Conflict(m) => ApiError::conflict(m),
            CoreError::Validation(m) => ApiError::validation(m),
            CoreError::Db(e) => ApiError::internal(e).with_code("database_error", "Database error"),
            CoreError::Internal(m) => ApiError::internal(m),
        }
    }
}

/// True if a MongoDB error is a duplicate-key write violation (code 11000).
///
/// Content-addressed writes (blobs, trees, commits) use `_id = hash(content)`,
/// so racing inserts of identical content collide here; that's a no-op, not a
/// user-visible error.
pub fn is_duplicate_key(err: &mongodb::error::Error) -> bool {
    use mongodb::error::{ErrorKind, WriteFailure};
    match *err.kind {
        ErrorKind::Write(WriteFailure::WriteError(ref e)) => e.code == 11000,
        ErrorKind::InsertMany(ref e) => e
            .write_errors
            .as_ref()
            .map(|errs| errs.iter().all(|w| w.code == 11000))
            .unwrap_or(false),
        _ => false,
    }
}
