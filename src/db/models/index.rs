use mongodb::bson::oid::ObjectId;
use mongodb::IndexModel;
use serde::{Deserialize, Serialize};

use crate::db::{index, MongoModel};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub path: String,
    pub blob_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,

    pub repo_id: ObjectId,

    pub entries: Vec<IndexEntry>,
}

impl MongoModel for Index {
    const COLLECTION_NAME: &'static str = "index";

    fn indexes() -> Vec<IndexModel> {
        vec![index::unique("repo_id")]
    }
}

crate::register_mongo_model!(Index);
