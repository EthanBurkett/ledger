use mongodb::IndexModel;
use serde::{Deserialize, Serialize};

use crate::db::MongoModel;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeEntry {
    pub name: String,
    pub entry_type: String, // "blob" | "tree"
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tree {
    #[serde(rename = "_id")]
    pub id: String, // hash(entries)

    pub entries: Vec<TreeEntry>,
}

impl MongoModel for Tree {
    const COLLECTION_NAME: &'static str = "trees";

    fn indexes() -> Vec<IndexModel> {
        vec![]
    }
}

crate::register_mongo_model!(Tree);
