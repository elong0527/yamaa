//! Submission resource refusals retain typed facts before accessing held cells.
//!
//! The seam is `submission_transport::admit` over a `TableAccess` double
//! that panics on any schema or cell access, so the order of admission is
//! observed; no replay case or benchmark can supply such a table.
use crate::support::submission::{one, project, specification};
use yamaa_core::{
    diagnostic::ConditionCode,
    limits::Limits,
    project_terminology as ct, submission_transport,
    table::{CellError, TableAccess, TableSchema, ValueRef},
};

#[test]
fn oversized_published_table_refuses_before_schema_or_cell_access() {
    struct Oversized;
    impl TableAccess for Oversized {
        type Error = ();
        fn schema(&self) -> &TableSchema {
            panic!("geometry admission precedes schema access")
        }
        fn row_count(&self) -> usize {
            Limits::TABLE_ROWS + 1
        }
        fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<()>> {
            panic!("geometry admission precedes cell access")
        }
    }
    let catalogue = ct::Catalogue::admit(Vec::new()).unwrap();
    let study = one(project(), specification(), &catalogue).unwrap();
    let finding =
        submission_transport::admit(&study, &study.datasets()[0], &Oversized).unwrap_err();
    assert_eq!(finding.code, ConditionCode::ApplicationRejected);
    assert_eq!(finding.definition().rule, None);
    assert_eq!(finding.path, "datasets.D0.transport");
    assert_eq!(finding.context["step"], "define");
    assert_eq!(finding.context["code"], "resource_limit");
    assert_eq!(
        finding.context["detail"],
        "table_rows: required Some(10000001), limit 10000000"
    );
    assert_eq!(finding.context["resource"], "table_rows");
    assert_eq!(finding.context["limit"], 10_000_000usize);
    assert_eq!(finding.context["actual"], 10_000_001usize);
}
