//! Group Management Interfaces, for handling active network groups
//!
//! Groups are a way to conceptually aggregate multiple nodes and treat them as
//! a single network. Nodes can be part of multiple groups at a time. A group of
//! nodes can be aggregated over multiple network types, and can be specified by
//! a [`Policy`].

use fog_crypto::identity::IdentityKey;
use fog_pack::types::*;

use crate::{gate::{GateSettings, Gate}, cursor::ForkCursor, cert::Policy, NetInfo};

pub trait Group {
    /// Open up a gate, which lets members of this group open a cursor in your
    /// database starting from the given hash. Dropping the Gate closes it.
    ///
    /// Multiple gates can be open at once, but gates with overlapping
    /// parameters cannot be open at the same hash, in which case this function
    /// should return None.
    ///
    /// For a given hash:
    ///
    /// 1. If a gate is open without a specific node listed, no other gates may
    ///     be opened.
    /// 2. If at least one gate is open with a specific node listed, other gates
    ///     with specific nodes may be opened, but all nodes across open gates
    ///     must be unique.
    ///
    fn gate(&self, gate: &Hash, settings: Option<GateSettings>) -> Option<Box<dyn Gate>>;

    /// Prepare a new cursor for use, starting from the given hash.
    fn cursor(&self, gate: &Hash) -> Box<dyn ForkCursor>;
}

/// Specification for a group. This limits what networks will be used for the
/// group, whether mixnet capabilities are required, and what specific nodes are
/// allowed into the group.
pub struct GroupSpec {
    /// What public key (if any) should be used to identify oneself to other
    /// nodes, and optionally what policy should be used to limit the group to
    /// only nodes whose key passes said policy.
    pub policy_settings: Option<(IdentityKey, Option<Policy>)>,
    /// What networks should be used when navigating this group.
    pub net: NetInfo,
    /// Whether or not a mixnet must be used when finding group members
    pub mixnet_locator: bool,
    /// Whether or not a mixnet must be used when communicating with group members.
    pub mixnet_comms: bool,
}
