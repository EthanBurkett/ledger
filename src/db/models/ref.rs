use mongodb::bson::oid::ObjectId;
use mongodb::IndexModel;
use serde::{Deserialize, Serialize};

use crate::db::{index, MongoModel};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ref {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,

    pub repo_id: ObjectId,

    pub name: String, // main, dev, etc

    pub commit: String,
}

impl MongoModel for Ref {
    const COLLECTION_NAME: &'static str = "refs";

    fn indexes() -> Vec<IndexModel> {
        vec![index::compound_unique(mongodb::bson::doc! {
            "repo_id": 1,
            "name": 1
        })]
    }
}

crate::register_mongo_model!(Ref);
