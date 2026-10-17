//! Facade-owned result tables over shared immutable Arrow buffers.
//!
//! [`Table`] is the facade form of the public table: owned Arrow record
//! batches with their schema, plus `columns()`, `rows()`, `ipc()` and
//! `stream()`. Stream export follows the Arrow host-handoff ownership rules:
//! each export owns an independent cursor with shared immutable buffer
//! references. The Arrow type of each public column type is committed:
//! strings are `Utf8`, integers `Int64`, floats `Float64`, dates `Date32`
//! (days) and zone-free datetimes `TimestampSecond`. No specification column
//! ever surfaces as an Arrow `Boolean`.
//!
//! Host frames cross into the engine only as opaque [`InputFrame`] handles;
//! hosts never decode engine tables except through [`Table`].

use std::sync::Arc;

use crate::error::TableError;

pub use arrow_array::ffi_stream::FFI_ArrowArrayStream;
pub use arrow_array::{
    Array, Date32Array, Float64Array, Int64Array, LargeStringArray, RecordBatch, StringArray,
    StringViewArray, TimestampSecondArray,
};
pub use arrow_schema::{DataType, Field, Fields, SchemaRef, TimeUnit};

/// An owned immutable result table with shared Arrow buffers.
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

    /// Borrow the public Arrow fields in declaration order.
    pub fn columns(&self) -> &Fields {
        self.inner.columns()
    }

    /// Borrow the public Arrow schema.
    pub fn schema(&self) -> &SchemaRef {
        self.inner.schema()
    }

    /// Borrow the owned record batches in record order.
    pub fn batches(&self) -> &[RecordBatch] {
        self.inner.batches()
    }

    /// Count the logical rows across all batches.
    pub fn rows(&self) -> usize {
        self.inner.rows()
    }

    /// Encode a copied IPC stream; host views use the C stream bridge.
    pub fn ipc(&self) -> Result<Vec<u8>, TableError> {
        self.inner.ipc().map_err(TableError::from_ipc)
    }

    /// Export one stream with an independent cursor and shared buffers.
    pub fn stream(&self) -> FFI_ArrowArrayStream {
        self.inner.stream()
    }
}

/// An opaque host frame admitted into the engine. Frames originate with the
/// host (an Arrow C stream the host owns) and are consumed by the engine on
/// capture; they carry no readable table surface.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct InputFrame {
    inner: Arc<yamaa_adapters::arrow_table::ArrowTable>,
}

impl InputFrame {
    /// Consume a standard host stream into the engine's validated snapshot.
    /// Primitive and UTF-8 buffers are shared; other physical types require
    /// an explicit preserving conversion by the caller before exporting.
    pub fn import(stream: FFI_ArrowArrayStream) -> Result<Self, TableError> {
        let table = yamaa_adapters::arrow_bridge::import_public_stream(stream)
            .map_err(TableError::from_table)?;
        Ok(Self {
            inner: Arc::new(table),
        })
    }

    pub(crate) fn into_adapter(self) -> Arc<yamaa_adapters::arrow_table::ArrowTable> {
        self.inner
    }

    pub(crate) fn from_adapter_table(table: Arc<yamaa_adapters::arrow_table::ArrowTable>) -> Self {
        Self { inner: table }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_streams_fail_with_a_stable_table_code() {
        let error = InputFrame::import(FFI_ArrowArrayStream::empty()).unwrap_err();
        assert!(!error.code().is_empty());
        assert!(!error.message().is_empty());
    }
}
