//! Facade-owned result tables and their Arrow form.
//!
//! [`Table`] is the facade form of the public table: its rows, its printed
//! preview, and its Arrow form, built once on first use: `columns()`,
//! `schema()`, `batches()` and `stream()`. Stream export follows the Arrow
//! host-handoff ownership rules: each export owns an independent cursor with
//! shared immutable buffer references. The Arrow type of each public column
//! type is committed: strings are `Utf8`, integers `Int64`, floats
//! `Float64`, dates `Date32` (days) and zone-free datetimes
//! `TimestampSecond`. No specification column ever surfaces as an Arrow
//! `Boolean`.
//!
//! Host frames cross into the engine only as opaque [`InputFrame`] handles;
//! hosts never decode engine tables except through [`Table`].
//!
//! [`Engine::save_table`] saves a table to a file its caller names, under
//! the profiles of a run's output.

use std::sync::Arc;

use crate::engine::Engine;
use crate::error::Failure;
#[cfg(feature = "arrow")]
use crate::error::TableError;
use crate::text::Action;

#[cfg(feature = "arrow")]
pub use arrow_array::ffi_stream::FFI_ArrowArrayStream;
#[cfg(feature = "arrow")]
pub use arrow_array::{
    Array, Date32Array, Float64Array, Int64Array, LargeStringArray, ListArray, RecordBatch,
    StringArray, StringViewArray, TimestampSecondArray,
};
#[cfg(feature = "arrow")]
pub use arrow_schema::{DataType, Field, Fields, SchemaRef, TimeUnit};

/// An owned immutable result table, cheap to clone: clones share its columns
/// and its Arrow form.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Table {
    inner: yamaa_adapters::public_table::PublicTable,
}

impl Table {
    pub(crate) fn from_adapter(table: &yamaa_adapters::public_table::PublicTable) -> Self {
        Self {
            inner: table.clone(),
        }
    }

    pub(crate) fn from_public(table: yamaa_adapters::public_table::PublicTable) -> Self {
        Self { inner: table }
    }

    pub(crate) fn public(&self) -> &yamaa_adapters::public_table::PublicTable {
        &self.inner
    }

    /// Restore a public result table from its Arrow C stream, including list
    /// columns. The stream must satisfy Arrow's producer contract and contain
    /// only the public result types: Utf8, Int64, Float64, Date32, zone-free
    /// second timestamps and lists of these. Unlike `InputFrame::import`,
    /// this does not interpret the values as specification input. Nonfinite
    /// floats become missing; dates and datetimes outside years 1 to 9999
    /// are refused.
    #[cfg(feature = "arrow")]
    pub fn import(stream: FFI_ArrowArrayStream) -> Result<Self, TableError> {
        yamaa_adapters::arrow_bridge::import_result_stream(stream)
            .map(Self::from_public)
            .map_err(TableError::from_public)
    }

    /// Borrow the public Arrow fields in declaration order.
    #[cfg(feature = "arrow")]
    pub fn columns(&self) -> &Fields {
        self.inner.columns()
    }

    /// Borrow the public Arrow schema.
    #[cfg(feature = "arrow")]
    pub fn schema(&self) -> &SchemaRef {
        self.inner.schema()
    }

    /// Borrow the owned record batches in record order.
    #[cfg(feature = "arrow")]
    pub fn batches(&self) -> &[RecordBatch] {
        self.inner.batches()
    }

    /// Count the logical rows across all batches.
    pub fn rows(&self) -> usize {
        self.inner.rows()
    }

    /// Export one stream with an independent cursor and shared buffers.
    #[cfg(feature = "arrow")]
    pub fn stream(&self) -> FFI_ArrowArrayStream {
        self.inner.stream()
    }

    /// The table's printed preview (D43, P-PRINT-1, #3030 D167), the same
    /// text in every host: what Python's `repr()` of a `yamaa.Table` returns.
    ///
    /// The first line is `yamaa.Table: <r> rows x <c> columns`, with the
    /// singular noun for 1. A table with columns adds a row of column names,
    /// a row of type names (`str`, `int`, `float`, `date`, `datetime` or
    /// `list<T>`) and its value rows: every row of a table of at most 10,
    /// otherwise the first 5, a `...` line and the last 5. Only the first 8
    /// columns print; a table with more ends each of those rows with a `...`
    /// column.
    ///
    /// A missing value prints `null`, a float its canonical text (VALUE-8), a
    /// date or datetime its canonical text (VALUE-17), and a list its items
    /// joined by `, ` in brackets. Every cell, names and types included,
    /// prints in ASCII with the escapes of [`crate::text`]; one that would
    /// print more than 20 characters prints its first 17 and `...`. Each
    /// column is as wide as its widest cell, cells are padded with spaces on
    /// the right, columns are two spaces apart, and no line ends in a space.
    /// The lines are joined with LF, with no final LF.
    ///
    /// The header freezes at 1.0.0; the rows below it may improve in any
    /// release.
    pub fn preview(&self) -> String {
        crate::text::preview(self)
    }
}

