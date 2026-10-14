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
fn arithmetic_predicates_share_the_structured_context_and_suggestion() {
    // The three predicate sites from #2204: a case `when`, a root `filter`
    // and an `assert.require`. Each reports invalid_predicate (REQ-0188)
    // with the same {position, predicate, suggestion} context; the
    // assert.require site no longer substitutes a prose `reason`.
    let head = "schema_version: '1.0'\ndomain: ADVS\nkeys: [USUBJID]\ninput: {VS: {path: never-read.csv, types: {SBPSUP: float, SBPSTD: float}}}\noutput: {path: advs.csv, columns: [USUBJID, OHYPO]}\ncolumns:\n  - {name: USUBJID, type: str, derivation: VS.USUBJID}\n  - {name: SBPSUP, type: float, derivation: VS.SBPSUP}\n  - {name: SBPSTD, type: float, derivation: VS.SBPSTD}\n";
    let suggestion = "derive the value as a column with compute, then compare that column";
    for (extra, path, predicate, position) in [
        (
            "  - name: OHYPO\n    type: str\n    derivation:\n      case:\n        - {when: 'SBPSTD - SBPSUP <= -20', then: {literal: 'Y'}}\n        - {otherwise: {literal: 'N'}}\n",
            "columns.OHYPO.derivation.case[0].when",
            "SBPSTD - SBPSUP <= -20",
            7,
        ),
        (
            "  - {name: OHYPO, type: str, derivation: {literal: 'N'}}\nfilter: 'VS.SBPSTD - VS.SBPSUP < 0'\n",
            "filter",
            "VS.SBPSTD - VS.SBPSUP < 0",
            10,
        ),
        (
            "  - {name: OHYPO, type: str, derivation: {literal: 'N'}}\nverifications:\n  - {assert: {when: \"OHYPO = 'Y'\", require: 'SBPSTD - SBPSUP <= -20'}}\n",
            "verifications[0].assert.require",
            "SBPSTD - SBPSUP <= -20",
            7,
        ),
    ] {
        let document = shipped_schema::capture()
            .unwrap()
            .prepare_standalone(Source {
                identity: "spec.yaml".into(),
                bytes: format!("{head}{extra}").into_bytes(),
            })
            .unwrap();
        let run = PreparedRun::prepare(document).unwrap();
        let rows = specification_check::issue_rows(&run).unwrap();
        assert_eq!(rows.len(), 1, "{path}: {rows:?}");
        assert_eq!(rows[0].condition, "invalid_predicate", "{path}");
        assert_eq!(rows[0].requirement.as_deref(), Some("REQ-0188"), "{path}");
        assert_eq!(rows[0].spec_paths, [path], "{path}");
        let context: serde_json::Value = serde_json::from_str(&rows[0].context).unwrap();
        assert_eq!(
            context,
            serde_json::json!({"position": position, "predicate": predicate, "suggestion": suggestion}),
            "{path}"
        );
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
