//! Submission resource refusals retain typed facts before accessing held cells.
#[allow(dead_code)]
mod support;
use support::diagnostic::submission::*;
use yamaa_core::limits::Limits;

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
    let study = one(project(), specification(), &ct::Catalogue::default()).unwrap();
    let finding =
        submission_transport::admit(&study, &study.datasets()[0], &Oversized).unwrap_err();
    assert_eq!(finding.code, ConditionCode::ApplicationRejected);
    assert_eq!(finding.definition().requirement, None);
    assert_eq!(finding.path, "datasets.D0.transport");
    assert_eq!(finding.context["stage"], "output");
    assert_eq!(finding.context["code"], "table_limit");
    assert_eq!(finding.context["resource"], "table_rows");
    assert_eq!(finding.context["limit"], 10_000_000usize);
    assert_eq!(finding.context["actual"], 10_000_001usize);
}
