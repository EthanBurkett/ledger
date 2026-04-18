use crate::db::MongoModel;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoAccess {
    pub id: Option<ObjectId>,

    pub repo_id: ObjectId,

    pub user_id: ObjectId,

    pub role: String, // "read" | "write" | "admin"
}

impl MongoModel for RepoAccess {
    const COLLECTION_NAME: &'static str = "repo_access";
}

crate::register_mongo_model!(RepoAccess);
