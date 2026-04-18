use mongodb::IndexModel;
use serde::{Deserialize, Serialize};

use crate::db::{index, MongoModel};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blob {
    #[serde(rename = "_id")]
    pub id: String, // hash(content)

    pub content: Vec<u8>,

    pub size: i64,
}

impl MongoModel for Blob {
    const COLLECTION_NAME: &'static str = "blobs";

    fn indexes() -> Vec<IndexModel> {
        vec![index::asc("size")]
    }
}

crate::register_mongo_model!(Blob);
