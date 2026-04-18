#![allow(dead_code)]

pub mod model;
pub mod models;
pub mod registry;

#[allow(unused_imports)]
pub use model::{ensure_indexes_for, index, MongoModel, Repository, Result};
#[allow(unused_imports)]
pub use mongodb::{bson, Client, Database};

use registry::ensure_all_registered;

/// Connects to MongoDB and applies all registered models (indexes, etc.).
///
/// If `database_name` is `None`, the database encoded in the URI is used, or
/// `"ledger"` as a fallback.
pub async fn connect_and_sync(uri: &str, database_name: Option<&str>) -> Result<Database> {
    let client = Client::with_uri_str(uri).await?;
    let db = match database_name {
        Some(name) => client.database(name),
        None => client
            .default_database()
            .unwrap_or_else(|| client.database("ledger")),
    };
    sync_models(&db).await?;
    Ok(db)
}

/// Runs index/schema synchronization for every registered model.
pub async fn sync_models(db: &Database) -> Result<()> {
    ensure_all_registered(db).await
}

/// Returns metadata for every model registered via [`crate::register_mongo_model!`].
pub fn registered_models() -> impl Iterator<Item = &'static registry::RegisteredModel> {
    inventory::iter::<registry::RegisteredModel>.into_iter()
}
