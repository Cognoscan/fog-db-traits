use std::{collections::{HashMap, BTreeMap}, error::Error, sync::Arc};

use async_trait::async_trait;
use fog_pack::{entry::EntryRef, error::Error as FogError, schema::Schema, types::*, document::Document};
use group::GroupSpec;
use thiserror::Error;

pub mod gate;
pub mod cert;
pub mod group;
pub mod transaction;
pub mod cursor;

/// Network connection information
pub struct NetInfo {
    /// Local database connection
    pub db: bool,
    /// Network within the currently running machine
    pub machine: bool,
    /// Local network
    pub local: bool,
    /// Regional (municipal, large corporate, etc.) network
    pub regional: bool,
    /// The global internet
    pub global: bool,
    /// Some other, specific network, with optional additional network information
    pub other: BTreeMap<String, BTreeMap<String, String>>,
}

/// Information about a connecting node. Includes the source network type from
/// which the connection was made, and optionally the Identities used by the
/// node.
pub struct NodeInfo {
    /// The network info for this node
    pub net: NetInfo,
    /// Long-term Identity, notionally tied to the user of the node
    pub perm_id: Option<Identity>,
    /// Ephemeral Identity, notionally tied to the node itself
    pub eph_id: Option<Identity>,
}

/// An origin address for a database node on the network.
///
/// This address is generally unique, and at the very least the node's intent is
/// to act as though it is unique.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct NodeAddr {
    /// Long-term Identity, notionally tied to the user of the node
    pub perm_id: Identity,
    /// Ephemeral Identity, notionally tied to the node itself
    pub eph_id: Identity,
}

/// An error from trying to convert a [`NodeInfo`] into a [`NodeAddr`].
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum NodeConvertError {
    #[error("Missing permanent ID")]
    MissingPermId,
    #[error("Missing ephemeral ID")]
    MissingEphId,
}

impl TryFrom<NodeInfo> for NodeAddr {
    type Error = NodeConvertError;

    fn try_from(value: NodeInfo) -> Result<Self, Self::Error> {
        let perm_id = value.perm_id.ok_or(NodeConvertError::MissingPermId)?;
        let eph_id = value.eph_id.ok_or(NodeConvertError::MissingEphId)?;
        Ok(Self {
            perm_id,
            eph_id
        })
    }
}

/// A network type
pub enum NetType {
    Db,
    Machine,
    Local,
    Regional,
    Global,
    Other(String),
}

/// A fundamental database error has occurred. Usually means the database must
/// be closed and access halted.
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

/// An implementation of a fog-pack database. Provides cursor, transaction,
/// schema, group, and name access.
///
/// - Transactions may be executed upon by calling [`Db::txn`].
/// - Groups may be opened through the database by calling [`Db::group`].
/// - Schemas may be added, retrieved, and removed from the database.
/// - Name-to-Document mappings may be added, retrieved, and removed from the
///     database. These mappings function as the roots of the database's
///     Document tree, pinning documents to the database.

pub trait Db {

    /// Start a new transaction with this database
    fn txn(&self) -> transaction::Transaction;

    /// Open a new group through this database
    fn group(&self, spec: GroupSpec) -> Box<dyn group::Group>;

    /// Open a local cursor on this database
    fn cursor(&self) -> Box<dyn cursor::Cursor>;

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
        self: Box<Self>,
        docs: HashMap<Hash, transaction::DocChange>,
        entries: HashMap<EntryRef, transaction::EntryChange>,
    ) -> DbResult<Result<(), transaction::CommitErrors>>;

    /// Get a schema in the database
    fn schema_get(&self, schema: &Hash) -> DbResult<Option<Arc<Schema>>>;

    /// Get a document directly from the database
    fn doc_get(&self, doc: &Hash) -> DbResult<Option<Arc<Document>>>;
}