use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use fog_pack::{document::Document, types::*};
use thiserror::Error;

use crate::{NodeAddr, Policy};

pub trait Group {
    /// Open up a gate, which lets members of this group open a cursor in your
    /// database starting from the given hash. Dropping the Gate closes it.
    ///
    /// Multiple gates can be open at once, but only one gate can be open at
    /// each Hash. This function should return None if there is already a gate
    /// open at the hash.
    fn gate(&self, gate: &Hash, settings: Option<GateSettings>) -> Option<Box<dyn Gate>>;

    /// Prepare a new cursor for use, starting from the given hash.
    fn cursor(&self, gate: &Hash) -> Box<dyn ForkCursor>;
}

#[non_exhaustive]
pub enum CursorError {
    /// The navigated-to document matched the hash, but was invalid somehow - it
    /// either failed validation by its schema or wasn't well-formed. Either
    /// way, it means the hash we were provided leads to nonsense data.
    InvalidDoc(Hash),
}

#[derive(Clone, Copy, Debug, Error)]
#[error("Cursor couldn't go back a step because it was already at the root")]
pub struct CursorBackError;

#[async_trait]
pub trait Cursor {
    /// Move the cursor forward by navigating to one of the documents linked to
    /// by the current document.
    async fn forward(&mut self, hash: &Hash) -> Result<Arc<Document>, CursorError>;

    /// Move the cursor back up a level. Fails if the cursor is already at the
    /// earliest point in its history.
    fn back(&mut self) -> Result<(), CursorBackError>;

    /// Fork the cursor. Works like `forward` but produces a new cursor in the
    /// process - one that starts from the document it navigated to.
    fn fork(&self) -> Box<dyn ForkCursor>;
}

#[async_trait]
pub trait ForkCursor {
    /// Complete the opening of a new cursor, returning the document it was
    /// commaned to start from.
    async fn complete(self) -> Result<(Box<dyn Cursor>, Arc<Document>), CursorError>;
}

pub struct GateSettings {
    /// An advisory policy for which nodes to give preferential treatment to.
    pub prefer: Policy,
}

/// Specification for a group. This limits what networks will be used for the
/// group, whether mixnet capabilities are required, and what specific nodes are
/// allowed into the group.
pub struct GroupSpec {
    pub net_machine: bool,
    pub net_local: bool,
    pub net_regional: bool,
    pub net_global: bool,
    pub net_other: BTreeMap<String, BTreeMap<String, String>>,
    /// Whether or not a mixnet must be used when finding group members
    pub mixnet_locator: bool,
    /// Whether or not a mixnet must be used when communicating with group members.
    pub mixnet_comms: bool,
    /// A Policy for limiting which nodes the group is in contact with. Only
    /// nodes whose permanent Identity passes the policy are allowed into the
    /// group.
    pub policy: Policy,
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

    /// Explicitly close the gate - should be equivalent to calling `drop(gate)`.
    fn close(self);
}
