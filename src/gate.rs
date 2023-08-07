//! The Database access Gate interface. Used for opening up the database to
//! remote nodes.
//!
//! A [`Gate`] is an open access point by which remote nodes can connect to your
//! local database for document retrieval and querying, via the
//! [`cursor`][crate::cursor] API.

use std::{fmt::Display, sync::Arc};

use crate::{cert::Policy, NodeInfo};
use crate::NodeAddr;
use async_trait::async_trait;
use fog_pack::{document::Document, entry::Entry, query::Query, types::Hash};
use thiserror::Error;

pub struct GateSettings {
    /// An advisory policy for which nodes to give preferential treatment to.
    pub prefer: Policy,
    /// Open the gate for *only* these specific nodes
    pub nodes: Vec<NodeAddr>,
    /// How many cursors a node is permitted to have open through this gate.
    pub cursors: u32,
    /// How many total cursors may be opened within this gate
    pub total_cursors: u32,
}

/// An open Gate. Allows other nodes in a network to read the database with a
/// cursor, starting from the hash at which the gate was opened. Any document
/// that can be navigated to is thus visible to other nodes. An exception is for
/// some entries - certain entries may be marked with a policy that further
/// limits visibility to the network, and those entries will not be available
/// for cursor navigation.
pub trait Gate {
    /// Get a list of what nodes are currently actively using a cursor within
    /// this gate, and how many cursors they have open.
    fn attached(&self) -> Vec<(NodeInfo, u32)>;

    /// How many cursors are currently open on this gate.
    fn total_cursors(&self) -> u32;

    /// Add a hook for handling all incoming queries on a specific document,
    /// scoped to just nodes that came in through this Gate. When a hook is
    /// established, *all* queries go through it - none will ever hit the
    /// database. It's up to the hook to pass queries on to the database, should
    /// it choose to do so.
    fn query_hook(&self, doc: &Hash, hook: Box<dyn QueryHook>);

    /// Explicitly close the gate - should be equivalent to calling `drop(gate)`.
    fn close(self);
}

#[async_trait]
pub trait ResponseStream {
    /// Send a response to the query. Should fail if the query is closed.
    async fn send(&self, response: Response) -> Result<(), ResponseError>;

    /// Try to send a response to the query. Should fail if the query is closed,
    /// or if this stream cannot currently accept a response.
    fn try_send(&self, response: Response) -> Result<(), Box<TryResponseError>>;

    /// Return true if the query is closed.
    fn is_closed(&self) -> bool;
}

/// Failure to send a hook response
#[derive(Clone, Debug)]
pub struct ResponseError(pub Response);

impl Display for ResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Response stream is closed")
    }
}

impl std::error::Error for ResponseError {}

/// Failure to try sending a hook response
#[derive(Clone, Debug, Error)]
pub enum TryResponseError {
    #[error("Response stream is full")]
    Full(Response),
    #[error("Response stream is closed")]
    Closed(Response),
}

#[derive(Clone, Debug)]
pub struct Response {
    pub entry: Entry,
    /// Associated documents needed to complete the entry. They should *only* be
    /// ones that are required by the entry, or this response may be dropped.
    pub docs: Vec<Arc<Document>>,
}

#[async_trait]
pub trait QueryHook {
    /// Handle an incoming query.
    /// If the query is considered malformed or malicious, return false. If the
    /// query is valid, return true. Valid queries with no results should still
    /// return true, and the response object should be dropped.
    fn handle(&self, incoming: Query, responses: Box<dyn ResponseStream>) -> bool;
}
