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
}

impl<E> yamaa_engine::producer_build::DecodePort for Decoder<E> {
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
            if let Some(diagnostic) = contract
                .validate_header(&source.name, &header, contract_limits)
                .map_err(DecodeError::Contract)?
            {
                return Err(DecodeError::Metadata(diagnostic));
            }
            crate::typed_csv::convert(
                parsed,
                types,
                crate::arrow_table::TableLimits {
                    max_rows: 65_536,
                    max_columns: 64,
                    max_batches: 1,
                    max_cells: limits.cells.min(262_144),
                },
            )
            .map_err(|e| DecodeError::Codec(crate::specification_run::typed_source_error(e)))?
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
