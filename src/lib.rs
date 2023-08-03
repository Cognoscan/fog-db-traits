use std::{collections::HashMap, error::Error, sync::Arc};

use async_trait::async_trait;
use fog_pack::{entry::EntryRef, error::Error as FogError, schema::Schema, types::*};

pub mod cert;
pub mod group;
pub mod transaction;

pub use cert::*;
pub use group::*;
pub use transaction::*;

pub struct NodeAddr {
    /// The network type this was returned on
    pub net: NetType,
    /// Long-term Identity, notionally tied to the user of the node
    pub perm_id: Option<Identity>,
    /// Ephemeral Identity, notionally tied to the node itself
    pub eph_id: Option<Identity>,
}

/// A network type
pub enum NetType {
    Machine,
    Local,
    Regional,
    Global,
    Other(String),
}

#[non_exhaustive]
pub enum DbError {
    /// Internal Database error
    Internal(Box<dyn Error>),
    /// Error occurred while handling a fog-pack document
    FogDoc {
        context: String,
        doc: Hash,
        err: FogError,
    },
    /// Error occurred while handling a fog-pack entry
    FogEntry {
        context: String,
        entry: EntryRef,
        err: FogError,
    },
    /// Some other fog-pack related error occurred
    FogOther { context: String, err: FogError },
}

type DbResult<T> = Result<T, Box<DbError>>;

pub trait Db {
    /// Start a new transaction with this database
    fn txn(&self) -> Transaction;
    fn group(&self, spec: GroupSpec) -> Box<dyn Group>;
    fn cursor(&self) -> Box<dyn Cursor>;

    fn get_schema(&self, schema: &Hash) -> DbResult<Option<Arc<Schema>>>;
}

/// A connection to the database through which a transaction can be committed.
#[async_trait]
pub trait DbCommit {
    async fn commit(
        &self,
        docs: HashMap<Hash, DocChange>,
        entries: HashMap<EntryRef, EntryChange>,
    ) -> DbResult<Result<(), CommitErrors>>;
    fn get_schema(&self, schema: &Hash) -> DbResult<Option<Arc<Schema>>>;
}
