use std::collections::{HashMap, HashSet};

use fog_pack::{
    document::{Document, NewDocument},
    entry::{Entry, EntryRef},
    error::Error as FogError,
    schema::{NoSchema, Schema},
    types::*,
};

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

pub struct Transaction {
    db: Box<dyn DbCommit>,
    pub docs: HashMap<Hash, DocChange>,
    pub entries: HashMap<EntryRef, EntryChange>,
}

/// Trying to complete a Schema
pub enum SchemaError {
    MissingSchema(Hash),
    ValidationFail(FogError),
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

    /// Try to add a [`NewDocument`] to the DB. Can fail due to internal database failure,
    pub fn add_new_doc(&mut self, doc: NewDocument) -> DbResult<Result<(), SchemaError>> {
        let (doc, doc_hash) = match doc.schema_hash() {
            Some(schema) => {
                let Some(schema) = self.db.get_schema(schema)? else {
                    return Ok(Err(SchemaError::MissingSchema(schema.to_owned())));
                };
                let doc = match schema.validate_new_doc(doc) {
                    Ok(doc) => doc,
                    Err(e) => return Ok(Err(SchemaError::ValidationFail(e))),
                };
                EncodedDoc::from_doc(Some(schema.as_ref()), doc)
            }
            None => {
                let doc = match NoSchema::validate_new_doc(doc) {
                    Ok(doc) => doc,
                    Err(e) => return Ok(Err(SchemaError::ValidationFail(e))),
                };
                EncodedDoc::from_doc(None, doc)
            }
        };
        match self.docs.entry(doc_hash) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                e.get_mut().add(doc);
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(DocChange::Add {
                    doc: Box::new(doc),
                    weak_ref: HashSet::new(),
                });
            }
        }
        Ok(Ok(()))
    }

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
        doc: Box<EncodedDoc>,
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
    fn add(&mut self, doc: EncodedDoc) {
        if let DocChange::Modify { weak_ref } = self {
            let weak_ref: HashSet<Hash> = weak_ref
                .iter()
                .filter_map(|(k, v)| if *v { Some(k.clone()) } else { None })
                .collect();
            *self = DocChange::Add {
                doc: Box::new(doc),
                weak_ref,
            };
        }
    }
}

pub enum EntryChange {
    Add {
        entry: Box<Entry>,
        ttl: Option<Timestamp>,
        policy: Option<Policy>,
    },
    Modify {
        ttl: Option<Option<Timestamp>>,
        policy: Option<Option<Policy>>,
    },
    Delete,
}
