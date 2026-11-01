//! The decoder port's one implementation. A parent layer or a dictionary
//! file decodes with the shared YAML reader, keeping the position of each
//! node; a file a project names decodes and admits against the 1.0 closure
//! of its type; and an input's bytes decode by its profile (CSV, Parquet,
//! SAS, ODM or Dataset-JSON) into a table that keeps a caller's opaque error. A CSV or Parquet
//! input also decodes by partition (#3792), into a snapshot of its
//! partitions' tables. Snapshot cells remain infallible; the error type is
//! only the engine's shared callback boundary. This grants no host access or
//! activation authority.
use crate::source_decoding::{Kept, Scanned};
use crate::{
    column_table::ColumnTable,
    document::Closures,
    source_decoding::Error,
    specification_source::{self, CapturedDocument, Source},
    yaml_decode::{decode_yaml, DecodedYaml},
};
use std::{marker::PhantomData, sync::Arc};
use yamaa_core::{
    project_environment::LockKind,
    project_preparation::DecodeTarget,
    specification::SourceDeclaration,
    table::{CellError, ColumnRef, TableAccess, TableSchema, ValueRef},
};

/// One decoded input: a table, or an input read by partition (#3792): its
/// scan's schema and record count, what decodes any range of its records
/// again from the held bytes (#3830), and the partitions the scan kept for
/// the run (#4212). The snapshot alone holds them, so the report and its
/// observation of the input never do. A run that fails after the scan,
/// before its partitions take them, keeps them with its attempt while the
/// report is built: about two partitions at the most.
pub struct Snapshot<E> {
    storage: Storage<E>,
}
enum Storage<E> {
    Whole(ColumnTable, PhantomData<fn() -> E>),
    Scanned(Arc<Scanned>, Kept, PhantomData<fn() -> E>),
}
impl<E> Snapshot<E> {
    fn whole(table: ColumnTable) -> Self {
        Self {
            storage: Storage::Whole(table, PhantomData),
        }
    }
    fn scanned((scanned, kept): (Scanned, Kept)) -> Self {
        Self {
            storage: Storage::Scanned(Arc::new(scanned), kept, PhantomData),
        }
    }
    /// The table of an input read whole; `None` for one read by partition.
    pub fn into_table(self) -> Option<ColumnTable> {
        match self.storage {
            Storage::Whole(table, _) => Some(table),
            Storage::Scanned(..) => None,
        }
    }
    /// The table of an input read whole, shared without a copy by a clone;
    /// `None` for one read by partition.
    pub fn as_table(&self) -> Option<&ColumnTable> {
        match &self.storage {
            Storage::Whole(table, _) => Some(table),
            Storage::Scanned(..) => None,
        }
    }
    /// What the scan of an input read by partition keeps (#3830); `None`
    /// for one read whole.
    pub fn scan(&self) -> Option<&Arc<Scanned>> {
        match &self.storage {
            Storage::Whole(..) => None,
            Storage::Scanned(scanned, ..) => Some(scanned),
        }
    }
    /// Decode records `range` of an input read by partition for a run's
    /// partition: from the partitions its scan kept, and the rest from
    /// `bytes`, the held bytes the scan read (#4212).
    fn partition(
        &self,
        name: &str,
        bytes: &[u8],
        range: std::ops::Range<usize>,
    ) -> Result<Self, Error> {
        match &self.storage {
            Storage::Scanned(scanned, kept, _) => {
                crate::source_decoding::decode_partition(scanned, kept, bytes, range)
                    .map(Snapshot::whole)
            }
            Storage::Whole(..) => unreachable!("{name} was not scanned by partition"),
        }
    }
}
impl<E> TableAccess for Snapshot<E> {
    type Error = E;
    fn schema(&self) -> &TableSchema {
        match &self.storage {
            Storage::Whole(table, _) => table.schema(),
            Storage::Scanned(scanned, ..) => &scanned.schema,
        }
    }
    fn row_count(&self) -> usize {
        match &self.storage {
            Storage::Whole(table, _) => table.row_count(),
            Storage::Scanned(scanned, ..) => scanned.records,
        }
    }
    /// A cell of an input read whole. An input read by partition holds no
    /// record: a run reads each range of its records decoded again.
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<E>> {
        match &self.storage {
            Storage::Whole(table, _) => table.cell(row, column).map_err(|error| match error {
                CellError::OutOfBounds { row, column } => CellError::OutOfBounds { row, column },
                CellError::Access(never) => match never {},
            }),
            Storage::Scanned(..) => Err(CellError::OutOfBounds { row, column }),
        }
    }
    /// A column of an input read whole, lent as the table holds it.
    fn column(&self, column: usize) -> Option<ColumnRef<'_>> {
        match &self.storage {
            Storage::Whole(table, _) => TableAccess::column(table, column),
            Storage::Scanned(..) => None,
        }
    }
    /// Every snapshot may be shared and moved between the threads of a
    /// partitioned run (#4193): it holds its values, and no error.
    fn shared(&self) -> Option<&(dyn TableAccess<Error = E> + Sync)> {
        Some(self)
    }
    fn into_shared<'t>(self) -> Result<Box<dyn TableAccess<Error = E> + Send + Sync + 't>, Self>
    where
        Self: 't,
    {
        Ok(Box::new(self))
    }
}
pub struct Decoder<E> {
    /// The closures a project's files admit against, or none until a project
    /// file is first admitted against the shipped closures.
    closures: Option<Closures>,
    error: PhantomData<fn() -> E>,
}
impl<E> Default for Decoder<E> {
    fn default() -> Self {
        Self {
            closures: None,
            error: PhantomData,
        }
    }
}
impl<E> Decoder<E> {
    /// A decoder that admits a project's files against `closures`.
    pub fn with_closures(closures: Closures) -> Self {
        Self {
            closures: Some(closures),
            error: PhantomData,
        }
    }
}
impl<E> yamaa_core::decoder::Decoder for Decoder<E> {
    type Raw = DecodedYaml;
    type Admitted = CapturedDocument;
    type DocumentError = specification_source::Error;
    type Error = Error;
    type Table = Snapshot<E>;
    fn raw(&mut self, identity: &str, bytes: &[u8]) -> Result<DecodedYaml, Self::DocumentError> {
        decode_yaml(bytes).map_err(|error| specification_source::Error::Decode {
            identity: identity.into(),
            error,
        })
    }
    fn raw_document(raw: &DecodedYaml) -> &yamaa_core::schema::Document {
        &raw.document
    }
    fn admitted(
        &mut self,
        source: Source,
        target: DecodeTarget,
    ) -> Result<CapturedDocument, Self::DocumentError> {
        let closures = match &mut self.closures {
            Some(closures) => closures,
            empty => empty.insert(Closures::shipped()?),
        };
        crate::project_source::admit(closures, source, target)
    }
    fn admitted_document(admitted: &CapturedDocument) -> &yamaa_core::schema::Document {
        &admitted.normalized().document
    }
    fn admitted_source(admitted: &CapturedDocument) -> &Source {
        admitted.source()
    }
    fn admitted_layers(
        admitted: &CapturedDocument,
    ) -> Option<(
        &yamaa_core::schema::SchemaStructure,
        &yamaa_core::schema::Document,
    )> {
        Some((admitted.schema().structure(), &admitted.raw().document))
    }
    /// The packaging tool whose lock syntax the held bytes parse as.
    fn lock(&mut self, source: &Source) -> Option<LockKind> {
        crate::project_lock::kind(&source.bytes).ok()
    }
    fn served_table(
        &mut self,
        source: &SourceDeclaration,
        table: &(dyn std::any::Any + Send + Sync),
        interpretation: yamaa_core::specification::SourceInterpretation,
        contract: Option<&yamaa_core::producer_contract::Contract>,
    ) -> Option<Result<Self::Table, Error>> {
        table.downcast_ref::<ColumnTable>().map(|table| {
            crate::source_decoding::decode_frame(source, table.clone(), interpretation, contract)
                .map(Snapshot::whole)
        })
    }
    /// A profile, typing, ordinal, metadata or document finding in one input
    /// lets the remaining inputs decode; any other failure ends the attempt.
    fn continue_after(&self, error: &Error) -> bool {
        use crate::{csv_source, dataset_json_source, parquet_source, sas_source};
        use yamaa_core::{source_ordinal, typed_csv};
        matches!(
            error,
            Error::Source(csv_source::TextTableError::Csv(
                csv_source::Error::Profile { .. }
            )) | Error::Ordinal(source_ordinal::Error::Collision)
                | Error::TypedSource(crate::typed_csv::Error::Typing(
                    typed_csv::Error::UnknownField { .. } | typed_csv::Error::FieldParse { .. }
                ))
                | Error::ParquetSource(
                    parquet_source::Error::Malformed
                        | parquet_source::Error::EmptyName { .. }
                        | parquet_source::Error::DuplicateName { .. }
                        | parquet_source::Error::Unsupported { .. }
                        | parquet_source::Error::Value { .. }
                )
                | Error::ProducerMetadata(_)
                | Error::OdmFields(_)
                | Error::SasSource(
                    sas_source::Error::Invalid(_)
                        | sas_source::Error::EncodingMismatch { .. }
                        | sas_source::Error::Field { .. }
                        | sas_source::Error::Typing(_)
                )
                | Error::DatasetJsonSource(
                    dataset_json_source::Error::Invalid(_)
                        | dataset_json_source::Error::Value { .. }
                        | dataset_json_source::Error::UnknownField { .. }
                )
                | Error::OdmValues(_)
        ) || matches!(error, Error::OdmSource(error) if error.refuses_document())
    }
    fn table(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        interpretation: yamaa_core::specification::SourceInterpretation,
        contract: Option<&yamaa_core::producer_contract::Contract>,
    ) -> Result<Snapshot<E>, Error> {
        crate::source_decoding::decode_source(source, bytes, interpretation, contract)
            .map(Snapshot::whole)
    }
    /// A CSV input's header or a Parquet input's footer gives the schema;
    /// every other profile is read whole.
    fn partition_schema(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        interpretation: yamaa_core::specification::SourceInterpretation,
    ) -> Option<TableSchema> {
        crate::source_decoding::source_schema(source, bytes, interpretation)
    }
    /// A CSV or Parquet input scans a partition at a time, each table
    /// visited (#3830), a Parquet input's decoding up to `threads` at once
    /// (#4220), and the first Parquet partitions kept for the run within
    /// the limits of one partition (#4212); every other profile is read
    /// whole.
    fn partitioned(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        interpretation: yamaa_core::specification::SourceInterpretation,
        contract: Option<&yamaa_core::producer_contract::Contract>,
        (rows, threads): (std::num::NonZeroUsize, std::num::NonZeroUsize),
        visit: &mut dyn FnMut(&Snapshot<E>, usize),
    ) -> Option<Result<Snapshot<E>, Error>> {
        let scanned = crate::source_decoding::scan_partitions(
            source,
            bytes,
            interpretation,
            contract,
            (rows, threads),
            &mut |table, first| visit(&Snapshot::whole(table.clone()), first),
        )?;
        Some(scanned.map(Snapshot::scanned))
    }
    fn partition(
        &mut self,
        scanned: &Snapshot<E>,
        source: &SourceDeclaration,
        bytes: &[u8],
        _: yamaa_core::specification::SourceInterpretation,
        _: Option<&yamaa_core::producer_contract::Contract>,
        range: std::ops::Range<usize>,
    ) -> Result<Snapshot<E>, Error> {
        scanned.partition(&source.name, bytes, range)
    }
    /// A range decodes from the scan, its kept partitions and the held
    /// bytes alone, so any thread may decode it (#4193, #4212).
    fn shared_partition<'a>(
        &self,
        scanned: &'a Snapshot<E>,
        source: &'a SourceDeclaration,
        bytes: &'a [u8],
        _: yamaa_core::specification::SourceInterpretation,
        _: Option<&'a yamaa_core::producer_contract::Contract>,
    ) -> Option<yamaa_core::decoder::RangeDecode<'a, Snapshot<E>, Error>> {
        Some(Box::new(move |range| {
            scanned.partition(&source.name, bytes, range)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "yaml")]
    use crate::specification_source::Source;
    use yamaa_core::decoder::Decoder as _;
    use yamaa_core::specification::SourceInterpretation;
    use yamaa_core::specification::SourceProfile;
    #[cfg(feature = "yaml")]
    use yamaa_core::value::ColumnType;

    fn csv_source() -> SourceDeclaration {
        SourceDeclaration {
            name: "RAW".into(),
            path: "raw.csv".into(),
            types: vec![],
            profile: SourceProfile::Csv,
            empty_string_present: false,
            ordinal: None,
            encoding: None,
            byte_order_mark: false,
        }
    }

    #[test]
    fn native_csv_values_and_retry_after_malformed_input_use_the_same_decoder() {
        let mut decoder = Decoder::<std::convert::Infallible>::default();
        let table = decoder
            .table(
                &csv_source(),
                b"A\nx\n",
                SourceInterpretation::Ordinary,
                None,
            )
            .unwrap();
        assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str("x"));
        assert!(decoder
            .table(
                &csv_source(),
                b"A\n\"x",
                SourceInterpretation::Ordinary,
                None
            )
            .is_err());
        let repeated = decoder
            .table(
                &csv_source(),
                b"A\nx\n",
                SourceInterpretation::Ordinary,
                None,
            )
            .unwrap();
        assert_eq!(repeated.cell(0, 0).unwrap(), ValueRef::Str("x"));
    }

    #[test]
    fn native_csv_records_and_generated_ordinal_preserve_source_order() {
        let mut decoder = Decoder::<std::convert::Infallible>::default();
        let mut source = csv_source();
        let table = decoder
            .table(&source, b"A\nx\ny\n", SourceInterpretation::Ordinary, None)
            .unwrap();
        assert_eq!(table.row_count(), 2);
        assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str("x"));
        assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Str("y"));
        source.ordinal = Some("POSITION".into());
        let table = decoder
            .table(&source, b"A\nx\n", SourceInterpretation::Ordinary, None)
            .unwrap();
        assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Int(1));
    }

    #[test]
    #[cfg(feature = "parquet")]
    fn native_parquet_retains_all_sixty_four_records() {
        use arrow_array::{Int64Array, RecordBatch};
        use arrow_schema::{DataType, Field, Schema};
        use parquet::{arrow::ArrowWriter, file::properties::WriterProperties};
        use std::sync::Arc;
        let schema = Arc::new(Schema::new(vec![Field::new("ID", DataType::Int64, false)]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(Int64Array::from_iter_values(1..=64))],
        )
        .unwrap();
        let mut bytes = Vec::new();
        let properties = WriterProperties::builder()
            .set_dictionary_enabled(false)
            .build();
        let mut writer = ArrowWriter::try_new(&mut bytes, schema, Some(properties)).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();
        assert!(bytes.len() < 2_048);
        let mut source = csv_source();
        source.profile = SourceProfile::Parquet;
        source.path = "raw.parquet".into();
        let mut decoder = Decoder::<std::convert::Infallible>::default();
        let table = decoder
            .table(&source, &bytes, SourceInterpretation::Ordinary, None)
            .unwrap();
        assert_eq!(table.row_count(), 64);
        assert_eq!(table.cell(63, 0).unwrap(), ValueRef::Int(64));
    }

    #[test]
    #[cfg(feature = "parquet")]
    fn both_supported_odm_routes_preserve_all_eleven_fields() {
        use arrow_array::{ArrayRef, RecordBatch, StringArray};
        use arrow_schema::{DataType, Field, Schema};
        use parquet::arrow::ArrowWriter;
        use std::sync::Arc;
        use yamaa_core::{odm_source::FIELDS, specification::SourceInterpretation};
        let csv = format!("{}\n{}\n", FIELDS.join(","), ["a"; 11].join(",")).into_bytes();
        let schema = Arc::new(Schema::new(
            FIELDS
                .iter()
                .map(|name| Field::new(*name, DataType::Utf8, false))
                .collect::<Vec<_>>(),
        ));
        let arrays = FIELDS
            .iter()
            .map(|_| Arc::new(StringArray::from(vec!["a"])) as ArrayRef)
            .collect();
        let batch = RecordBatch::try_new(schema.clone(), arrays).unwrap();
        let mut parquet = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut parquet, schema, None).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();
        for (profile, bytes) in [(SourceProfile::Csv, csv), (SourceProfile::Parquet, parquet)] {
            let mut source = csv_source();
            source.profile = profile;
            let mut decoder = Decoder::<std::convert::Infallible>::default();
            let table = decoder
                .table(&source, &bytes, SourceInterpretation::Odm, None)
                .unwrap();
            assert_eq!(table.row_count(), 1);
            assert_eq!(table.schema().columns().len(), 11);
            for column in 0..11 {
                assert_eq!(table.cell(0, column).unwrap(), ValueRef::Str("a"));
            }
        }
    }

    #[cfg(feature = "readstat")]
    #[test]
    fn both_sas_source_profiles_preserve_rows_across_repeated_decoding() {
        for (profile, bytes) in [
            (
                SourceProfile::Xport,
                include_bytes!("../tests/fixtures/sas/fractional.xpt").as_slice(),
            ),
            (
                SourceProfile::Sas7bdat,
                include_bytes!("../tests/fixtures/sas/datetime.sas7bdat").as_slice(),
            ),
        ] {
            let mut source = csv_source();
            source.profile = profile;
            source.encoding = Some("windows-1252".into());
            let mut decoder = Decoder::<std::convert::Infallible>::default();
            let table = decoder
                .table(&source, bytes, SourceInterpretation::Ordinary, None)
                .unwrap();
            assert!(table.row_count() > 0);
            let repeated = decoder
                .table(&source, bytes, SourceInterpretation::Ordinary, None)
                .unwrap();
            assert_eq!(repeated.row_count(), table.row_count());
        }
    }

    #[test]
    #[cfg(feature = "yaml")]
    fn generated_contract_csv_admits_100k_and_retains_exact_cell_and_ordinal_boundaries() {
        let document = crate::shipped_schema::prepare(
            Source {
                identity: "producer.yaml".into(),
                bytes: b"yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {RAW: raw.csv}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\n  - {name: VALUE, type: int, label: Value, derivation: RAW.VALUE}\noutput: {path: produced.csv, columns: [ID, VALUE]}\n".to_vec(),
            },
            "producer.yaml".into(),
            &mut crate::specification_source::EntryOnly,
        )
        .unwrap();
        let contract =
            yamaa_core::producer_contract::prepare_document(document.document()).unwrap();
        let mut source = SourceDeclaration {
            name: "P".into(),
            path: "produced.csv".into(),
            types: vec![
                ("ID".into(), ColumnType::Int),
                ("VALUE".into(), ColumnType::Int),
            ],
            profile: SourceProfile::Csv,
            encoding: None,
            byte_order_mark: false,
            empty_string_present: false,
            ordinal: None,
        };
        let mut bytes = String::from("ID,VALUE\n");
        for row in 1..=100_000 {
            use std::fmt::Write;
            writeln!(bytes, "{row},1").unwrap();
        }
        let table = Decoder::<std::convert::Infallible>::default()
            .table(
                &source,
                bytes.as_bytes(),
                SourceInterpretation::Ordinary,
                Some(&contract),
            )
            .unwrap();
        assert_eq!(table.row_count(), 100_000);
        for row in 0..100_000 {
            assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Int(row as i64 + 1));
            assert_eq!(table.cell(row, 1).unwrap(), ValueRef::Int(1));
        }
        source.ordinal = Some("POSITION".into());
        let table = Decoder::<std::convert::Infallible>::default()
            .table(
                &source,
                bytes.as_bytes(),
                SourceInterpretation::Ordinary,
                Some(&contract),
            )
            .unwrap();
        assert_eq!(table.schema().columns()[2].name, "POSITION");
        assert_eq!(table.cell(99_999, 2).unwrap(), ValueRef::Int(100_000));
    }
}
