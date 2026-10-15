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
pub struct Decoder<E> {
    error: PhantomData<fn() -> E>,
    usage: crate::source_decode_usage::Usage,
}
impl<E> Default for Decoder<E> {
    fn default() -> Self {
        Self {
            error: PhantomData,
            usage: Default::default(),
        }
    }
}
impl<E> SourceDecoder for Decoder<E> {
    type Error = Error;
    type Table = Snapshot<E>;
    fn continue_after(&self, error: &Error) -> bool {
        crate::specification_run::SourceDecoder.continue_after(error)
    }
    fn decode(&mut self, source: &SourceDeclaration, bytes: &[u8]) -> Result<Snapshot<E>, Error> {
        self.usage = Default::default();
        crate::specification_run::SourceDecoder
            .decode(source, bytes)
            .map(|snapshot| Snapshot {
                table: snapshot.0,
                error: PhantomData,
            })
    }
    fn decode_with_interpretation(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        interpretation: yamaa_core::specification::SourceInterpretation,
    ) -> Result<Snapshot<E>, Error> {
        self.usage = Default::default();
        crate::specification_run::decode_interpreted(source, bytes, None, interpretation).map(
            |table| Snapshot {
                table,
                error: PhantomData,
            },
        )
    }
}

impl<E> yamaa_core::producer_build::DecodePort for Decoder<E> {
    fn usage(&self) -> Option<yamaa_core::producer_build::DecodeUsage> {
        self.usage
            .complete
            .then_some(yamaa_core::producer_build::DecodeUsage {
                storage_bytes: self.usage.storage_bytes,
                work_bytes: self.usage.work_bytes,
            })
    }
    fn decode_bounded_interpreted(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        contract: Option<&yamaa_core::producer_contract::Contract>,
        limits: yamaa_core::producer_build::DecodeLimits,
        interpretation: yamaa_core::specification::SourceInterpretation,
    ) -> Result<Snapshot<E>, yamaa_core::producer_build::DecodeError<Error>> {
        if interpretation == yamaa_core::specification::SourceInterpretation::Ordinary {
            return self.decode_bounded(source, bytes, contract, limits);
        }
        self.usage = Default::default();
        crate::specification_run::decode_interpreted_with_usage(
            source,
            bytes,
            limits,
            interpretation,
            &mut self.usage,
        )
        .map(|table| Snapshot {
            table,
            error: PhantomData,
        })
        .map_err(yamaa_core::producer_build::DecodeError::Codec)
    }
    fn decode_bounded(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        contract: Option<&yamaa_core::producer_contract::Contract>,
        limits: yamaa_core::producer_build::DecodeLimits,
    ) -> Result<Snapshot<E>, yamaa_core::producer_build::DecodeError<Error>> {
        use yamaa_core::producer_build::DecodeError;
        self.usage = Default::default();
        self.usage.complete();
        let table = if let Some(contract) =
            contract.filter(|_| source.profile == yamaa_core::specification::SourceProfile::Csv)
        {
            // Admit stored header order before declared typing can replace a
            // producer mismatch with an unrelated missing-field diagnostic.
            let types = yamaa_core::typed_csv::PreparedTypes::new(
                &source.types,
                yamaa_core::specification::DEFAULT_MAX_COLUMNS,
            )
            .map_err(|e| {
                DecodeError::Codec(Error::TypedSource(crate::typed_csv::Error::Typing(e)))
            })?;
            let parsed = crate::source_decode_usage::parse_csv(
                bytes,
                crate::specification_run::bounded_csv_limits(limits),
                limits,
                &mut self.usage,
            )
            .map_err(DecodeError::Codec)?;
            let contract_limits = yamaa_core::producer_contract::Limits::default();
            if parsed.names.len() > contract_limits.fields {
                return Err(DecodeError::Limit("producer_source_fields"));
            }
            let header = parsed.names.iter().map(String::as_str).collect::<Vec<_>>();
            if let Some(diagnostic) = yamaa_core::source_header::validate_header(
                contract,
                &source.name,
                &header,
                contract_limits,
            )
            .map_err(DecodeError::Contract)?
            {
                return Err(DecodeError::Metadata(diagnostic));
            }
            crate::source_decode_usage::csv_conversion(
                &parsed,
                limits.storage_bytes,
                &mut self.usage,
            )
            .map_err(|error| {
                DecodeError::Codec(Error::Source(crate::csv_source::TextTableError::Table(
                    error,
                )))
            })?;
            let table = crate::typed_csv::convert(
                parsed,
                types,
                crate::arrow_table::TableLimits {
                    max_rows: limits.rows.min(yamaa_core::capacity::SOURCE_ROWS),
                    max_columns: yamaa_core::specification::DEFAULT_MAX_COLUMNS,
                    max_batches: 1,
                    max_cells: limits.cells.min(yamaa_core::capacity::SOURCE_CELLS),
                },
            )
            .map_err(|e| DecodeError::Codec(crate::specification_run::typed_source_error(e)))?;
            crate::source_ordinal::append_with_usage(
                source,
                table,
                crate::arrow_table::TableLimits {
                    max_rows: limits.rows.min(yamaa_core::capacity::SOURCE_ROWS),
                    max_columns: yamaa_core::specification::DEFAULT_MAX_COLUMNS,
                    max_batches: 1,
                    max_cells: limits.cells.min(yamaa_core::capacity::SOURCE_CELLS),
                },
                limits.storage_bytes,
                &mut self.usage,
            )
            .map_err(DecodeError::Codec)?
        } else {
            crate::specification_run::decode_interpreted_with_usage(
                source,
                bytes,
                limits,
                yamaa_core::specification::SourceInterpretation::Ordinary,
                &mut self.usage,
            )
            .map_err(DecodeError::Codec)?
        };
        Ok(Snapshot {
            table,
            error: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{csv_source::TextTableError, specification_source::Source};
    use yamaa_core::producer_build::{DecodeError, DecodeLimits, DecodePort};
    use yamaa_core::{specification::SourceProfile, value::ColumnType};

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
    fn native_csv_receipt_is_admitted_charge_and_resets_after_every_attempt() {
        let mut decoder = Decoder::<std::convert::Infallible>::default();
        let limits = DecodeLimits {
            rows: 1,
            cells: 1,
            storage_bytes: 179,
            work_bytes: 4,
        };
        let table = decoder
            .decode_bounded(&csv_source(), b"A\nx\n", None, limits)
            .unwrap();
        assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str("x"));
        let usage = decoder.usage().unwrap();
        assert_eq!((usage.storage_bytes, usage.work_bytes), (179, 4));
        assert!(matches!(
            decoder.decode_bounded(
                &csv_source(),
                b"A\nx\n",
                None,
                DecodeLimits {
                    storage_bytes: 178,
                    ..limits
                }
            ),
            Err(DecodeError::Codec(Error::Source(TextTableError::Table(
                crate::arrow_table::TableError::Limit {
                    resource: "storage_bytes",
                    limit: 178,
                    required: Some(179),
                }
            ))))
        ));
        assert_eq!(decoder.usage().unwrap().storage_bytes, 178);
        assert!(decoder
            .decode_bounded(&csv_source(), b"A\n\"x", None, limits)
            .is_err());
        assert_eq!(decoder.usage().unwrap().storage_bytes, 113);
        assert_eq!(decoder.usage().unwrap().work_bytes, 4);
        assert!(decoder
            .decode_bounded(
                &csv_source(),
                b"A\nx\n",
                None,
                DecodeLimits {
                    work_bytes: 3,
                    ..limits
                }
            )
            .is_err());
        assert_eq!(decoder.usage().unwrap().work_bytes, 0);
        assert_eq!(decoder.usage().unwrap().storage_bytes, 4);
        decoder.decode(&csv_source(), b"A\nx\n").unwrap();
        assert!(decoder.usage().is_none());
        decoder
            .decode_bounded(&csv_source(), b"A\nx\n", None, limits)
            .unwrap();
        assert!(decoder
            .decode_with_interpretation(
                &csv_source(),
                b"A\n\"x",
                yamaa_core::specification::SourceInterpretation::Ordinary
            )
            .is_err());
        assert!(decoder.usage().is_none());
    }

    #[test]
    fn native_csv_row_guard_stops_before_next_record_ownership_and_ordinal_is_charged() {
        let mut decoder = Decoder::<std::convert::Infallible>::default();
        let mut source = csv_source();
        let limits = DecodeLimits {
            rows: 1,
            cells: 2,
            storage_bytes: 211,
            work_bytes: 6,
        };
        assert!(decoder
            .decode_bounded(&source, b"A\nx\ny\n", None, limits)
            .is_err());
        assert_eq!(decoder.usage().unwrap().storage_bytes, 114);
        source.ordinal = Some("POSITION".into());
        let table = decoder
            .decode_bounded(&source, b"A\nx\n", None, limits)
            .unwrap();
        assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Int(1));
        assert_eq!(decoder.usage().unwrap().storage_bytes, 211);
        assert!(matches!(
            decoder.decode_bounded(
                &source,
                b"A\nx\n",
                None,
                DecodeLimits {
                    storage_bytes: 210,
                    ..limits
                }
            ),
            Err(DecodeError::Codec(Error::Source(TextTableError::Table(
                crate::arrow_table::TableError::Limit {
                    resource: "storage_bytes",
                    limit: 210,
                    required: Some(211),
                }
            ))))
        ));
    }

    #[test]
    fn native_parquet_receipts_enforce_independent_stage_byte_fences() {
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
        let limits = DecodeLimits {
            rows: 64,
            cells: 64,
            storage_bytes: 2_048,
            work_bytes: 2_048,
        };
        let mut decoder = Decoder::<std::convert::Infallible>::default();
        let table = decoder
            .decode_bounded(&source, &bytes, None, limits)
            .unwrap();
        assert_eq!(table.row_count(), 64);
        assert_eq!(table.cell(63, 0).unwrap(), ValueRef::Int(64));
        let usage = decoder.usage().unwrap();
        assert_eq!((usage.storage_bytes, usage.work_bytes), (2_048, 2_048));
        assert!(decoder
            .decode_bounded(
                &source,
                &bytes,
                None,
                DecodeLimits {
                    storage_bytes: 2_047,
                    ..limits
                }
            )
            .is_err());
        assert_eq!(decoder.usage().unwrap().storage_bytes, 2_016);
        assert!(decoder
            .decode_bounded(
                &source,
                &bytes,
                None,
                DecodeLimits {
                    work_bytes: 2_047,
                    ..limits
                }
            )
            .is_err());
        assert!(decoder.usage().unwrap().work_bytes <= 2_047);
        assert_eq!(decoder.usage().unwrap().storage_bytes, bytes.len());
        assert!(decoder
            .decode_bounded(&source, &bytes, None, DecodeLimits { rows: 63, ..limits })
            .is_err());
        assert_eq!(decoder.usage().unwrap().work_bytes, 0);
        assert_eq!(decoder.usage().unwrap().storage_bytes, bytes.len());
    }

    #[test]
    fn both_supported_odm_routes_return_bounded_receipts_at_their_exact_charge() {
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
            let limits = DecodeLimits {
                rows: 1,
                cells: 11,
                storage_bytes: 65_536,
                work_bytes: 65_536,
            };
            let table = decoder
                .decode_bounded_interpreted(
                    &source,
                    &bytes,
                    None,
                    limits,
                    SourceInterpretation::Odm,
                )
                .unwrap();
            assert_eq!(table.row_count(), 1);
            assert_eq!(table.schema().columns().len(), 11);
            for column in 0..11 {
                assert_eq!(table.cell(0, column).unwrap(), ValueRef::Str("a"));
            }
            let usage = decoder
                .usage()
                .expect("successful supported interpretation is accounted");
            assert!(usage.storage_bytes > 0 && usage.work_bytes > 0);
            let exact = DecodeLimits {
                storage_bytes: usage.storage_bytes,
                work_bytes: usage.work_bytes,
                ..limits
            };
            decoder
                .decode_bounded_interpreted(&source, &bytes, None, exact, SourceInterpretation::Odm)
                .unwrap();
            assert_eq!(decoder.usage().unwrap(), usage);
        }
    }

