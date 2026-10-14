use yamaa_adapters::{
    shipped_schema, specification_check, specification_run::PreparedRun,
    specification_source::Source,
};

fn checked(extra: &str, derivation: &str) -> PreparedRun {
    let raw = format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{DM: never-read.csv}}\noutput: {{path: result.csv, columns: [ID, V]}}\n{extra}\ncolumns:\n  - {{name: ID, type: str, label: ID, derivation: DM.ID}}\n  - name: V\n    type: float\n    label: V\n    derivation: {derivation}\n");
    let document = shipped_schema::capture()
        .unwrap()
        .prepare_standalone(Source {
            identity: "spec.yaml".into(),
            bytes: raw.into_bytes(),
        })
        .unwrap();
    PreparedRun::prepare(document).unwrap()
}

#[test]
fn original_parser_and_window_declaration_truth_is_available_without_source_authority() {
    for (extra, derivation, condition, path) in [
        (
            "",
            "{compute: {expr: '2 +'}}",
            "invalid_numeric_expression",
            "columns.V.derivation.compute.expr",
        ),
        (
            "",
            "{aggregate: 'SUM(DM.V) +'}",
            "invalid_aggregate_expression",
            "columns.V.derivation.aggregate",
        ),
        (
            "",
            "{flag: {condition: 'DM.V != 1'}}",
            "invalid_predicate",
            "columns.V.derivation.flag.condition",
        ),
        (
            "filter: 'DM.V != 1'",
            "{literal: 1}",
            "invalid_predicate",
            "filter",
        ),
        (
            "",
            "{source: {variable: DM.V, filter: 'DM.V != 1'}}",
            "invalid_predicate",
            "columns.V.derivation.source.filter",
        ),
        (
            "",
            "{row_value: {source: V, offset: 0, window: {order_by: [ID]}}}",
            "zero_offset",
            "columns.V.derivation.row_value.offset",
        ),
        (
            "",
            "{rank: {}}",
            "window_order_by_required",
            "columns.V.derivation.rank.window",
        ),
        (
            "",
            "{case: [{when: 'DM.V != 1', then: {literal: 1}}, {otherwise: {literal: 2}}]}",
            "invalid_predicate",
            "columns.V.derivation.case[0].when",
        ),
    ] {
        let rows = specification_check::issue_rows(&checked(extra, derivation)).unwrap();
        assert_eq!(rows.len(), 1, "{derivation}: {rows:?}");
        assert_eq!(rows[0].condition, condition, "{derivation}");
        assert_eq!(rows[0].spec_paths, [path], "{derivation}");
        assert!(rows[0].requirement.is_some());
    }
}

#[test]
fn clean_declarations_still_defer_actual_dataset_fields_and_values_to_build() {
    assert!(
        specification_check::issue_rows(&checked("", "{compute: {expr: 'UNKNOWN + 1'}}"))
            .unwrap()
            .is_empty()
    );
    assert!(
        specification_check::issue_rows(&checked("", "{compute: {expr: '1 / 0'}}"))
            .unwrap()
            .is_empty()
    );
}
