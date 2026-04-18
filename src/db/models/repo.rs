use mongodb::bson::oid::ObjectId;
use mongodb::IndexModel;
use serde::{Deserialize, Serialize};

use crate::db::{index, MongoModel};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,

    pub name: String,

    pub head_commit: Option<String>,
}

impl MongoModel for Repo {
    const COLLECTION_NAME: &'static str = "repos";

    fn indexes() -> Vec<IndexModel> {
        vec![index::unique("name"), index::asc("head_commit")]
    }
}

crate::register_mongo_model!(Repo);
