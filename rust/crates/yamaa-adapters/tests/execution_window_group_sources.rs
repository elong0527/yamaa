#![cfg(feature = "yaml")]
//! A driver field that a window partitions, orders or reads by is a scalar
//! source of each row. Under grouped construction it must be a group key: a
//! grouped row template's window fails under ROW-5, and a column-level
//! window under NAME-4, before any data is read. The declarations decide
//! it, so `check` reports the same refusal.
use crate::support::Edit as _;
use std::fs;
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::domain_entry::Request;

struct Fixture(crate::support::Scratch);
impl Fixture {
    fn new(specification: &str) -> Self {
        let root = crate::support::Scratch::new("window-group-sources");
        fs::write(root.join("spec.yaml"), specification).unwrap();
        fs::write(root.join("source.csv"), "ID,VAL\n1,3\n1,1\n2,5\n").unwrap();
        Self(root)
    }
    fn run(&self) -> file_application::Domain {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        file_application::domain(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            identity(specification),
        )
        .unwrap()
    }
    /// The issues `check` reports, without reading the source.
    fn check(&self) -> Vec<yamaa_core::json::Value> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        file_application::check(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            identity(specification),
        )
        .iter()
        .map(issue)
        .collect()
    }
}

fn identity(specification: &str) -> Identity<'_> {
    crate::support::study::identity("window-group-sources", specification)
}

fn issue(issue: &Diagnostic) -> yamaa_core::json::Value {
    crate::support::study::issue_row(issue)
}

const HEAD: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID]\n\
    input: {SRC: {path: source.csv, types: {ID: int, VAL: int}}}\n\
    output: {path: out.csv, columns: [ID, LOW, SEQ]}\n";

#[test]
fn grouped_windows_read_only_group_keys_of_the_driver() {
    // In the grouped template's own window, and at column level where every
    // grouped template must group by the field.
    let row = format!(
        "{HEAD}columns:\n  - {{name: ID, type: int}}\n  - {{name: LOW, type: int}}\n\
         \x20 - {{name: SEQ, type: int}}\n\
         rows:\n  - id: grouped\n    group_by: [SRC.ID]\n    derivations:\n      ID: SRC.ID\n\
         \x20     LOW: {{aggregate: 'MIN(SRC.VAL)'}}\n\
         \x20     SEQ: {{row_number: {{window: {{order_by: [ORDER]}}}}}}\n"
    );
    let column = format!(
        "{HEAD}columns:\n  - {{name: ID, type: int}}\n  - {{name: LOW, type: int}}\n\
         \x20 - name: SEQ\n    type: int\n\
         \x20   derivation: {{row_number: {{window: {{order_by: [ORDER]}}}}}}\n\
         rows:\n  - id: grouped\n    group_by: [SRC.ID]\n    derivations:\n      ID: SRC.ID\n\
         \x20     LOW: {{aggregate: 'MIN(SRC.VAL)'}}\n"
    );
    for (specification, path, rule, row) in [
        (
            &row,
            "rows.grouped.derivations.SEQ.row_number.window.order_by[0]",
            "ROW-5",
            Some("grouped"),
        ),
        (
            &column,
            "columns.SEQ.derivation.row_number.window.order_by[0]",
            "NAME-4",
            None,
        ),
    ] {
        // A field outside the groups is refused, by `check` as by `domain`; a
        // group key and a value of the grouped row both build.
        let fixture = Fixture::new(&specification.edit("ORDER", "SRC.VAL"));
        let refused = |identifier: &str, path: &str| {
            let mut context = yamaa_adapters::json!({"identifier": identifier, "dataset": "SRC"});
            if let Some(row) = row {
                context["row"] = yamaa_adapters::json!(row);
            }
            yamaa_adapters::json!({
                "phase": "validation",
                "condition": "ungrouped_driver_field",
                "rule": rule,
                "spec_paths": [path],
                "context": context
            })
        };
        let mut domain = fixture.run();
        assert_eq!(
            domain.issues().iter().map(issue).collect::<Vec<_>>(),
            vec![refused("SRC.VAL", path)]
        );
        assert!(domain.save().is_err());
        assert_eq!(fixture.check(), vec![refused("SRC.VAL", path)]);
        for order in ["SRC.ID", "LOW"] {
            let fixture = Fixture::new(&specification.edit("ORDER", order));
            assert!(fixture.check().is_empty(), "{order}: {:?}", fixture.check());
            let mut domain = fixture.run();
            assert!(domain.issues().is_empty(), "{order}: {:?}", domain.issues());
            assert!(domain.save().unwrap(), "{order}");
            // Both orders rank subject 1 (lowest value 1) before subject 2.
            assert_eq!(
                fs::read_to_string(fixture.0.join("out.csv")).unwrap(),
                "ID,LOW,SEQ\n1,1,1\n2,5,2\n",
                "{order}"
            );
        }
        // A refused group read does not hide another read the binder refuses:
        // the unknown field is reported at its own entry in the same pass.
        // Whether the source stores a field is known only once it is read, so
        // to `check` both fields are driver fields outside the groups.
        let fixture = Fixture::new(&specification.edit("ORDER", "SRC.VAL, SRC.MISSING"));
        let domain = fixture.run();
        let reported = domain
            .issues()
            .iter()
            .map(|issue| (issue.condition().to_string(), issue.spec_paths.clone()))
            .collect::<Vec<_>>();
        let missing = path.edit("order_by[0]", "order_by[1]");
        assert!(
            reported.contains(&("ungrouped_driver_field".into(), vec![path.into()])),
            "{reported:?}"
        );
        assert!(
            reported.contains(&("unknown_field".into(), vec![missing.clone()])),
            "{reported:?}"
        );
        assert_eq!(
            fixture.check(),
            vec![refused("SRC.VAL", path), refused("SRC.MISSING", &missing)]
        );
        // So a field the source does not store at all is outside the groups to
        // `check`, and unknown to `domain`.
        let fixture = Fixture::new(&specification.edit("ORDER", "SRC.MISSING"));
        assert_eq!(fixture.check(), vec![refused("SRC.MISSING", path)]);
        assert_eq!(
            fixture.run().issues().iter().map(issue).collect::<Vec<_>>(),
            vec![yamaa_adapters::json!({
                "phase": "validation",
                "condition": "unknown_field",
                "rule": "NAME-3",
                "spec_paths": [path],
                "context": {"identifier": "SRC.MISSING"}
            })]
        );
    }
}
