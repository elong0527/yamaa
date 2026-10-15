//! Deterministic native fixtures for application boundary tests.
//!
//! Available to downstream tests only through the opt-in `test-support` feature.
//! These ports consume scripted outcomes once, log before each effect, and panic
//! on an unexpected request. They perform no I/O, codec work, activation or retry.
//! Owned errors are moved intact and need neither Clone nor Send nor Sync.
use alloc::{rc::Rc, string::String, vec::Vec};
use core::{
    cell::{Ref, RefCell},
    marker::PhantomData,
};
use yamaa_core::{
    specification::SourceDeclaration,
    table::{CellError, TableAccess, TableSchema, ValueRef},
    value::Value,
};

use crate::{
    dataset, function_invocation, project_preparation as metadata, specification_output as output,
    specification_run as run,
};

mod ports;
pub use ports::*;

/// Ordered observations shared by independently owned fake ports.
#[derive(Clone, Debug)]
pub enum Event {
    Inspect(SourceDeclaration),
    Capture {
        source: SourceDeclaration,
        byte_limit: usize,
    },
    Decode {
        source: SourceDeclaration,
        bytes: Vec<u8>,
    },
    MetadataCapture {
        kind: metadata::Kind,
        declaring_source: String,
        written: String,
        remaining_bytes: usize,
        remaining_identity_bytes: usize,
    },
    MetadataDecode {
        target: metadata::DecodeTarget,
        identity: String,
        bytes: Vec<u8>,
    },
    Schema,
    Rows,
    Cell {
        row: usize,
        column: usize,
    },
    Call {
        slot: usize,
        arguments: Vec<(String, Value)>,
    },
    Encode {
        projection: Vec<usize>,
        byte_limit: usize,
    },
    Publish {
        path: String,
        bytes: Vec<u8>,
    },
    Observation(&'static str),
}

fn arguments_equal(left: &[(String, Value)], right: &[(String, Value)]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|((name, left), (other, right))| {
                name == other
                    && match (left, right) {
                        (Value::Float(left), Value::Float(right)) => {
                            left.get().to_bits() == right.get().to_bits()
                        }
                        _ => left == right,
                    }
            })
}
impl PartialEq for Event {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Inspect(left), Self::Inspect(right)) => left == right,
            (
                Self::Capture {
                    source: left,
                    byte_limit: a,
                },
                Self::Capture {
                    source: right,
                    byte_limit: b,
                },
            ) => left == right && a == b,
            (
                Self::MetadataCapture {
                    kind: a,
                    declaring_source: b,
                    written: c,
                    remaining_bytes: d,
                    remaining_identity_bytes: e,
                },
                Self::MetadataCapture {
                    kind: v,
                    declaring_source: w,
                    written: x,
                    remaining_bytes: y,
                    remaining_identity_bytes: z,
                },
            ) => a == v && b == w && c == x && d == y && e == z,
            (
                Self::MetadataDecode {
                    target: a,
                    identity: b,
                    bytes: c,
                },
                Self::MetadataDecode {
                    target: x,
                    identity: y,
                    bytes: z,
                },
            ) => a == x && b == y && c == z,
            (
                Self::Decode {
                    source: left,
                    bytes: a,
                },
                Self::Decode {
                    source: right,
                    bytes: b,
                },
            ) => left == right && a == b,
            (Self::Schema, Self::Schema) | (Self::Rows, Self::Rows) => true,
            (Self::Cell { row: a, column: b }, Self::Cell { row: c, column: d }) => {
                a == c && b == d
            }
            (
                Self::Call {
                    slot: a,
                    arguments: left,
                },
                Self::Call {
                    slot: b,
                    arguments: right,
                },
            ) => a == b && arguments_equal(left, right),
            (
                Self::Encode {
                    projection: left,
                    byte_limit: a,
                },
                Self::Encode {
                    projection: right,
                    byte_limit: b,
                },
            ) => left == right && a == b,
            (
                Self::Publish {
                    path: left,
                    bytes: a,
                },
                Self::Publish {
                    path: right,
                    bytes: b,
                },
            ) => left == right && a == b,
            (Self::Observation(left), Self::Observation(right)) => left == right,
            _ => false,
        }
    }
}

/// Cloning the log shares observations; it does not clone any port or error.
#[derive(Clone, Default)]
pub struct EventLog(Rc<RefCell<Vec<Event>>>);
impl EventLog {
    pub fn record(&self, event: Event) {
        self.0.borrow_mut().push(event);
    }
    pub fn events(&self) -> Ref<'_, [Event]> {
        Ref::map(self.0.borrow(), Vec::as_slice)
    }
    /// Only the test caller can clear observations; no port clears its history.
    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }
}

/// Immutable owned rows with observable schema/cardinality/cell access.
/// Types are fixture declarations, not a codec or a conversion policy.
pub struct FixtureTable<E = core::convert::Infallible> {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    log: EventLog,
    error: PhantomData<E>,
}
impl<E> FixtureTable<E> {
    pub fn new(schema: TableSchema, rows: Vec<Vec<Value>>, log: EventLog) -> Self {
        assert!(
            rows.iter().all(|row| row.len() == schema.columns().len()),
            "fixture row width"
        );
        Self {
            schema,
            rows,
            log,
            error: PhantomData,
        }
    }
}
impl<E> TableAccess for FixtureTable<E> {
    type Error = E;
    fn schema(&self) -> &TableSchema {
        self.log.record(Event::Schema);
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.log.record(Event::Rows);
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<E>> {
        self.log.record(Event::Cell { row, column });
        self.rows
            .get(row)
            .and_then(|values| values.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}

#[cfg(test)]
mod tests;
