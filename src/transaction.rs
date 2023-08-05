use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use fog_pack::{
    document::{Document, NewDocument},
    entry::{Entry, EntryRef, NewEntry},
    error::Error as FogError,
    schema::{NoSchema, Schema},
    types::*,
};
use thiserror::Error;

use crate::{DbCommit, DbResult, Policy};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitError {
    /// Tried to change or delete an entry but it wasn't in the DB
    MissingEntry(EntryRef),
    /// Tried to add an entry but its parent document wasn't in the DB
    MissingParent(EntryRef),
    /// Tried to change a document's references but it wasn't in the DB
    MissingDoc(Hash),
    /// Tried to change a document's references but the ref wasn't in the document
    MissingDocRef { doc: Hash, target: Hash },
    /// Tried to add a document but the schema was missing
    MissingSchema { doc: Hash, schema: Hash },
}

pub struct CommitErrors {
    pub docs: HashMap<Hash, DocChange>,
    pub entries: HashMap<EntryRef, EntryChange>,
    pub errors: Vec<CommitError>,
}

/// A pending transaction to execute on a database.
pub struct Transaction {
    db: Box<dyn DbCommit>,
    docs: HashMap<Hash, DocChange>,
    entries: HashMap<EntryRef, EntryChange>,
}

/// Failure while trying to find and complete a schema
#[derive(Clone, Debug, Error)]
pub enum SchemaError {
    #[error("Missing schema {0}")]
    MissingSchema(Hash),
    #[error("Validation failed")]
    ValidationFail(#[from] FogError),
}

/// Failure while trying to find a schema for a document
#[derive(Clone, Debug)]
pub struct MissingSchema(pub Hash);

impl std::fmt::Display for MissingSchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Missing schema {0}")
    }
}

impl std::error::Error for MissingSchema {}

/// Failure while processing an entry
#[derive(Clone, Debug, Error)]
pub enum EntryError {
    #[error("Entry needed missing schema {0}")]
    MissingEntrySchema(Hash),
    #[error("Entry Validation failed")]
    EntryValidationFail(#[from] FogError),
    #[error("Document validation failed within context of entry (doc = {doc})")]
    DocValidationFail { doc: Hash, source: FogError },
    #[error("Missing document {0}")]
    MissingDoc(Hash),
}

impl Transaction {
    pub fn new(db: Box<dyn DbCommit>) -> Self {
        Self {
            db,
            docs: HashMap::new(),
            entries: HashMap::new(),
        }
    }

    /// Replace the current transaction with whatever transaction errored out last time.
    pub fn load_from_errors(&mut self, errs: CommitErrors) {
        self.docs = errs.docs;
        self.entries = errs.entries;
    }

