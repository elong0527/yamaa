//! Decode existing CSV/Parquet profiles while retaining a caller's opaque error.
//! Snapshot cells remain infallible; the error type is only the engine's shared
//! callback boundary. This grants no host access or activation authority.
use crate::{arrow_table::ArrowTable, specification_run::Error};
use std::marker::PhantomData;
use yamaa_core::specification_run::SourceDecoder;
use yamaa_core::{
    specification::SourceDeclaration,
    table::{CellError, TableAccess, TableSchema, ValueRef},
};

pub struct Snapshot<E> {
    table: ArrowTable,
    error: PhantomData<fn() -> E>,
}
impl<E> Snapshot<E> {
    pub fn table(&self) -> &ArrowTable {
        &self.table
    }
    pub fn into_table(self) -> ArrowTable {
        self.table
    }
}
impl<E> TableAccess for Snapshot<E> {
    type Error = E;
    fn schema(&self) -> &TableSchema {
        self.table.schema()
    }
    fn row_count(&self) -> usize {
        self.table.row_count()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<E>> {
        self.table.cell(row, column).map_err(|error| match error {
            CellError::OutOfBounds { row, column } => CellError::OutOfBounds { row, column },
            CellError::Access(never) => match never {},
        })
    }
}
pub struct Decoder<E>(PhantomData<fn() -> E>);
impl<E> Default for Decoder<E> {
    fn default() -> Self {
        Self(PhantomData)
    }
}
impl<E> SourceDecoder for Decoder<E> {
    type Error = Error;
    type Table = Snapshot<E>;
    fn continue_after(&self, error: &Error) -> bool {
        crate::specification_run::SourceDecoder.continue_after(error)
    }
    fn decode(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        interpretation: yamaa_core::specification::SourceInterpretation,
        contract: Option<&yamaa_core::producer_contract::Contract>,
    ) -> Result<Snapshot<E>, Error> {
        crate::specification_run::decode_source(source, bytes, interpretation, contract).map(
            |table| Snapshot {
                table,
                error: PhantomData,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "yaml")]
    use crate::specification_source::Source;
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
        }
    }

    #[test]
    fn native_csv_values_and_retry_after_malformed_input_use_the_same_decoder() {
        let mut decoder = Decoder::<std::convert::Infallible>::default();
        let table = decoder
            .decode(
                &csv_source(),
                b"A\nx\n",
                SourceInterpretation::Ordinary,
                None,
            )
            .unwrap();
        assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str("x"));
        assert!(decoder
            .decode(
                &csv_source(),
                b"A\n\"x",
                SourceInterpretation::Ordinary,
                None
            )
            .is_err());
        let repeated = decoder
            .decode(
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
            .decode(&source, b"A\nx\ny\n", SourceInterpretation::Ordinary, None)
            .unwrap();
        assert_eq!(table.row_count(), 2);
        assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str("x"));
        assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Str("y"));
        source.ordinal = Some("POSITION".into());
        let table = decoder
            .decode(&source, b"A\nx\n", SourceInterpretation::Ordinary, None)
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
            .decode(&source, &bytes, SourceInterpretation::Ordinary, None)
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
                .decode(&source, &bytes, SourceInterpretation::Odm, None)
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
                .decode(&source, bytes, SourceInterpretation::Ordinary, None)
                .unwrap();
            assert!(table.row_count() > 0);
            let repeated = decoder
                .decode(&source, bytes, SourceInterpretation::Ordinary, None)
                .unwrap();
            assert_eq!(repeated.row_count(), table.row_count());
        }
    }

    #[test]
    #[cfg(feature = "yaml")]
    fn generated_contract_csv_admits_100k_and_retains_exact_cell_and_ordinal_boundaries() {
        let document = crate::shipped_schema::capture()
            .unwrap()
            .prepare_standalone(Source {
                identity: "producer.yaml".into(),
                bytes: b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {RAW: raw.csv}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\n  - {name: VALUE, type: int, label: Value, derivation: RAW.VALUE}\noutput: {path: produced.csv, columns: [ID, VALUE]}\n".to_vec(),
            })
            .unwrap();
        let contract = yamaa_core::producer_contract::prepare(document.model()).unwrap();
        let mut source = SourceDeclaration {
            name: "P".into(),
            path: "produced.csv".into(),
            types: vec![
                ("ID".into(), ColumnType::Int),
                ("VALUE".into(), ColumnType::Int),
            ],
            profile: SourceProfile::Csv,
            encoding: None,
            empty_string_present: false,
            ordinal: None,
        };
        let mut bytes = String::from("ID,VALUE\n");
        for row in 1..=100_000 {
            use std::fmt::Write;
            writeln!(bytes, "{row},1").unwrap();
        }
        let table = Decoder::<std::convert::Infallible>::default()
            .decode(
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
            .decode(
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
