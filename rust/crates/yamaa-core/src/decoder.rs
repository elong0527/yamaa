//! The decoder port: the engine turns held bytes into what it reads through
//! it, a document as written with the position of each node, a file a project
//! names admitted as the type it is read as, or a table by an input's profile.
//! The adapters implement it with the YAML reader, the shipped closures and
//! the CSV, Parquet, SAS and ODM codecs; core holds no codec. A CSV or
//! Parquet input can also be read by partition (#3792): a scan decodes every
//! partition once, in order, to find what the whole read finds, keeping at
//! most one partition's records of them (#4212), and each range of records
//! then decodes again when the run reads it, but for the records the scan
//! kept (#3830).
use crate::{
    producer_contract::Contract,
    project_environment::LockKind,
    project_preparation::{DecodeTarget, Source},
    schema::{Document, SchemaStructure},
    specification::{SourceDeclaration, SourceInterpretation},
    table::{TableAccess, TableSchema},
};
use std::{any::Any, num::NonZeroUsize, ops::Range};

/// A decode of a range of a scanned input's records that threads share
/// ([`Decoder::shared_partition`], #4193).
pub type RangeDecode<'a, T, E> = Box<dyn Fn(Range<usize>) -> Result<T, E> + Sync + 'a>;

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
    /// The 1.0 closure a project file was admitted against and its document
    /// as written, against which a project file that names a `parent`
    /// composes its layers (PROJECT-16); `None` when this decoder composes
    /// no project layers.
    fn admitted_layers(_admitted: &Self::Admitted) -> Option<(&SchemaStructure, &Document)> {
        None
    }
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
    /// The schema of a captured input this decoder can read by partition
    /// (#3792), read from its header or footer alone: the fields
    /// [`Self::table`] gives the table when it finds nothing. `None` when
    /// the decoder reads the input only whole, or its header or footer does
    /// not read; such an input is read whole.
    fn partition_schema(
        &mut self,
        _source: &SourceDeclaration,
        _bytes: &[u8],
        _interpretation: SourceInterpretation,
    ) -> Option<TableSchema> {
        None
    }
    /// Scan one captured input by partition (#3792): decode each partition
    /// of about `rows` records once, in order, admitted on its own, and give
    /// it to `visit` with the position of its first record among the
    /// input's records before dropping it (#3830), or keeping it for
    /// [`Self::partition`] while the partitions kept hold no more than
    /// `rows` records together (#4212). Every finding
    /// [`Self::table`] reports is reported in the same order and with the
    /// same context; only the row and cell limits bound each partition
    /// instead of the whole table. The table returned holds no record: it
    /// has the input's schema and record count, and [`Self::partition`]
    /// decodes any range of its records again. `None` when the decoder reads
    /// the input only whole.
    #[allow(clippy::type_complexity)]
    fn partitioned(
        &mut self,
        _source: &SourceDeclaration,
        _bytes: &[u8],
        _interpretation: SourceInterpretation,
        _contract: Option<&Contract>,
        _rows: NonZeroUsize,
        _visit: &mut dyn FnMut(&Self::Table, usize),
    ) -> Option<Result<Self::Table, Self::Error>> {
        None
    }
    /// Decode records `range` of an input [`Self::partitioned`] scanned
    /// into `scanned`, from the same held bytes or the partitions its scan
    /// kept: a table of `range.len()` records, read by their positions in
    /// the range. The scan found every finding, so this finds none.
    fn partition(
        &mut self,
        _scanned: &Self::Table,
        source: &SourceDeclaration,
        _bytes: &[u8],
        _interpretation: SourceInterpretation,
        _contract: Option<&Contract>,
        _range: Range<usize>,
    ) -> Result<Self::Table, Self::Error> {
        unreachable!("{} was not scanned by partition", source.name)
    }
    /// Decode ranges of an input [`Self::partitioned`] scanned into
    /// `scanned` as [`Self::partition`] does, from any thread (#4193), so
    /// the partitions of a run decode on the threads that derive them.
    /// `None` when this decoder decodes partitions only through
    /// [`Self::partition`], on the calling thread.
    fn shared_partition<'a>(
        &self,
        _scanned: &'a Self::Table,
        _source: &'a SourceDeclaration,
        _bytes: &'a [u8],
        _interpretation: SourceInterpretation,
        _contract: Option<&'a Contract>,
    ) -> Option<RangeDecode<'a, Self::Table, Self::Error>> {
        None
    }
    /// Whether a failed input is an ingestion finding, after which the
    /// remaining declared inputs still decode. A resource, transport or
    /// opaque boundary failure ends the attempt.
    fn continue_after(&self, error: &Self::Error) -> bool;
}
