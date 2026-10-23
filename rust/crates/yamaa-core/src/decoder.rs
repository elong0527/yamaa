//! The decoder port: the engine turns held bytes into what it reads through
//! it, a document as written with the position of each node, a file a project
//! names admitted as the type it is read as, or a table by an input's profile.
//! The adapters implement it with the YAML reader, the shipped closures and
//! the CSV, Parquet, SAS and ODM codecs; core holds no codec.
use crate::{
    producer_contract::Contract,
    project_environment::LockKind,
    project_preparation::{DecodeTarget, Source},
    schema::Document,
    specification::{SourceDeclaration, SourceInterpretation},
    table::TableAccess,
};
use std::any::Any;

/// The engine's one way from held bytes to what it reads. Each request reads
/// only the bytes it is given, applies its own physical limits before it
/// allocates, and returns a value that owns its storage: no borrowed input
/// and no temporary host buffer.
pub trait Decoder {
    /// A held file decoded as written, keeping where each node was written.
    type Raw;
    /// A file a project names, decoded and admitted as the type it is read as,
    /// keeping its held bytes and where each node was written.
    type Admitted;
    /// Why held bytes did not decode, or did not admit, as a document.
    type DocumentError;
    /// Why held bytes did not decode as a table.
    type Error;
    /// The table an input decodes to.
    type Table: TableAccess;
    /// Decode the held file `identity` names as written: a specification's
    /// parent layer, or a dictionary file a specification names.
    fn raw(&mut self, identity: &str, bytes: &[u8]) -> Result<Self::Raw, Self::DocumentError>;
    /// The document a raw decoding holds.
    fn raw_document(raw: &Self::Raw) -> &Document;
    /// Decode `source`, a file a project names, and admit it as `target`
    /// without lowering it.
    fn admitted(
        &mut self,
        source: Source,
        target: DecodeTarget,
    ) -> Result<Self::Admitted, Self::DocumentError>;
    /// The admitted document of a project's file.
    fn admitted_document(admitted: &Self::Admitted) -> &Document;
    /// The held file a project's file was admitted from.
    fn admitted_source(admitted: &Self::Admitted) -> &Source;
    /// The packaging tool whose lock syntax the held bytes are, or `None`
    /// when they are no lock the engine reads.
    fn lock(&mut self, source: &Source) -> Option<LockKind>;
    /// Decode one captured input with its declared logical types, applying
    /// the declared interpretation and any linked producer contract before
    /// value conversion.
    fn table(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        interpretation: SourceInterpretation,
        contract: Option<&Contract>,
    ) -> Result<Self::Table, Self::Error>;
    /// Admit a table the resolver served in place of bytes, owned
    /// independently of host handles, through the same binding lifecycle, or
    /// `None` when this decoder reads no such table.
    fn served_table(
        &mut self,
        _source: &SourceDeclaration,
        _table: &(dyn Any + Send + Sync),
        _interpretation: SourceInterpretation,
        _contract: Option<&Contract>,
    ) -> Option<Result<Self::Table, Self::Error>> {
        None
    }
    /// Whether a failed input is an ingestion finding, after which the
    /// remaining declared inputs still decode. A resource, transport or
    /// opaque boundary failure ends the attempt.
    fn continue_after(&self, error: &Self::Error) -> bool;
}
