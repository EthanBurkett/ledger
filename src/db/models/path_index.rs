use mongodb::bson::oid::ObjectId;
use mongodb::IndexModel;
use serde::{Deserialize, Serialize};

use crate::db::{index, MongoModel};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathIndex {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,

    pub repo_id: ObjectId,

    pub path: String,

    pub path_hash: String,

    pub target_hash: String,

    pub target_type: String,
}

impl MongoModel for PathIndex {
    const COLLECTION_NAME: &'static str = "path_index";

    fn indexes() -> Vec<IndexModel> {
        vec![index::asc("repo_id"), index::unique("path_hash")]
    }
}

crate::register_mongo_model!(PathIndex);
