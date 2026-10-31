//! Factories and observers owned by the yamaa integration suite.

pub mod peak;
pub mod scratch;

pub use scratch::Scratch;

/// Every issue whole: its severity, phase, condition, rule, spec paths,
/// context, source span and operand route; its message is not frozen.
pub fn issues(diagnostics: &[yamaa::Diagnostic]) -> String {
    assert!(!diagnostics.is_empty(), "an issue");
    diagnostics
        .iter()
        .map(|issue| {
            format!(
                "{:?} {} {} {:?} {:?} {} {:?} {:?}",
                issue.severity,
                issue.phase,
                issue.code,
                issue.rule,
                issue.spec_path,
                issue.context.to_json(),
                issue.source_span,
                issue.operand_route,
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Held by each test that runs beyond the row limit, so no two of them run
/// at once: each takes several gigabytes, and libtest runs the tests of this
/// one binary side by side, which a 7 GB runner cannot hold for two (#3869).
/// A test that panicked while holding it leaves it poisoned, and the next
/// one takes it all the same.
pub fn beyond_the_row_limit() -> std::sync::MutexGuard<'static, ()> {
    static ALONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    ALONE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Project functions for specifications that call none: a call is a bug in
/// the test.
pub struct NoFunctions;
impl yamaa::ProjectFunctions for NoFunctions {
    fn verify_lock(
        &self,
        _: &yamaa::host::functions::LockContext<'_>,
        _: &[yamaa::host::functions::FunctionIdentity],
    ) -> Result<(), yamaa::host::functions::FunctionError> {
        Ok(())
    }
    fn bind(
        &self,
        _: &yamaa::host::functions::FunctionIdentity,
        _: &yamaa::host::functions::Signature,
    ) -> Result<yamaa::host::functions::FunctionHandle, yamaa::host::functions::FunctionError> {
        unreachable!("the test calls no project function")
    }
    fn invoke(
        &self,
        _: &yamaa::host::functions::FunctionHandle,
        _: &[yamaa::host::functions::Argument<'_>],
    ) -> Result<yamaa::scalar::Scalar, yamaa::host::functions::CallError> {
        unreachable!("the test calls no project function")
    }
}

/// A Parquet file's field names and rows as the engine's Parquet reader
/// reads them back, each cell its type and value, a float by its bits: two
/// files that read back alike have equal values (PARQUET-4), whatever their
/// bytes (D305). `None` when the reader refuses the file.
#[cfg(feature = "parquet")]
pub fn parquet_values(bytes: &[u8]) -> Option<(Vec<String>, Vec<Vec<String>>)> {
    use yamaa_core::table::{TableAccess, ValueRef};
    let table = yamaa_adapters::parquet_source::parse(bytes).ok()?;
    let columns = table.schema().columns();
    let names = columns.iter().map(|column| column.name.clone()).collect();
    let mut rows = Vec::with_capacity(table.row_count());
    for row in 0..table.row_count() {
        let mut cells = Vec::with_capacity(columns.len());
        for column in 0..columns.len() {
            cells.push(match table.cell(row, column).ok()? {
                ValueRef::Missing => "missing".to_owned(),
                ValueRef::Str(text) => format!("str {text:?}"),
                ValueRef::Int(value) => format!("int {value}"),
                ValueRef::Float(value) => format!("float {:016x}", value.get().to_bits()),
                ValueRef::Date(date) => format!("date {:?}", date.fields()),
                ValueRef::DateTime(datetime) => format!("datetime {:?}", datetime.fields()),
                ValueRef::Bool(value) => format!("bool {value}"),
            });
        }
        rows.push(cells);
    }
    Some((names, rows))
}
