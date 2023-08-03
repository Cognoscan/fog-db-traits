use std::num::NonZeroU8;
use fog_pack::types::*;
use serde::{Deserialize, Serialize};

/// A Policy, which specifies what requirements an identity must meet to be
/// accepted by the policy. If the chains are empty, an identity must be amongst
/// the listed root identities. If the chains are *not* empty, the identity must
/// either be amongst the roots, or it must satisfy the rules in any one of the
/// chains.
///
/// A chain is a sequence of links. A certificate database should start at the
/// last link, looking for identities that have created a [`Cert`] that is
/// valid, matches the key/val pair in the link, matches the context in the
/// overall Policy, and has the checked-for Identity as the subject. Amongst the
/// resulting identities, the next link in the chain should be checked, and so
/// on until hitting a root Identity.
///
/// There must be at least `min_issuers` valid Identities that issued a
/// certificate matching the link's rule in order for the link to be fully
/// fulfilled.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Policy {
    pub context: Hash,
    //#[fog(min_len = 1)]
    pub roots: Vec<Identity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chains: Vec<PolicyChain>,
}

/// A policy chain. Each link represents a requirement that an identity must
/// meet in order to act as a signer for the subsequent link.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PolicyChain {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chain: Vec<PolicyLink>,
}

/// A link in a policy chain. Consists of a key-value pair, and how many Identities meeting
/// the previous link requirements must have issued a certificate asserting the
/// key-value pair for an Identity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PolicyLink {
    //#[fog(max_len = 255)]
    key: String,
    //#[fog(max_len = 255)]
    val: String,
    min_issuers: NonZeroU8,
}

/// A certificate, which can be encoded as a fog-pack
/// [`Document`][fog_pack::document::Document] and signed.
///
/// A certificate is valid as long as the end time is greater than the start
/// time, and the end time is past the system's clock time.
///
/// A certificate database generally only keeps one certificate for a given
/// subject/context/key/val combination. When deciding which of two certificates
/// to keep, it should always pick the one with the higher start time. It should
/// also record the highest end time it has seen for a given certificate combo,
/// as this lets it know when it can discard the certificate.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cert {
    //#[fog(query)]
    pub subject: Identity,
    //#[fog(query)]
    pub context: Hash,
    //#[fog(max_len = 255, query)]
    pub key: String,
    //#[fog(max_len = 255, query)]
    pub val: String,
    //#[fog(max_len = 255, query, ord)]
    pub start: Timestamp,
    //#[fog(max_len = 255, query, ord)]
    pub end: Timestamp,
}

impl Cert {
    pub fn is_valid(&self) -> bool {
        self.end > self.start
    }
}