    #[test]
    fn both_sas_source_profiles_use_receipts_and_preserve_exact_caller_caps() {
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
            let limits = DecodeLimits {
                rows: 1000,
                cells: 100_000,
                storage_bytes: 4_194_304,
                work_bytes: 4_194_304,
            };
            let mut decoder = Decoder::<std::convert::Infallible>::default();
            let table = decoder
                .decode_bounded(&source, bytes, None, limits)
                .unwrap();
            assert!(table.row_count() > 0);
            let usage = decoder
                .usage()
                .expect("successful supported SAS profile is accounted");
            assert!(usage.storage_bytes > 0 && usage.work_bytes > 0);
            let exact = DecodeLimits {
                storage_bytes: usage.storage_bytes,
                work_bytes: usage.work_bytes,
                ..limits
            };
            let repeated = decoder.decode_bounded(&source, bytes, None, exact).unwrap();
            assert_eq!(repeated.row_count(), table.row_count());
            assert_eq!(decoder.usage().unwrap(), usage);
            assert!(decoder
                .decode_bounded(
                    &source,
                    bytes,
                    None,
                    DecodeLimits {
                        storage_bytes: usage.storage_bytes - 1,
                        ..exact
                    }
                )
                .is_err());
            assert!(decoder.usage().unwrap().storage_bytes < usage.storage_bytes);
        }
    }

    #[test]
    fn generated_contract_csv_admits_100k_and_retains_exact_cell_and_ordinal_boundaries() {
        let document = crate::shipped_schema::capture()
            .unwrap()
            .prepare_standalone(Source {
                identity: "producer.yaml".into(),
                bytes: b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {RAW: raw.csv}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\n  - {name: VALUE, type: int, label: Value, derivation: RAW.VALUE}\noutput: {path: produced.csv, columns: [ID, VALUE]}\n".to_vec(),
            })
            .unwrap();
        let contract =
            yamaa_core::producer_contract::prepare(document.model(), Default::default()).unwrap();
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
        let limits = DecodeLimits {
            rows: 100_000,
            cells: 200_000,
            storage_bytes: 16_777_216,
            work_bytes: 16_777_216,
        };
        let table = Decoder::<std::convert::Infallible>::default()
            .decode_bounded(&source, bytes.as_bytes(), Some(&contract), limits)
            .unwrap();
        assert_eq!(table.row_count(), 100_000);
        for row in 0..100_000 {
            assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Int(row as i64 + 1));
            assert_eq!(table.cell(row, 1).unwrap(), ValueRef::Int(1));
        }
        assert!(matches!(
            Decoder::<std::convert::Infallible>::default().decode_bounded(
                &source,
                bytes.as_bytes(),
                Some(&contract),
                DecodeLimits {
                    cells: 199_999,
                    ..limits
                }
            ),
            Err(DecodeError::Codec(Error::Source(TextTableError::Table(
                crate::arrow_table::TableError::Limit {
                    resource: "cells",
                    limit: 199_999,
                    required: Some(200_000)
                }
            ))))
        ));
        source.ordinal = Some("POSITION".into());
        let table = Decoder::<std::convert::Infallible>::default()
            .decode_bounded(
                &source,
                bytes.as_bytes(),
                Some(&contract),
                DecodeLimits {
                    cells: 300_000,
                    ..limits
                },
            )
            .unwrap();
        assert_eq!(table.schema().columns()[2].name, "POSITION");
        assert_eq!(table.cell(99_999, 2).unwrap(), ValueRef::Int(100_000));
        assert!(matches!(
            Decoder::<std::convert::Infallible>::default().decode_bounded(
                &source,
                bytes.as_bytes(),
                Some(&contract),
                DecodeLimits {
                    cells: 299_999,
                    ..limits
                }
            ),
            Err(DecodeError::Codec(Error::ResourceLimit(refusal))) if refusal.resource == "cells" && refusal.limit == 299_999 && refusal.actual == Some(300_000) && refusal.spec_paths == ["input.P"]
        ));
    }
}
