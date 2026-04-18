use std::future::Future;
use std::pin::Pin;

use mongodb::Database;

/// Record submitted by each model via [`crate::register_mongo_model!`].
pub struct RegisteredModel {
    pub collection_name: &'static str,
    pub ensure_indexes: EnsureIndexesFn,
}

pub type EnsureIndexesFn =
    for<'a> fn(
        &'a Database,
    ) -> Pin<Box<dyn Future<Output = Result<(), mongodb::error::Error>> + Send + 'a>>;

inventory::collect!(RegisteredModel);

/// Runs every registered model's sync function in sequence.
pub async fn ensure_all_registered(db: &Database) -> Result<(), mongodb::error::Error> {
    for model in inventory::iter::<RegisteredModel> {
        (model.ensure_indexes)(db).await?;
    }
    Ok(())
}
