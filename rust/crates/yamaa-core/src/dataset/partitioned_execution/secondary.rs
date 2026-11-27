//! A partitioned run's failure on a record of another input (#5086): no
//! partitioning divides a secondary input, so the failing record keeps its
//! own position, which a whole-table run reports; only a driver record or a
//! candidate moves to the whole input's position.
//!
//! The seam is `partitions::rebase` over `ExecutionError::Predicate`, whose
//! row no host diagnostic names: no benchmark or host fixture can tell a
//! moved row from a kept one.
use super::*;
use crate::dataset::{
    Intermediate, IntermediateSource, RelationPredicate, RowMode, RowTemplate, SecondarySource,
    SourceSchemas,
};

/// Four driver records keyed K = 1..=4, and a column V read from the
/// intermediate L over the input SEC, matched on K and filtered by
/// `V * 2^62 > 0`, which overflows on SEC's third record, where V is 2.
/// The filter reads only SEC's records, so a run that partitions its
/// driver records reads L whole in each partition.
fn secondary_filter() -> (DatasetPlan, Rows, Rows) {
    let integer = ColumnType::Int;
    let driver = schema(&[("K", integer)]);
    let other = schema(&[("K", integer), ("V", integer)]);
    let lookup = Intermediate {
        identifier: "L".into(),
        path: "intermediates[0]".into(),
        source: IntermediateSource::Input(0),
        keys: vec![IntermediateKey {
            source_column: 0,
            path: "intermediates[0].key[0]".into(),
            value: MatchValue::Variable(MatchRead::Output {
                column: 0,
                identifier: "K".into(),
            }),
        }],
        filter: Some(RelationPredicate::from(predicate(
            "V * 4611686018427387904 > 0",
            "intermediates[0].filter",
            vec![Bound {
                name: "V".into(),
                read: BoundRead::Source(1),
            }],
        ))),
        selection: None,
        nearest: None,
        pivot: None,
        range: None,
        columns: None,
        no_match: Some(Value::Missing),
        require_match: false,
        unique: Vec::new(),
        derivations: None,
        pending_schema: None,
        records: None,
    };
    let plan = DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: driver.clone(),
            secondary: vec![SecondarySource {
                name: "SEC".into(),
                schema: other.clone(),
            }],
        },
        vec![lookup],
        schema(&[("K", integer), ("V", integer)]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(
                    1,
                    Expression::Intermediate {
                        index: 0,
                        column: 1,
                    },
                ),
            ],
            filter: None,
        }],
        Vec::new(),
        vec![0],
        Vec::new(),
    );
    let input = Rows::new(driver, (1..=4).map(|k| vec![int(k)]).collect());
    let records = [(1, 1), (2, 1), (3, 2), (4, 1)];
    let secondary = Rows::new(
        other,
        records.iter().map(|&(k, v)| vec![int(k), int(v)]).collect(),
    );
    (plan, input, secondary)
}

#[test]
fn a_filter_failing_on_another_inputs_record_names_its_own_row_over_any_partitions() {
    let (plan, input, secondary) = secondary_filter();
    let resolved = plan.resolve().unwrap();
    let whole = resolved.execute(
        &input,
        &[&secondary],
        &mut NoFunctions::new(),
        None,
        &|| false,
    );
    // The third driver record, K = 3, is the first whose match reads SEC's
    // third record, row 2.
    let failure = match &whole.result {
        Err(error) => match &**error {
            ExecutionError::Predicate {
                source,
                source_row,
                error,
            } => Some((*source, *source_row, error.spec_path.as_str())),
            _ => None,
        },
        Ok(_) => None,
    };
    assert_eq!(
        failure,
        Some((Some(0), 2, "intermediates[0].filter")),
        "the whole-table run's failure"
    );
    for sizes in divisions(input.rows.len()) {
        let attempt = as_stated(partitioned(&resolved, &input, &[&secondary], &sizes));
        assert_eq!(attempt, whole, "partitions {sizes:?}");
    }
}
