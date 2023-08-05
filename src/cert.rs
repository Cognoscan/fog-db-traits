use fog_pack::types::*;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU8;

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
/// A certificate is valid for a matching subject/context/key if:
/// - the current time is between the start & end times
/// - "valid" is set to true
///
/// A certificate database generally only keeps one certificate for a given
/// subject/context/key/signer combination. When deciding which of two certificates
/// to keep, it should do the following:
///
/// 1. Pick the one with the higher start time
/// 2. If start times match, pick the one with the higher sequence number
/// 3. If the sequence numbers also match, prefer the stored one.
///
/// A database should also record the highest end time it has seen for a given
/// certificate combo, as this lets it know when it can discard the certificate.
///
/// Sometimes, issuing this certificate requires that another be revoked at the
/// same time - for instance, if an authority is being transferred. In this
/// case, a "revokes" option should be added that details the revocation of
/// another certificate. If the revocation rule is valid and can be executed
/// successfully by the database, then this certificate is valid. Otherwise,
/// this certificate shouldn't be accepted.
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
    //#[fog(query, ord)]
    pub seq: u64,
    //#[fog(max_len = 255, query, ord)]
    pub start: Timestamp,
    //#[fog(max_len = 255, query, ord)]
    pub end: Timestamp,
    //#[fog(query)]
    pub valid: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revokes: Option<CertReplace>,
}

/// A certificate replacement statement. Replace the certificate under the
/// `revoke` hash with the one at the `replace_with` hash. Replacement should
/// fail if the `revoke` & `replace_with` hashes don't share the exact same
/// subject/context/key/signer set.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CertReplace {
    pub revoke: Hash,
    pub replace_with: Hash,
}

impl Cert {
    /// Check for validity. If no time is provided, the start & end times are ignored.
    pub fn is_valid(&self, time: Option<Timestamp>) -> bool {
        if let Some(time) = time {
            if time < self.start || time > self.end {
                return false;
            }
        }
        self.valid
    }

    /// Determine if two certificates are equal in subject/context/key
    pub fn key_eq(&self, other: &Cert) -> bool {
        self.subject == other.subject && self.context == other.context && self.key == other.key
    }

    /// Determine if the provided certificate should replace this one.
    pub fn should_replace(&self, other: &Cert) -> bool {
        (other.start > self.start) || ((other.start == self.start) && (other.seq > self.seq))
    }
}