    /// Try to add a [`NewDocument`] to the DB. Can fail due to internal
    /// database failure. It can also fail if the document's schema isn't in the
    /// database, or if validation fails. On success, it returns a copy of the
    /// document that will be committed.
    pub fn add_new_doc(
        &mut self,
        doc: NewDocument,
    ) -> DbResult<Result<Arc<Document>, SchemaError>> {
        let (doc, (encoded, doc_hash)) = match doc.schema_hash() {
            Some(schema) => {
                let Some(schema) = self.db.schema_get(schema)? else {
                    return Ok(Err(SchemaError::MissingSchema(schema.to_owned())));
                };
                let doc = match schema.validate_new_doc(doc) {
                    Ok(doc) => doc,
                    Err(e) => return Ok(Err(SchemaError::ValidationFail(e))),
                };
                let doc = Arc::new(doc);
                (
                    doc.clone(),
                    EncodedDoc::from_doc(Some(schema.as_ref()), doc.as_ref().clone()),
                )
            }
            None => {
                let doc = match NoSchema::validate_new_doc(doc) {
                    Ok(doc) => doc,
                    Err(e) => return Ok(Err(SchemaError::ValidationFail(e))),
                };
                let doc = Arc::new(doc);
                (
                    doc.clone(),
                    EncodedDoc::from_doc(None, doc.as_ref().clone()),
                )
            }
        };
        let encoded = Box::new(encoded);
        match self.docs.entry(doc_hash) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                e.get_mut().add(encoded, doc.clone());
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(DocChange::Add {
                    doc: doc.clone(),
                    encoded,
                    weak_ref: HashSet::new(),
                });
            }
        }
        Ok(Ok(doc))
    }

    /// Try to add a [`Document`] to the DB. Can fail due to internal
    /// database failure. It can also fail if the document's schema isn't in the
    /// database.
    pub fn add_doc(&mut self, doc: Arc<Document>) -> DbResult<Result<(), MissingSchema>> {
        let (encoded, doc_hash) = match doc.schema_hash() {
            Some(schema) => {
                let Some(schema) = self.db.schema_get(schema)? else {
                    return Ok(Err(MissingSchema(schema.to_owned())));
                };
                EncodedDoc::from_doc(Some(schema.as_ref()), doc.as_ref().clone())
            }
            None => EncodedDoc::from_doc(None, doc.as_ref().clone()),
        };
        let encoded = Box::new(encoded);
        match self.docs.entry(doc_hash) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                e.get_mut().add(encoded, doc);
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(DocChange::Add {
                    encoded,
                    doc,
                    weak_ref: HashSet::new(),
                });
            }
        }
        Ok(Ok(()))
    }

    /// Try to add a [`NewEntry`] to the DB. Can fail due to internal database
    /// failure, if the schema is missing from the database, or if any of the
    /// documents needed for validation are missing from both the transaction
    /// and the database.
    pub fn add_new_entry(&mut self, entry: NewEntry) -> DbResult<Result<(), EntryError>> {
        let Some(schema) = self.db.schema_get(entry.schema_hash())? else {
            return Ok(Err(EntryError::MissingEntrySchema(entry.schema_hash().to_owned())));
        };
        let mut checklist = match schema.validate_new_entry(entry) {
            Ok(list) => list,
            Err(e) => return Ok(Err(EntryError::EntryValidationFail(e))),
        };
        for (link_hash, item) in checklist.iter() {
            if let Some(DocChange::Add { doc, .. }) = self.docs.get(&link_hash) {
                if let Err(e) = item.check(doc) {
                    return Ok(Err(EntryError::DocValidationFail {
                        doc: link_hash,
                        source: e,
                    }));
                }
                continue;
            }
            if let Some(doc) = self.db.doc_get(&link_hash)? {
                if let Err(e) = item.check(&doc) {
                    return Ok(Err(EntryError::DocValidationFail {
                        doc: link_hash,
                        source: e,
                    }));
                }
                continue;
            }
            return Ok(Err(EntryError::MissingDoc(link_hash)));
        }
        let entry = checklist.complete().unwrap();
        let (entry, e_ref) = EncodedEntry::from_entry(&schema, entry);
        let entry = Box::new(entry);
        match self.entries.entry(e_ref) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                e.get_mut().add(entry);
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(EntryChange::Add {
                    entry,
                    ttl: None,
                    policy: None,
                });
            }
        }
        Ok(Ok(()))
    }

    /// Try to add a [`Entry`] to the DB. Can fail due to internal database
    /// failure, or if the schema is missing from the database.
    pub fn add_entry(&mut self, entry: Entry) -> DbResult<Result<(), EntryError>> {
        let Some(schema) = self.db.schema_get(entry.schema_hash())? else {
            return Ok(Err(EntryError::MissingEntrySchema(entry.schema_hash().to_owned())));
        };
        let (entry, e_ref) = EncodedEntry::from_entry(&schema, entry);
        let entry = Box::new(entry);
        match self.entries.entry(e_ref) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                e.get_mut().add(entry);
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(EntryChange::Add {
                    entry,
                    ttl: None,
                    policy: None,
                });
            }
        }
        Ok(Ok(()))
    }

    /// Weaken/strengthen a reference for a Document.
    pub fn set_weak_ref(&mut self, doc: &Hash, ref_hash: &Hash, weak: bool) {
        match self.docs.entry(doc.to_owned()) {
            std::collections::hash_map::Entry::Occupied(mut e) => match e.get_mut() {
                DocChange::Add { weak_ref, .. } => {
                    if weak {
                        weak_ref.insert(ref_hash.to_owned());
                    } else {
                        weak_ref.remove(ref_hash);
                    }
                },
                DocChange::Modify { weak_ref } => {
                    weak_ref.insert(ref_hash.to_owned(), weak);
                }
            },
            std::collections::hash_map::Entry::Vacant(v) => {
                let mut weak_ref = HashMap::new();
                weak_ref.insert(ref_hash.to_owned(), weak);
                v.insert(DocChange::Modify { weak_ref });
            }
        }
    }

    /// Set or clear the time-to-live for an Entry.
    pub fn set_ttl(&mut self, entry: &EntryRef, ttl: Option<Timestamp>) {
        let set = ttl;
        match self.entries.entry(entry.to_owned()) {
            std::collections::hash_map::Entry::Occupied(mut e) => match e.get_mut() {
                EntryChange::Add { ttl, .. } => {
                    *ttl = set;
                },
                EntryChange::Modify { ttl, .. } => {
                    *ttl = Some(set);
                }
                EntryChange::Delete => (),
            },
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(EntryChange::Modify { ttl: Some(set), policy: None });
            }
        }
    }

    /// Set or clear the policy for an Entry.
    pub fn set_policy(&mut self, entry: &EntryRef, policy: Option<Policy>) {
        let set = policy;
        match self.entries.entry(entry.to_owned()) {
            std::collections::hash_map::Entry::Occupied(mut e) => match e.get_mut() {
                EntryChange::Add { policy, .. } => {
                    *policy = set;
                },
                EntryChange::Modify { policy, .. } => {
                    *policy = Some(set);
                }
                EntryChange::Delete => (),
            },
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(EntryChange::Modify { policy: Some(set), ttl: None });
            }
        }
    }

    /// Delete an entry from the database.
    pub fn del_entry(&mut self, entry: &EntryRef) {
        self.entries.insert(entry.to_owned(), EntryChange::Delete);
    }

    /// Commit this transaction to the database. This can fail due to internal
    /// database errors, but it can also fail any of the various [`CommitError`]
    /// reasons.
    pub async fn commit(self) -> DbResult<Result<(), CommitErrors>> {
        self.db.commit(self.docs, self.entries).await
    }
}

