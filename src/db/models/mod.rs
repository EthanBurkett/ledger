//! Database models: one module per model file.
//!
//! To add a model:
//! 1. Create a new file under `db/models/` (e.g. `transaction.rs`).
//! 2. Declare a `pub struct`, implement [`crate::db::MongoModel`], and call
//!    `crate::register_mongo_model!(YourStruct);` at the bottom of the file.
//! 3. Add `pub mod transaction;` here so the module is linked and registration runs.

pub mod example_account;

#[allow(unused_imports)]
pub use example_account::Account;

/// Registers a [`MongoModel`](crate::db::MongoModel) with the global sync registry.
///
/// Call this once per model, in the same module as the `impl MongoModel`:
///
/// ```ignore
/// crate::register_mongo_model!(Account);
/// ```
#[macro_export]
macro_rules! register_mongo_model {
    ($model:ident) => {
        $crate::__register_mongo_model_impl!($model, $model);
    };
    ($model:path as $alias:ident) => {
        $crate::__register_mongo_model_impl!($model, $alias);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __register_mongo_model_impl {
    ($model:path, $alias:ident) => {
        ::paste::paste! {
            #[doc(hidden)]
            pub(crate) fn [< __ledger_ensure_indexes_ $alias:snake >](
                db: &::mongodb::Database,
            ) -> ::std::pin::Pin<
                ::std::boxed::Box<
                    dyn ::std::future::Future<
                            Output = ::std::result::Result<(), ::mongodb::error::Error>,
                        > + ::std::marker::Send + '_,
                >,
            > {
                ::std::boxed::Box::pin(
                    $crate::db::model::ensure_indexes_for::<$model>(db),
                )
            }

            ::inventory::submit! {
                $crate::db::registry::RegisteredModel {
                    collection_name: <$model as $crate::db::MongoModel>::COLLECTION_NAME,
                    ensure_indexes: [< __ledger_ensure_indexes_ $alias:snake >],
                }
            }
        }
    };
}
