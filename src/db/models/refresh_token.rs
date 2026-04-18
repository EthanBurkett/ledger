use mongodb::bson::oid::ObjectId;
use mongodb::IndexModel;
use serde::{Deserialize, Serialize};

use crate::db::{index, MongoModel};

/// A server-side record for an issued refresh token. Refresh tokens are
/// rotated on every use: issuing a new refresh token marks the prior record
/// revoked and points `replaced_by` at the new `jti`.
///
/// Detecting a request that presents an already-revoked refresh token
/// indicates replay/theft and should revoke the entire chain for that user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshToken {
    /// Matches the JWT `jti` claim (uuid v4).
    #[serde(rename = "_id")]
    pub id: String,

    pub user_id: ObjectId,

    pub issued_at: i64,
    pub expires_at: i64,

    /// Set when the token is revoked (by logout, rotation, or chain-revoke).
    pub revoked_at: Option<i64>,

    /// Set when this token was rotated; points at the new token's jti.
    pub replaced_by: Option<String>,

    pub stay_logged_in: bool,
}

impl MongoModel for RefreshToken {
    const COLLECTION_NAME: &'static str = "refresh_tokens";

    fn indexes() -> Vec<IndexModel> {
        vec![
            index::asc("user_id"),
            index::asc("expires_at"),
            index::asc("revoked_at"),
        ]
    }
}

crate::register_mongo_model!(RefreshToken);
