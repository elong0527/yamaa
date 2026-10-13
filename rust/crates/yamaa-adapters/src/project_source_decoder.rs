//! Decode existing CSV/Parquet profiles while retaining a caller's opaque error.
//! Snapshot cells remain infallible; the error type is only the engine's shared
//! callback boundary. This grants no host access or activation authority.
use crate::{arrow_table::ArrowTable, specification_run::Error};
use std::marker::PhantomData;
use yamaa_core::{
    specification::SourceDeclaration,
    table::{CellError, TableAccess, TableSchema, ValueRef},
};
use yamaa_engine::specification_run::SourceDecoder;

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
    fn decode(&mut self, source: &SourceDeclaration, bytes: &[u8]) -> Result<Snapshot<E>, Error> {
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
        crate::specification_run::decode_interpreted(source, bytes, None, interpretation).map(
            |table| Snapshot {
                table,
                error: PhantomData,
            },
        )
    }
}

impl<E> yamaa_engine::producer_build::DecodePort for Decoder<E> {
    fn decode_bounded_interpreted(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        contract: Option<&yamaa_core::producer_contract::Contract>,
        limits: yamaa_engine::producer_build::DecodeLimits,
        interpretation: yamaa_core::specification::SourceInterpretation,
    ) -> Result<Snapshot<E>, yamaa_engine::producer_build::DecodeError<Error>> {
        if interpretation == yamaa_core::specification::SourceInterpretation::Ordinary {
            return self.decode_bounded(source, bytes, contract, limits);
        }
        crate::specification_run::decode_interpreted(source, bytes, Some(limits), interpretation)
            .map(|table| Snapshot {
                table,
                error: PhantomData,
            })
            .map_err(yamaa_engine::producer_build::DecodeError::Codec)
    }
    fn decode_bounded(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        contract: Option<&yamaa_core::producer_contract::Contract>,
        limits: yamaa_engine::producer_build::DecodeLimits,
    ) -> Result<Snapshot<E>, yamaa_engine::producer_build::DecodeError<Error>> {
        use yamaa_engine::producer_build::DecodeError;
        let table = if let Some(contract) =
            contract.filter(|_| source.profile == yamaa_core::specification::SourceProfile::Csv)
        {
            // Admit stored header order before declared typing can replace a
            // producer mismatch with an unrelated missing-field diagnostic.
            let types =
                yamaa_core::typed_csv::PreparedTypes::new(&source.types, 64).map_err(|e| {
                    DecodeError::Codec(Error::TypedSource(crate::typed_csv::Error::Typing(e)))
                })?;
            let parsed = crate::csv_source::parse(
                bytes,
                crate::specification_run::bounded_csv_limits(limits),
            )
            .map_err(|e| {
                DecodeError::Codec(Error::Source(crate::csv_source::TextTableError::Csv(e)))
            })?;
            let contract_limits = yamaa_core::producer_contract::Limits::default();
            if parsed.names.len() > contract_limits.fields {
                return Err(DecodeError::Limit("producer_source_fields"));
            }
            let header = parsed.names.iter().map(String::as_str).collect::<Vec<_>>();
            if let Some(diagnostic) = yamaa_engine::source_header::validate_header(
                contract,
                &source.name,
                &header,
                contract_limits,
            )
            .map_err(DecodeError::Contract)?
            {
                return Err(DecodeError::Metadata(diagnostic));
            }
            let table = crate::typed_csv::convert(
                parsed,
                types,
                crate::arrow_table::TableLimits {
                    max_rows: yamaa_engine::capacity::SOURCE_ROWS,
                    max_columns: 64,
                    max_batches: 1,
                    max_cells: limits.cells.min(yamaa_engine::capacity::SOURCE_CELLS),
                },
            )
            .map_err(|e| DecodeError::Codec(crate::specification_run::typed_source_error(e)))?;
            crate::source_ordinal::append(
                source,
                table,
                crate::arrow_table::TableLimits {
                    max_rows: yamaa_engine::capacity::SOURCE_ROWS,
                    max_columns: 64,
                    max_batches: 1,
                    max_cells: limits.cells.min(yamaa_engine::capacity::SOURCE_CELLS),
                },
            )
            .map_err(DecodeError::Codec)?
        } else {
            crate::specification_run::decode_source(source, bytes, Some(limits))
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
    use yamaa_core::{specification::SourceProfile, value::ColumnType};
    use yamaa_engine::producer_build::{DecodeError, DecodeLimits, DecodePort};

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
            empty_string_present: false,
            ordinal: None,
        };
        let mut bytes = String::from("ID,VALUE\n");
        for row in 1..=100_000 {
            use std::fmt::Write;
            writeln!(bytes, "{row},1").unwrap();
        }
        let limits = DecodeLimits {
            cells: 200_000,
            storage_bytes: 8_388_608,
            work_bytes: 8_388_608,
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
            Err(DecodeError::Codec(Error::Ordinal(_)))
        ));
    }
}
