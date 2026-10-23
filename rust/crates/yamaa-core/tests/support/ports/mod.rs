//! Deterministic native fixtures for application boundary tests.
//!
//! Private to the core integration tests.
//! These ports consume scripted outcomes once, log before each effect, and panic
//! on an unexpected request. They perform no I/O, codec work, activation or retry.
//! Owned errors are moved intact and need neither Clone nor Send nor Sync.
use std::rc::Rc;
use std::{
    cell::{Ref, RefCell},
    marker::PhantomData,
};
use yamaa_core::{
    specification::SourceDeclaration,
    table::{CellError, TableAccess, TableSchema, ValueRef},
    value::Value,
};

use yamaa_core::{function_invocation, project_preparation as metadata};

mod scripted;
#[allow(unused_imports)]
pub use scripted::*;

/// What a test decoder never decodes: no document, project file, table or
/// failure of it exists.
#[derive(Debug, PartialEq)]
pub enum Never {}
impl TableAccess for Never {
    type Error = Never;
    fn schema(&self) -> &TableSchema {
        match *self {}
    }
    fn row_count(&self) -> usize {
        match *self {}
    }
    fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Never>> {
        match *self {}
    }
}

/// The decoder port's raw document half for a test decoder that decodes no
/// parent layer or dictionary file: such a request panics, as a scripted port
/// does on an unexpected request.
macro_rules! no_raw {
    () => {
        type Raw = $crate::support::ports::Never;
        fn raw(&mut self, identity: &str, _: &[u8]) -> Result<Self::Raw, Self::DocumentError> {
            panic!("unexpected document decode of {identity}")
        }
        fn raw_document(raw: &Self::Raw) -> &yamaa_core::schema::Document {
            match *raw {}
        }
    };
}
pub(crate) use no_raw;

/// The decoder port's project half for a test decoder that reads no file a
/// project names and no lock: such a request panics.
macro_rules! no_project_files {
    () => {
        type Admitted = $crate::support::ports::Never;
        fn admitted(
            &mut self,
            source: yamaa_core::project_preparation::Source,
            _: yamaa_core::project_preparation::DecodeTarget,
        ) -> Result<Self::Admitted, Self::DocumentError> {
            panic!("unexpected project file decode of {}", source.identity)
        }
        fn admitted_document(admitted: &Self::Admitted) -> &yamaa_core::schema::Document {
            match *admitted {}
        }
        fn admitted_source(admitted: &Self::Admitted) -> &yamaa_core::project_preparation::Source {
            match *admitted {}
        }
        fn lock(
            &mut self,
            source: &yamaa_core::project_preparation::Source,
        ) -> Option<yamaa_core::project_environment::LockKind> {
            panic!("unexpected lock read of {}", source.identity)
        }
    };
}
pub(crate) use no_project_files;

/// Both document halves of the decoder port for a test decoder that decodes
/// only tables.
macro_rules! no_documents {
    () => {
        type DocumentError = $crate::support::ports::Never;
        $crate::support::ports::no_raw!();
        $crate::support::ports::no_project_files!();
    };
}
pub(crate) use no_documents;

/// The decoder port's table half for a test decoder that decodes only
/// documents: a table request panics, as a scripted port does on an
/// unexpected request.
macro_rules! no_tables {
    () => {
        type Error = $crate::support::ports::Never;
        type Table = $crate::support::ports::Never;
        fn table(
            &mut self,
            source: &yamaa_core::specification::SourceDeclaration,
            _: &[u8],
            _: yamaa_core::specification::SourceInterpretation,
            _: Option<&yamaa_core::producer_contract::Contract>,
        ) -> Result<Self::Table, Self::Error> {
            panic!("unexpected table decode of {}", source.name)
        }
        fn continue_after(&self, error: &Self::Error) -> bool {
            match *error {}
        }
    };
}
pub(crate) use no_tables;

/// Ordered observations shared by independently owned fake ports.
#[derive(Clone, Debug)]
pub enum Event {
    Inspect(SourceDeclaration),
    Capture {
        source: SourceDeclaration,
    },
    Decode {
        source: SourceDeclaration,
        bytes: Vec<u8>,
    },
    MetadataCapture {
        kind: metadata::Kind,
        declaring_source: String,
        written: String,
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
            (Self::Capture { source: left }, Self::Capture { source: right }) => left == right,
            (
                Self::MetadataCapture {
                    kind: a,
                    declaring_source: b,
                    written: c,
                },
                Self::MetadataCapture {
                    kind: v,
                    declaring_source: w,
                    written: x,
                },
            ) => a == v && b == w && c == x,
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
pub struct FixtureTable<E = std::convert::Infallible> {
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