impl Engine {
    /// Save `table` to the file `out` names and return the one path written,
    /// as the command line's `yamaa read-odm SOURCE OUT` saves the item table
    /// [`Engine::read_odm`] returns.
    ///
    /// The case-insensitive extension of `out` selects the `csv` or
    /// `parquet` profile from OUTPUT-5's closed mapping, and the encoders of
    /// a run's output write the table, so the file holds the bytes a run
    /// saves for the same table. Any other extension, or none, fails as
    /// `unknown_artifact_profile` (OUTPUT-5) before anything is written.
    ///
    /// The file is saved through the engine's resolver by the publisher
    /// every `save()` uses, as a save of one file (OUTPUT-8, #3030 D99): its
    /// target is opened, the file is staged beside it, then committed.
    /// Without a resolver, `out` is a local path: its parent directory is
    /// selected once, and its last segment is the caller's own file name
    /// (FILE-7, FILE-8). The path returned is absolute, with forward
    /// slashes, or the reference a caller's resolver gave its target (D38).
    ///
    /// A target that cannot be written fails as a run's does. One in a
    /// directory that does not exist, a directory or a link fails with
    /// FILE-7's or FILE-8's condition under `output`, at no field, naming
    /// `out` as given (#3030 D189); any other fails as `engine_rejected` in
    /// the `output` step with the save's code, such as `invalid_target`. A
    /// table with a column of a type neither profile writes, such as the list
    /// columns of an issues table, fails as `engine_rejected` with the code
    /// `output_schema`, naming the column. A refusal names the call as
    /// `cannot save <out>` (D39) and writes nothing (#3030 D134).
    ///
    /// # Example
    ///
    /// The item table of an ODM file a memory resolver serves, saved back
    /// through that resolver. Lanes without the `odm` feature cannot read it.
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use yamaa::{Engine, EngineOptions, FailureKind, MemoryResolver, OdmOptions};
    ///
    /// let resolver = Arc::new(MemoryResolver::new());
    /// resolver.insert("/", "odm.xml", "odm:1", Arc::from(
    ///     &br#"<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3">
    ///   <ClinicalData StudyOID="S" MetaDataVersionOID="M">
    ///     <SubjectData SubjectKey="01"><StudyEventData StudyEventOID="E">
    ///       <FormData FormOID="F"><ItemGroupData ItemGroupOID="G">
    ///         <ItemData ItemOID="AGE" Value="42"/>
    ///       </ItemGroupData></FormData>
    ///     </StudyEventData></SubjectData>
    ///   </ClinicalData>
    /// </ODM>"#[..],
    /// ));
    /// let engine = Engine::new(
    ///     EngineOptions::neutral("doctest".into(), "0".into()).with_resolver(resolver.clone()),
    /// );
    /// match engine.read_odm("odm.xml", &OdmOptions::new()) {
    ///     Ok(table) => {
    ///         let paths = engine.save_table(&table, "items.csv").unwrap();
    ///         assert_eq!(paths, ["memory:/items.csv"]);
    ///         let saved = resolver.published("memory:/items.csv").unwrap();
    ///         assert!(saved.starts_with(b"ODMVersion,FileOID,StudyOID,"));
    ///         let refused = engine.save_table(&table, "items.txt").unwrap_err();
    ///         assert_eq!(
    ///             refused.message,
    ///             "cannot save items.txt: the extension of items.txt is not one of \
    ///              .csv, .parquet [unknown_artifact_profile, OUTPUT-5]"
    ///         );
    ///         assert!(resolver.published("memory:/items.txt").is_none());
    ///     }
    ///     Err(failure) => {
    ///         assert_eq!(failure.kind, FailureKind::Unavailable { feature: "odm" });
    ///     }
    /// }
    /// ```
    pub fn save_table(&self, table: &Table, out: &str) -> Result<Vec<String>, Failure> {
        yamaa_application::table_artifact::save(
            table.public(),
            out,
            crate::internal::resolver_bridge(self.shared()),
        )
        .map_err(|issues| Failure::from_issues(Action::Save(out), &issues))
    }
}

/// An opaque host frame admitted into the engine. Frames originate with the
/// host (an Arrow C stream the host owns) and are consumed by the engine on
/// capture; they carry no readable table surface.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct InputFrame {
    inner: Arc<yamaa_adapters::column_table::ColumnTable>,
}

impl InputFrame {
    /// Consume a standard host stream into the engine's own columns, copying
    /// its values: UTF-8 (including large and view arrays), `Int64`, `Float64`,
    /// `Date32` and zone-free `TimestampSecond`. Other physical types require
    /// an explicit preserving conversion by the caller before exporting.
    #[cfg(feature = "arrow")]
    pub fn import(stream: FFI_ArrowArrayStream) -> Result<Self, TableError> {
        let table = yamaa_adapters::arrow_bridge::import_public_stream(stream)
            .map_err(TableError::from_table)?;
        Ok(Self {
            inner: Arc::new(table),
        })
    }

    pub(crate) fn into_adapter(self) -> Arc<yamaa_adapters::column_table::ColumnTable> {
        self.inner
    }

    pub(crate) fn from_adapter_table(
        table: Arc<yamaa_adapters::column_table::ColumnTable>,
    ) -> Self {
        Self { inner: table }
    }
}

#[cfg(all(test, feature = "arrow"))]
mod tests {
    use super::*;

    #[test]
    fn empty_streams_fail_with_a_stable_table_code() {
        let error = InputFrame::import(FFI_ArrowArrayStream::empty()).unwrap_err();
        assert!(!error.code().is_empty());
        assert!(!error.message().is_empty());
    }
}
