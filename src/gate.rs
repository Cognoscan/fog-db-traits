use std::{sync::Arc, fmt::Display};

use crate::{Policy, NodeAddr};
use async_trait::async_trait;
use fog_pack::{types::Hash, document::Document, entry::Entry, query::Query};
use thiserror::Error;


pub struct GateSettings {
    /// An advisory policy for which nodes to give preferential treatment to.
    pub prefer: Policy,
    /// Open the gate for *only* these specific nodes
    pub nodes: Vec<NodeAddr>,
}

/// An open Gate. Allows other nodes in a network to read the database with a
/// cursor, starting from the hash at which the gate was opened. Any document
/// that can be navigated to is thus visible to other nodes. An exception is for
/// some entries - certain entries may be marked with a policy that further
/// limits visibility to the network, and those entries will not be available
/// for cursor navigation.
pub trait Gate {
    /// Get a list of what nodes are currently actively using a cursor within
    /// this gate.
    fn attached(&self) -> Vec<NodeAddr>;

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
pub struct  ResponseError(pub Response);

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