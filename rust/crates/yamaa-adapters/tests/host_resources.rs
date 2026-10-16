use arrow_array::{Float64Array, RecordBatch};
use std::sync::Arc;
use yamaa_adapters::{
    arrow_table::{self, ArrowTable},
    file_resources::{Error, Resources},
    host_resources::{Failure, MemoryResolver, Operation, Reply, Request, Resolver},
};
use yamaa_core::{
    table::{Column, TableSchema},
    temporal::{Date, DatePrecision, DateTime, DateTimePrecision},
    value::ColumnType,
};

#[test]
fn explicit_reference_reuses_held_bytes_across_aliases_and_refuses_changes() {
    let backend = Arc::new(MemoryResolver::default());
    backend.insert(
        "/",
        "one.csv",
        "object:version-1",
        Arc::from(&b"A\n7\n"[..]),
    );
    backend.insert(
        "/",
        "alias.csv",
        "object:version-1",
        Arc::from(&b"A\n7\n"[..]),
    );
    let mut resources = Resources::with_resolver(backend.clone(), "spec.yaml");
    let (one, fresh) = resources.capture("one.csv").unwrap();
    assert!(fresh);
    let (alias, fresh) = resources.capture("alias.csv").unwrap();
    assert!(!fresh);
    assert!(Arc::ptr_eq(&one, &alias));
    assert_eq!(resources.capture_reads(), 1);
    backend.insert(
        "/",
        "one.csv",
        "object:version-2",
        Arc::from(&b"A\n7\n"[..]),
    );
    assert_eq!(
        resources.capture("one.csv"),
        Err(Error::Host(Failure::Changed))
    );
    assert_eq!(one.as_ref(), b"A\n7\n");
    let mut fresh_attempt = Resources::with_resolver(backend, "spec.yaml");
    assert!(fresh_attempt.capture("one.csv").unwrap().1);
}

#[test]
fn absent_and_cancelled_resources_remain_explicit_without_captures() {
    let backend = Arc::new(MemoryResolver::default());
    let resources = Resources::with_resolver(backend.clone(), "spec.yaml");
    assert_eq!(
        resources.inspect("missing.csv"),
        Err(Error::Host(Failure::Missing))
    );
    backend.cancel();
    assert_eq!(
        resources.inspect("missing.csv"),
        Err(Error::Host(Failure::Cancelled))
    );
    assert_eq!(resources.capture_reads(), 0);
}

fn frame(
    float: f64,
    date_precision: DatePrecision,
    timestamp_precision: DateTimePrecision,
) -> Arc<ArrowTable> {
    let date = Date::new(2026, 1, 1, date_precision).unwrap();
    let timestamp = DateTime::new(date, 0, 0, 0, timestamp_precision).unwrap();
    let logical = TableSchema::new(vec![
        Column {
            name: "F".into(),
            kind: ColumnType::Float,
        },
        Column {
            name: "D".into(),
            kind: ColumnType::Date,
        },
        Column {
            name: "T".into(),
            kind: ColumnType::DateTime,
        },
    ])
    .unwrap();
    let batch = RecordBatch::try_new(
        arrow_table::physical_schema(&logical),
        vec![
            Arc::new(Float64Array::from(vec![float])),
            Arc::new(arrow_table::date_array(&[Some(date)]).unwrap()),
            Arc::new(arrow_table::datetime_array(&[Some(timestamp)]).unwrap()),
        ],
    )
    .unwrap();
    Arc::new(ArrowTable::try_new(logical, vec![batch]).unwrap())
}

#[test]
fn frame_freshness_compares_signed_zero_and_collected_temporal_precision() {
    for changed in [
        frame(-0.0, DatePrecision::Day, DateTimePrecision::Second),
        frame(0.0, DatePrecision::Year, DateTimePrecision::Second),
        frame(0.0, DatePrecision::Day, DateTimePrecision::Day),
    ] {
        let backend = Arc::new(MemoryResolver::default());
        let original = frame(0.0, DatePrecision::Day, DateTimePrecision::Second);
        backend.insert_frame("/", "raw.csv", "warehouse:version-1", original);
        let mut resources = Resources::with_resolver(backend.clone(), "spec.yaml");
        assert!(resources.capture("raw.csv").unwrap().1);
        backend.insert_frame("/", "raw.csv", "warehouse:version-1", changed);
        assert_eq!(
            resources.capture("raw.csv"),
            Err(Error::Host(Failure::Changed))
        );
    }
}

#[cfg(feature = "yaml")]
struct InterruptStage(MemoryResolver);
#[cfg(feature = "yaml")]
impl Resolver for InterruptStage {
    fn request(&self, request: Request<'_>) -> Result<Reply, Failure> {
        let operation = request.operation;
        let reply = self.0.request(request)?;
        if operation == Operation::Stage {
            Err(Failure::Cancelled)
        } else {
            Ok(reply)
        }
    }
}

#[cfg(feature = "yaml")]
#[test]
fn interrupted_stage_cleans_provisional_output_by_held_target_identity() {
    let backend = Arc::new(InterruptStage(MemoryResolver::default()));
    let spec = b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {RAW: raw.csv}\ncolumns: [{name: ID, type: int, derivation: RAW.ID}]\noutput: {path: out.csv, columns: [ID]}\n";
    backend
        .0
        .insert("/", "spec.yaml", "object:spec-1", Arc::from(&spec[..]));
    backend.0.insert(
        "/spec.yaml",
        "raw.csv",
        "object:data-1",
        Arc::from(&b"ID\n7\n"[..]),
    );
    let resources = Resources::with_resolver(backend.clone(), "spec.yaml");
    let prepared =
        yamaa_adapters::file_preparation::FileSpecification::prepare(resources, "spec.yaml")
            .unwrap();
    let mut result = yamaa_adapters::file_application::plain(
        prepared,
        yamaa_adapters::specification_report::Identity {
            runtime: "rust",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "domain",
            specification: "spec.yaml",
            base_directory: ".",
        },
        yamaa_core::application_observer::Scope::default(),
    )
    .unwrap();
    assert!(!result.save().unwrap());
    assert_eq!(backend.0.staged_count(), 0);
    assert!(backend.0.published("memory:/out.csv").is_none());
}
