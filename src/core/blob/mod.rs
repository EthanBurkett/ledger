//! Blobs: content-addressed binary content.

use mongodb::bson::doc;

use crate::core::{
    errors::is_duplicate_key,
    hash::sha256_hex,
    CoreError, CoreResult,
};
use crate::db::{models::blob::Blob, MongoModel};

/// Inserts `content` as a new blob. If a blob with identical content already
/// exists the existing record is returned (idempotent — content-addressed).
pub async fn put(content: Vec<u8>) -> CoreResult<Blob> {
    let id = sha256_hex(&content);
    let size = content.len() as i64;
    let blob = Blob { id, content, size };

    match Blob::repository().insert_one(&blob).await {
        Ok(_) => Ok(blob),
        Err(e) if is_duplicate_key(&e) => Ok(blob),
        Err(e) => Err(e.into()),
    }
}

pub async fn get(hash: &str) -> CoreResult<Blob> {
    Blob::repository()
        .find_by_id(hash.to_string())
        .await?
        .ok_or_else(|| CoreError::NotFound {
            entity: "blob",
            id: hash.to_string(),
        })
}

pub async fn exists(hash: &str) -> CoreResult<bool> {
    Ok(Blob::repository()
        .count(doc! { "_id": hash })
        .await?
        > 0)
}

pub async fn delete(hash: &str) -> CoreResult<()> {
    let r = Blob::repository().delete_by_id(hash.to_string()).await?;
    if r.deleted_count == 0 {
        return Err(CoreError::NotFound {
            entity: "blob",
            id: hash.to_string(),
        });
    }
    Ok(())
}
