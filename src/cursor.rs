//! Database cursors, for querying both local and remote databases.
//!
//! The cursor API provides a common interface for requesting data from both the
//! local database and any number of remote nodes. When a cursor is moved
//! around, the request may be fulfilled by any nodes in the
//! [`Group`][crate::group::Group] this cursor was opened on. The same goes for
//! when a cursor is used to make queries: any connected node within the group
//! may respond to the query, and it is up to the various networking
//! implementations to deduplicate query results as best as they are able.
use std::sync::Arc;

use async_trait::async_trait;
use fog_pack::{document::Document, entry::Entry, query::NewQuery, types::*};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::NodeInfo;

#[derive(Clone, Debug, Error)]
#[non_exhaustive]
pub enum CursorError {
    /// The navigated-to document matched the hash, but was invalid somehow - it
    /// either failed validation by its schema or wasn't well-formed. Either
    /// way, it means the hash we were provided leads to nonsense data.
    #[error("Referred-to Document is invalid ({0})")]
    InvalidDoc(Hash),
    /// The document hash requested wasn't in the current document under the
    /// cursor.
    #[error("Hash is not in current document ({0})")]
    NotInDoc(Hash),
}

#[derive(Clone, Copy, Debug, Error)]
#[error("Cursor couldn't go back a step because it was already at the root")]
pub struct CursorBackError;

/// A cursor for navigating through a database.
///
/// A cursor is opened through a specific [`Gate`][crate::gate::Gate] or on the
/// local database, and permits navigation through the database by following
/// Document Hashes or making queries against Documents.
#[async_trait]
pub trait Cursor {
    /// Move the cursor forward by navigating to one of the documents linked to
    /// by the current document. Fails if the requested document hash isn't in
    /// the current document, or if the returned data hashes correctly but isn't
    /// a valid fog-pack document.
    async fn forward(&mut self, hash: &Hash) -> Result<Arc<Document>, CursorError>;

    /// Move the cursor forward only if the requested document is in the local
    /// database. Can fail for the same reasons as [`forward`][Cursor::forward]
    /// but may also return `None` if the local database doesn't have said
    /// document.
    fn forward_local(&mut self, hash: &Hash) -> Result<Option<Arc<Document>>, CursorError>;

    /// Move the cursor back up a level. Fails if the cursor is already at the
    /// earliest point in its history.
    fn back(&mut self) -> Result<(), CursorBackError>;

    /// Fork the cursor. Works like `forward` but produces a new cursor in the
    /// process - one that starts from the document it navigated to.
    fn fork(&self) -> Box<dyn ForkCursor>;

    /// Fork the cursor. Works like `forward_local` but produces a new cursor in
    /// the process - one that starts from the document it navigated to.
    fn fork_local(&self) -> Result<Option<NewCursor>, CursorError> {
        let fork = self.fork();
        fork.complete_local()
    }

    /// Return the document the cursor is currently on.
    fn current(&self) -> Arc<Document>;

    /// Make a query on the current document.
    fn query(self: Box<Self>, query: DbQuery) -> Box<dyn CursorQuery>;
}

/// Successful result of forking a cursor.
pub type NewCursor = (Box<dyn Cursor>, Arc<Document>);

/// An active query on a document.
#[async_trait]
pub trait CursorQuery {
    /// Give up on the query and return to the document the query was made against.
    fn back(self: Box<Self>) -> Box<dyn Cursor>;

    /// Get the next query update.
    async fn next(&self) -> QueryUpdate;

    /// Try to get the next query update, returning `None` if no update is yet
    /// available.
    fn try_next(&self) -> Option<QueryUpdate>;
}

/// A full query made against a database and zero or more remote nodes.
#[derive(Clone, Debug)]
pub struct DbQuery {
    /// The fog-pack query being run against the entries attached to a document.
    pub query: NewQuery,
    /// Set to reverse the result ordering. Normally starts with the
    /// lowest-numbered.
    pub rev_order: bool,
    /// Location of the field to order results by
    pub ordering: Option<Vec<Index>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Index {
    Map(String),
    Array(u32),
}

#[async_trait]
pub trait ForkCursor {
    /// Complete the opening of a new cursor, returning the document it was
    /// commanded to start from.
    async fn complete(self: Box<Self>) -> Result<NewCursor, CursorError>;

    /// Complete the opening of a new cursor, returning the document it was
    /// commanded to start from.
    fn complete_local(self: Box<Self>) -> Result<Option<NewCursor>, CursorError>;
}

/// An indication of how useful a query was. This is advisory information for
/// the network subsystem that returned the query.
pub enum Usefulness {
    /// The received entry is correct and useful to the query maker.
    Useful,
    /// The received entry is correct and relevant to the query, but a newer entry was received
    /// that makes this one useless.
    Stale,
    /// The received entry is correct, but the content was irrelevant. This is slightly worse than
    /// stale: the query maker can't figure out why it would've received this data, even if it is
    /// *technically* correct. Think search results that don't contain any of the search terms, a
    /// query for cat pictures but the returned picture isn't of a cat, that sort of thing.
    Irrelevant,
    /// The received entry was incorrect - despite conforming to the schema, it violated
    /// expectations. Example: a 2D image format where the data length doesn't match up with the
    /// width & height values included in the format.
    Incorrect,
}

/// An entry returned from a query.
pub struct QueryResult {
    /// The entry itself.
    pub entry: Entry,
    /// Any associated documents needed to verify the entry
    pub docs: Vec<Arc<Document>>,
    /// The source node this result came from
    pub source: NodeInfo,
    /// Optional return to indicate how useful this result was to the query maker. Completing this
    /// can help the network eliminate poorly behaved or unhelpful nodes.
    pub useful: Box<dyn UsefulReport>,
    /// Use to fork a cursor into one of the linked documents - either the
    /// attached ones *or* any of the other ones referred to by hash in the
    /// Entry.
    pub fork_spawner: Box<dyn ForkSpawner>,
}

/// Used to fork a querying cursor into one of the documents linked to by a
/// returned Entry.
pub trait ForkSpawner {
    fn fork(&self) -> Box<dyn ForkCursor>;
}

/// Used to report how useful a query result was.
pub trait UsefulReport {
    fn report(self: Box<Self>, useful: Usefulness);
}

/// A update event from an ongoing query.
// Query updates should consist of vastly more QueryResults than connection changes, so the
// overhead from large differences in variants is negligible.
pub enum QueryUpdate {
    /// The query has found a matching entry
    Result(Box<QueryResult>),
    /// The query has found a new node to run the query on
    NewConnection(NodeInfo),
    /// A node the query was being run on became disconnected
    LostConnection(NodeInfo),
}