pub struct EncodedDoc {
    schema: Option<Hash>,
    data: Vec<u8>,
    refs: Vec<Hash>,
}

impl EncodedDoc {
    pub fn from_doc(schema: Option<&Schema>, doc: Document) -> (Self, Hash) {
        let refs = doc.find_hashes();
        let schema_hash = doc.schema_hash().cloned();
        let (hash, data) = if let Some(schema) = schema {
            schema.encode_doc(doc).unwrap()
        } else {
            NoSchema::encode_doc(doc).unwrap()
        };
        (
            Self {
                schema: schema_hash,
                data,
                refs,
            },
            hash,
        )
    }

    pub fn schema(&self) -> &Option<Hash> {
        &self.schema
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get all hashes that were in the document
    pub fn refs(&self) -> &[Hash] {
        &self.refs
    }
}

pub struct EncodedEntry {
    data: Vec<u8>,
    all_refs: Vec<Hash>,
    required_refs: Vec<Hash>,
}

impl EncodedEntry {
    pub fn from_entry(schema: &Schema, entry: Entry) -> (Self, EntryRef) {
        let all_refs = entry.find_hashes();
        let (e_ref, data, required_refs) = schema.encode_entry(entry).unwrap();
        (
            Self {
                data,
                all_refs,
                required_refs,
            },
            e_ref,
        )
    }

    /// Get the encoded entry data
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get all hashes that were in the entry
    pub fn all_refs(&self) -> &[Hash] {
        &self.all_refs
    }

    /// Get the hashes that are required for validation
    pub fn required_refs(&self) -> &[Hash] {
        &self.required_refs
    }
}

pub enum DocChange {
    /// Add a document to the DB
    Add {
        /// Document to add to the DB
        encoded: Box<EncodedDoc>,
        /// Actual document, unencoded so we can still check it as needed
        doc: Arc<Document>,
        /// Set of references to weaken
        weak_ref: HashSet<Hash>,
    },
    /// Change the metadata of a document in the DB.
    Modify {
        /// Set to true to make a reference weak
        weak_ref: HashMap<Hash, bool>,
    },
}

impl DocChange {
    fn add(&mut self, encoded: Box<EncodedDoc>, doc: Arc<Document>) {
        if let DocChange::Modify { weak_ref } = self {
            let weak_ref: HashSet<Hash> = weak_ref
                .iter()
                .filter_map(|(k, v)| if *v { Some(k.clone()) } else { None })
                .collect();
            *self = DocChange::Add {
                encoded,
                doc,
                weak_ref,
            };
        }
    }
}

pub enum EntryChange {
    Add {
        entry: Box<EncodedEntry>,
        ttl: Option<Timestamp>,
        policy: Option<Policy>,
    },
    Modify {
        ttl: Option<Option<Timestamp>>,
        policy: Option<Option<Policy>>,
    },
    Delete,
}

impl EntryChange {
    fn add(&mut self, entry: Box<EncodedEntry>) {
        match self {
            EntryChange::Modify { ttl, policy } => {
                *self = EntryChange::Add {
                    entry,
                    ttl: ttl.unwrap_or_default(),
                    policy: policy.clone().unwrap_or_default(),
                };
            }
            EntryChange::Delete => {
                *self = EntryChange::Add {
                    entry,
                    ttl: None,
                    policy: None,
                };
            }
            EntryChange::Add { .. } => (),
        }
    }
}
