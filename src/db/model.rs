use std::borrow::Borrow;

use futures::TryStreamExt;
use mongodb::{
    Collection, Database, IndexModel,
    bson::{Bson, Document, doc},
    results::{DeleteResult, InsertOneResult, UpdateResult},
};

pub type Result<T> = std::result::Result<T, mongodb::error::Error>;

pub trait MongoModel:
    serde::Serialize + serde::de::DeserializeOwned + Send + Sync + Unpin + 'static
{
    const COLLECTION_NAME: &'static str;

    fn indexes() -> Vec<IndexModel> {
        Vec::new()
    }

    /// Typed collection handle from the global [`App`](crate::app::App).
    ///
    /// Panics if `App::init` has not yet been called.
    fn collection() -> Collection<Self>
    where
        Self: Sized,
    {
        Self::collection_in(crate::app::App::get().db())
    }

    /// Typed collection handle bound to an explicit [`Database`].
    fn collection_in(db: &Database) -> Collection<Self>
    where
        Self: Sized,
    {
        db.collection::<Self>(Self::COLLECTION_NAME)
    }

    /// High-level repository backed by the global [`App`](crate::app::App).
    ///
    /// Panics if `App::init` has not yet been called.
    fn repository() -> Repository<Self>
    where
        Self: Sized,
    {
        Repository {
            coll: Self::collection(),
        }
    }

    /// High-level repository bound to an explicit [`Database`].
    fn repository_in(db: &Database) -> Repository<Self>
    where
        Self: Sized,
    {
        Repository {
            coll: Self::collection_in(db),
        }
    }
}

pub async fn ensure_indexes_for<T: MongoModel>(db: &Database) -> Result<()> {
    let indexes = T::indexes();
    if indexes.is_empty() {
        return Ok(());
    }
    let coll: Collection<T> = db.collection::<T>(T::COLLECTION_NAME);
    coll.create_indexes(indexes).await?;
    Ok(())
}

pub struct Repository<T: MongoModel> {
    pub coll: Collection<T>,
}

impl<T: MongoModel> Default for Repository<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: MongoModel> Repository<T> {
    /// Repository backed by the global [`App`](crate::app::App).
    pub fn new() -> Self {
        Self {
            coll: T::collection(),
        }
    }

    /// Repository bound to an explicit [`Database`].
    pub fn new_in(db: &Database) -> Self {
        Self {
            coll: T::collection_in(db),
        }
    }

    pub fn collection(&self) -> &Collection<T> {
        &self.coll
    }

    pub async fn insert_one(&self, doc: impl Borrow<T>) -> Result<InsertOneResult> {
        self.coll.insert_one(doc).await
    }

    pub async fn insert_many<I>(&self, docs: I) -> Result<mongodb::results::InsertManyResult>
    where
        I: IntoIterator,
        I::Item: Borrow<T>,
    {
        self.coll.insert_many(docs).await
    }

    pub async fn find_one(&self, filter: Document) -> Result<Option<T>> {
        self.coll.find_one(filter).await
    }

    pub async fn find_by_id(&self, id: impl Into<Bson>) -> Result<Option<T>> {
        self.coll.find_one(doc! { "_id": id.into() }).await
    }

    pub async fn find(&self, filter: Document) -> Result<Vec<T>> {
        let cursor = self.coll.find(filter).await?;
        cursor.try_collect().await
    }

    pub async fn count(&self, filter: Document) -> Result<u64> {
        self.coll.count_documents(filter).await
    }

    pub async fn update_one(&self, filter: Document, update: Document) -> Result<UpdateResult> {
        self.coll.update_one(filter, update).await
    }

    pub async fn update_by_id(
        &self,
        id: impl Into<Bson>,
        update: Document,
    ) -> Result<UpdateResult> {
        self.coll
            .update_one(doc! { "_id": id.into() }, update)
            .await
    }

    pub async fn delete_one(&self, filter: Document) -> Result<DeleteResult> {
        self.coll.delete_one(filter).await
    }

    pub async fn delete_by_id(&self, id: impl Into<Bson>) -> Result<DeleteResult> {
        self.coll.delete_one(doc! { "_id": id.into() }).await
    }

    pub async fn replace_one(
        &self,
        filter: Document,
        replacement: impl Borrow<T>,
    ) -> Result<UpdateResult> {
        self.coll.replace_one(filter, replacement).await
    }
}

/// Helpers for building [`IndexModel`] values. Re-exported at `crate::db`.
pub mod index {
    use mongodb::{
        IndexModel,
        bson::{Document, doc},
        options::IndexOptions,
    };

    /// Ascending index on a single field.
    pub fn asc(field: &str) -> IndexModel {
        IndexModel::builder().keys(doc! { field: 1_i32 }).build()
    }

    /// Descending index on a single field.
    pub fn desc(field: &str) -> IndexModel {
        IndexModel::builder().keys(doc! { field: -1_i32 }).build()
    }

    /// Unique ascending index on a single field.
    pub fn unique(field: &str) -> IndexModel {
        IndexModel::builder()
            .keys(doc! { field: 1_i32 })
            .options(IndexOptions::builder().unique(true).build())
            .build()
    }

    /// Compound index from an explicit key spec, e.g. `compound(doc! { "a": 1, "b": -1 })`.
    pub fn compound(keys: Document) -> IndexModel {
        IndexModel::builder().keys(keys).build()
    }

    /// Compound unique index from an explicit key spec.
    pub fn compound_unique(keys: Document) -> IndexModel {
        IndexModel::builder()
            .keys(keys)
            .options(IndexOptions::builder().unique(true).build())
            .build()
    }
}
