use mongodb::{bson::oid::ObjectId, IndexModel};
use serde::{Deserialize, Serialize};

use crate::db::{index, MongoModel};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    #[serde(rename = "_id")]
    pub id: String, // hash(commit content)

    pub repo_id: ObjectId,

    pub tree: String,

    pub parents: Vec<String>,

    pub message: String,

    pub timestamp: i64,
}

impl MongoModel for Commit {
    const COLLECTION_NAME: &'static str = "commits";

    fn indexes() -> Vec<IndexModel> {
        vec![
            index::asc("repo_id"),
            index::asc("timestamp"),
            index::asc("parents"),
        ]
    }
}

crate::register_mongo_model!(Commit);
