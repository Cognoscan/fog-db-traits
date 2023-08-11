/*!
This crate defines the interface to a generic implementation of a fog-pack database (a FogDB).

The Database
------------

A FogDB database consists of a collection of
[Documents][fog_pack::document::Document], each of which is immutable and
referred to by its Hash. Documents can also link to other documents by those
same hashes. The database has a set of named "root documents" that it keeps
resident, and any documents that can be reached by following hash links from
those roots will also be kept resident in the database. In other words, if
you can reach a Document from a root, it stays in the database. If you
can't, it gets evicted from the database. These links can also be "weakened" in
a transaction, much as you can with most reference-tracking garbage collectors.

Documents can adhere to a [Schema][fog_pack::schema::Schema], which constrains
a document's format and provide hints on how to compress it for storage. These
schema let one pre-verify that a document can be deserialized into a data
structure, and let systems know ahead of time what type of data is in a
document.

Now, if the database were just immutable documents, it would be quite difficult
to deal with. That's why every document adhering to a schema can also have
[Entries][fog_pack::entry::Entry], which are essentially smaller documents
attached to a parent document under a key prefix. These entries are not looked
up by their Hash, but are found by running a [Query][fog_pack::query::Query] on
the parent document - in a FogDB, this query will return a sequence of matching
entries, and will remain active in case more entries are found in the future.

The format of entries are also constrained by the parent document's schema,
which puts them in an interesting position for a database, and is what makes
FogDB multi-modal:

- From a document-oriented view, they're a collection of documents all matching the same schema.
- From a relational database view, the parent document & entry key is a table
    reference, and the entries are records (or *entries*, get it?) in the table.
- From a graph database view, the documents are nodes, and the entries are edges.

Rather than provide the expected access APIs for all of these, FogDB provides a
base over which such APIs can be built.

Transactions: Modifying the Database
-----

The database has three ways to modify it:
- Modify the set of root named documents by changing a name-to-hash mapping.
- Modify the set of stored schema by adding or removing a schema document.
- Execute a transaction on the database

Transactions are the most common way to change the database. They follow ACID
properties, so when a transaction is committed, either all parts of the
transaction complete simultaneously or the whole transaction is rejected. Most
commonly, the transaction might fail if attempting to delete an entry that has
already been removed - this is how compare-and-swap type transactions can be
done to the database.

Transactions can do the following:
- Add a document to the database
- Weaken/strengthen document hash links
- Add an entry to the database, optionally setting a time-to-live or an access
    policy
- Modify an entry's time-to-live or its access policy
- Delete an entry from the database

Documents cannot be deleted directly; instead, when they are no longer reachable
from the named root documents, they are automatically garbage-collected.

Note that all transactions will only execute on the local FogDB instance; this
follows the rule of the system can only modify itself, and it is up to other
database nodes to modify themselves to match as they desire.

Cursors: Reading the Database
------

The database is accessed through the [Cursor][cursor] interface. A
[cursor][cursor::Cursor] can be opened either on a [Group][group::Group::cursor]
(see [Connecting to Other Databases](#groups-connecting-to-other-databases)) or
on the [database][Db::cursor]. A cursor must start from some specific Document,
and can be thought of as always being "over" a document.  Each document can
contain hashes of other documents; the cursor can follow these with a "forward"
function call. Alternately, a new cursor can be "forked" off to the linked
document. In this way, many cursors can be created for quicker traversal of a
Document tree.

A cursor can also be used to make a query, which uses up the cursor and turns it
into a [CursorQuery][cursor::CursorQuery] (which can be backed out of to get the
cursor back). This yields a stream of entries from the document the cursor is over.

A query is just a fog-pack [Query][fog_pack::query::Query] with an optional
preferred ordering to the returned Entry [results][cursor::QueryResult]. If an
entry has hash links to documents, new cursors can be forked off to them using
the included [`ForkSpawner`][cursor::ForkSpawner].

Here's where it gets interesting: if a cursor was opened up on a
[Group][group::Group], then any remote databases meeting the group's
requirements can also be read by a cursor. In this way, many databases at once
can be used to simultaneously retrieve documents and give query results, which
is why each query result includes the source database it was retrieved from.

This means that document retrieval is near-instant when the local database has
the document, but a cursor can indefinitely go searching through remote
databases in search of one that has the requested document. By forking off many
cursors at once, the network can use an entire swarm of remote databases to
retrieve the documents.

Groups: Connecting to other Databases
-----

Each FogDB instance exists as a single Node, which may use any number of
network protocols to communicate with other Nodes. This lets the [cursor]
interface use many remote databases at once to retrieve documents and get query
results, and lets portions of the database be exposed to other nodes in turn.

Connecting to other nodes is done by [opening a group][Db::group] using a [group
specification][group::GroupSpec]. This specification limits the network types
over which the group will find other nodes, how it can find and connect to them,
and if the nodes must identify themselves as part of a Policy (see [Policies and
Certificates](#policies-and-certificates)).

Node discovery can be limited to these approximate network classes:

- Machine: communication between other running FogDB instances on the same
    computer.
- Direct: Direct machine-to-machine networking, with no switches or routers
    present. Primary example is WiFi Direct.
- Local: local networks. LANs, ad-hoc networks, and other physically close
    networking systems fall under this category.
- Regional: A collection of local networks that isn't the internet. Campus
    networks and Metropolitan area networks fall under this category. The IPv6
    "organization-level" multicast scope also fits.
- Global: the global internet.

Once a group is opened, the various underlying network protocols will attempt to
establish a collection of nodes that fit the group's specification, and will
work to set up and maintain node discovery mechanisms for the group.

Gates: Making the Database Remotely Available
-----

When a group is established, it's not enough to actually communicate between
database nodes. Each node must choose what parts of the database to expose to
remote nodes, and this is done by creating a [Gate][gate::Gate]. A gate allows
remote nodes to open a database cursor starting at a specific document, given
when the gate is [opened][group::Group::gate].

Gates provide the means to easily scope access to the database: anything that
can be reached from the starting document is fair game for access by a remote
node in the Group. Queries can also be made on reached documents. Entries can
have additional access policies that a node must match in order to be given the
entry; otherwise it is skipped over.

When a query is made on a particular document reached through a gate, you can
optionally [hook into the query][gate::Gate::query_hook] and manually provide
query results. This allows for dynamic generation of query responses, and can be
used to build RPC-like mechanisms from the query system.

Policies and Certificates
-------------------------

Policies are FogDB's way of scoping access to a database, and make use of
fog-pack [Identities][fog_pack::types::Identity] to do so. An Identity is a
public-private keypair, which can be used to sign documents and entries, and
generally establish a unique identity.

Nodes can identify themselves on the network using these long-term signing keys.
A full [Node Address][NodeAddr] consists of a long-term key like this, and an
ephemeral key pair that is regenerated by each network protocol every time a
group is created.  Not all nodes will have these Identities, but they're
required when joining any group with a policy in place.

Identities can be used to sign a special document called a
[Certificate][cert::Cert]. Certificates are identified by their signer, the
subject Identity, a Hash value acting as a context, and a key string.
Certificates are immutable, but new ones with the same
signer/subject/context/key combination can be made in order to replace previous
ones - this also serves as a way to revoke certificates. See [the
documentation][cert::Cert] for more info.

Certificates on their own do nothing, but with a [Policy][cert::Policy] they can
delegate access permissions. A policy can be as simple as a list of permitted
Identities, but they can also include [Policy Chains][cert::PolicyChain], which
allow certificates to be used to establish permission.

Policies and Certificates are automatically propagated through databases; they
must be actively retrieved or exchanged as part of a network protocol. FogDB
doesn't specify any particular mechanism for this, leaving it up to applications
and network protocols to propagate certificates and set policies. It's assumed
that, as part of a FogDB setup, certificates will be stored in the database and
be used to check policies.

*/

use std::{collections::{HashMap, BTreeMap}, error::Error, sync::Arc};

use async_trait::async_trait;
use cursor::{DbQuery, CursorQuery};
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
    /// Direct machine-to-machine communication
    pub direct: bool,
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
    pub net: NetType,
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
    Direct,
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
    fn cursor(&self) -> cursor::NewCursor;

    /// Get a document directly from the database
    fn doc_get(&self, doc: &Hash) -> DbResult<Option<Arc<Document>>>;

    /// Make a query directly on the database
    fn query(&self, doc: &Hash, query: DbQuery) -> Box<dyn CursorQuery>;

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