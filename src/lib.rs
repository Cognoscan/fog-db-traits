use std::{collections::HashMap, error::Error, sync::Arc};

use async_trait::async_trait;
use fog_pack::{entry::EntryRef, error::Error as FogError, schema::Schema, types::*, document::Document};

pub mod gate;
pub mod cert;
pub mod group;
pub mod transaction;

pub use gate::*;
pub use cert::*;
pub use group::*;
pub use transaction::*;

/// An origin address for a database node on the network.
///
/// If both `perm_id` and `eph_id` are specified, this address is generally
/// unique, and at the very least the node's intent is to act as though it is
/// unique.
pub struct NodeAddr {
    /// The network type this node is using
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

    /// Open a new group through this database
    fn group(&self, spec: GroupSpec) -> Box<dyn Group>;

    /// Open a local cursor on this database
    fn cursor(&self) -> Box<dyn Cursor>;

    /// Get a document directly from the database
    fn doc_get(&self, doc: &Hash) -> DbResult<Option<Arc<Document>>>;

    /// Get a schema in the database
    fn schema_get(&self, schema: &Hash) -> DbResult<Option<Arc<Schema>>>;

    /// Add a schema to the database. Fails if the schema document wasn't valid.
    fn schema_add(&self, schema: Arc<Document>) -> DbResult<Result<Arc<Schema>, FogError>>;

    /// Remove a schema from the database. Returns false if the schema wasn't in the database.
    fn schema_del(&self, schema: &Hash) -> DbResult<bool>;

    /// Get a list of all schemas in the database.
    fn schema_list(&self) -> Vec<Hash>;

    /// Get a hash associated with a name in the database.
    fn name_get(&self, name: &str) -> DbResult<Option<Hash>>;

    /// Add a name-to-hash mapping to the database. This pins the document
    /// inside the database, once it's been added. This should be done before
    /// adding the document in a transaction. Returns the previous hash, if
    /// there was one.
    fn name_add(&self, name: &str, hash: &Hash) -> DbResult<Option<Hash>>;

    /// Remove a name-hash mapping from the database, returning None if there
    /// wasn't one stored.
    fn name_del(&self, schema: &Hash) -> DbResult<Option<Hash>>;

    /// Get a list of all named documents in the database.
    fn name_list(&self) -> Vec<(String, Hash)>;
}

/// A connection to the database through which a transaction can be committed.
#[async_trait]
pub trait DbCommit {
    async fn commit(
        &self,
        docs: HashMap<Hash, DocChange>,
        entries: HashMap<EntryRef, EntryChange>,
    ) -> DbResult<Result<(), CommitErrors>>;

    /// Get a schema in the database
    fn schema_get(&self, schema: &Hash) -> DbResult<Option<Arc<Schema>>>;

    /// Get a document directly from the database
    fn doc_get(&self, doc: &Hash) -> DbResult<Option<Arc<Document>>>;
}
