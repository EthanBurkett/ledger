use mongodb::bson::oid::ObjectId;
use mongodb::IndexModel;
use serde::{Deserialize, Serialize};

use crate::db::{index, MongoModel};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,

    pub username: String,

    /// Argon2id PHC string.
    pub password_hash: String,

    /// Unix seconds.
    pub created_at: i64,

    /// Unix seconds. Updated on every successful login.
    pub last_login_at: Option<i64>,
}

impl MongoModel for User {
    const COLLECTION_NAME: &'static str = "users";

    fn indexes() -> Vec<IndexModel> {
        vec![index::unique("username")]
    }
}

crate::register_mongo_model!(User);
