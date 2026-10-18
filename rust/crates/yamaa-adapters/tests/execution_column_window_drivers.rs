#![cfg(feature = "yaml")]
//! A column-level window spans the rows of every row template and reads a
//! qualified field at the record that built each row (WINDOW-11). When the
//! row templates do not all share one driver, a read of a field of one of
//! their drivers, or of the input a driver intermediate reads, names no
//! record of the other templates' rows. `check` and `domain` both refuse it
//! as `unknown_field` before any data is read, naming the field, the drivers
//! and the window operation; it used to build, ordering those rows by the
//! records of another dataset at the same positions. The same window over
//! row templates that share the driver builds unchanged.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
use yamaa_core::{domain_entry::Request, json::Value};

struct Fixture(PathBuf);
impl Fixture {
    fn new(specification: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "yamaa-column-window-drivers-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("spec.yaml"), specification).unwrap();
        fs::write(root.join("dm.csv"), DM).unwrap();
        fs::write(root.join("ae.csv"), AE).unwrap();
        fs::write(root.join("lb.csv"), LB).unwrap();
        Self(root)
    }
    fn specification(&self) -> String {
        self.0.join("spec.yaml").to_str().unwrap().to_owned()
    }
    fn identity(specification: &str) -> Identity<'_> {
        Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "column-window-drivers",
            specification,
            base_directory: ".",
        }
    }
    fn request(specification: &str) -> Request<'_> {
        Request {
            specification,
            environment: None,
        }
    }
    fn domain(&self) -> Result<file_application::Domain, Vec<Issue>> {
        let specification = self.specification();
        file_application::domain(
            yamaa_core::project_function::Language::Python,
            Self::request(&specification),
            Self::identity(&specification),
        )
    }
    fn check(&self) -> Vec<Issue> {
        let specification = self.specification();
        file_application::check(
            yamaa_core::project_function::Language::Python,
            Self::request(&specification),
            Self::identity(&specification),
        )
    }
    /// The issues `domain` reports, whether preparation or the build fails.
    fn issues(&self) -> Vec<Value> {
        let issues = match self.domain() {
            Ok(domain) => domain.issues(),
            Err(issues) => issues,
        };
        issues.iter().map(row).collect()
    }
    /// `check` and `domain` report exactly these WINDOW-11 refusals, each as
    /// (operand path, field, drivers, window operation).
    fn refused(&self, expected: &[(&str, &str, &[&str], &str)]) {
        let expected = expected
            .iter()
            .map(|&(path, identifier, drivers, operation)| {
                yamaa_adapters::json!({
                    "phase": "validation",
                    "condition": "unknown_field",
                    "requirement": "WINDOW-11",
                    "spec_paths": [path],
                    "context": {
                        "identifier": identifier,
                        "drivers": drivers,
                        "operation": operation
                    }
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(self.issues(), expected);
        assert_eq!(self.check().iter().map(row).collect::<Vec<_>>(), expected);
    }
    /// `check` finds nothing and `domain` saves exactly `expected`.
    fn saved(&self, expected: &str) {
        let check = self.check();
        assert!(check.is_empty(), "{check:?}");
        let mut domain = self.domain().unwrap_or_else(|issues| panic!("{issues:?}"));
        assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        assert!(domain.save().unwrap());
        assert_eq!(
            fs::read_to_string(self.0.join("out.csv")).unwrap(),
            expected
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn row(issue: &Issue) -> Value {
    yamaa_adapters::json!({
        "phase": issue.phase,
        "condition": issue.condition,
        "requirement": issue.requirement,
        "spec_paths": issue.spec_paths,
        "context": issue.context.clone()
    })
}

// Three subjects of different ages. S1 has three adverse events, two of them
// serious; S2 has one serious event.
const DM: &str = "USUBJID,AGE,RFSTDTC\nS1,40,2024-01-05\nS2,30,2024-01-07\nS3,50,2024-01-09\n";
const AE: &str = "USUBJID,AESEQ,AESER,AESTDTC\n\
    S1,1,Y,2024-01-03\n\
    S1,2,N,2024-01-08\n\
    S2,1,Y,2024-01-06\n\
    S1,3,Y,2024-01-10\n";
// Read by no row template.
const LB: &str = "USUBJID,LBSEQ\nS1,1\n";

const SUBJECTS: &str = "  - {id: subjects, dataset: DM, derivations: {USUBJID: DM.USUBJID, SRC: {literal: DM}, SEQ: {literal: 0}}}\n";
const EVENTS: &str = "  - {id: events, dataset: AE, derivations: {USUBJID: AE.USUBJID, SRC: {literal: AE}, SEQ: AE.AESEQ}}\n";
const SERIOUS: &str = "  - {id: serious, dataset: SER, derivations: {USUBJID: SER.USUBJID, SRC: {literal: SER}, SEQ: SER.AESEQ}}\n";
/// Two named intermediates select AE's serious and non-serious events.
const INTERMEDIATES: &str = "intermediates:\n\
    \x20 - {id: SER, dataset: AE, filter: \"AE.AESER = 'Y'\"}\n\
    \x20 - {id: NONSER, dataset: AE, filter: \"AE.AESER = 'N'\"}\n";

/// Rows built by `rows` in order, and the column-level derivation of `ORD`
/// after `extra` declared columns.
fn specification(prefix: &str, rows: &[&str], extra: &str, kind: &str, derivation: &str) -> String {
    format!(
        "schema_version: \"1.0\"\n\
         domain: XX\n\
         keys: [USUBJID, SRC, SEQ]\n\
         output: {{path: out.csv, columns: [USUBJID, SRC, SEQ, ORD]}}\n\
         input:\n\
         \x20 DM: {{path: dm.csv, types: {{AGE: int, RFSTDTC: date}}}}\n\
         \x20 AE: {{path: ae.csv, types: {{AESEQ: int, AESTDTC: date}}}}\n\
         \x20 LB: {{path: lb.csv, types: {{LBSEQ: int}}}}\n\
         {prefix}\
         rows:\n{rows}\
         columns:\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: SRC, type: str}}\n\
         \x20 - {{name: SEQ, type: int}}\n\
         {extra}\
         \x20 - {{name: ORD, type: {kind}, derivation: {derivation}}}\n",
        rows = rows.concat()
    )
}

/// `row_number` ordered by `order` over `rows`.
fn numbered(prefix: &str, rows: &[&str], order: &str) -> Fixture {
    Fixture::new(&specification(
        prefix,
        rows,
        "",
        "int",
        &format!("{{row_number: {{window: {{order_by: [{order}]}}}}}}"),
    ))
}

const ORDER: &str = "columns.ORD.derivation.row_number.window.order_by[0]";

#[test]
fn a_window_reading_a_driver_another_template_lacks_is_refused() {
    // The events rows used to take the ages of the subjects at their own
    // record positions, so S1's second event was ordered by S2's age.
    numbered("", &[SUBJECTS, EVENTS], "DM.AGE").refused(&[(
        ORDER,
        "DM.AGE",
        &["DM", "AE"],
        "row_number",
    )]);
    // A field of the later template's driver, which binding used to refuse
    // as a field of no dataset (REQ-0103), is refused the same way.
    numbered("", &[SUBJECTS, EVENTS], "AE.AESEQ").refused(&[(
        ORDER,
        "AE.AESEQ",
        &["DM", "AE"],
        "row_number",
    )]);
    // Each driver is named once, in construction order.
    numbered(
        "",
        &[EVENTS, SUBJECTS, &EVENTS.replace("id: events", "id: again")],
        "AE.AESEQ",
    )
    .refused(&[(ORDER, "AE.AESEQ", &["AE", "DM"], "row_number")]);
}

#[test]
fn every_operand_of_every_window_operation_is_checked() {
    let drivers: &[&str] = &["DM", "AE"];
    let column = "columns.ORD.derivation";
    for (kind, derivation, prefix, expected) in [
        (
            "int",
            "{rank: {window: {order_by: [DM.AGE]}}}",
            "",
            vec![(format!("{column}.rank.window.order_by[0]"), "DM.AGE", "rank")],
        ),
        (
            "int",
            "{row_number: {window: {group_by: [DM.AGE], order_by: [SEQ]}}}",
            "",
            vec![(
                format!("{column}.row_number.window.group_by[0]"),
                "DM.AGE",
                "row_number",
            )],
        ),
        (
            "int",
            "{row_value: {source: DM.AGE, offset: -1, window: {order_by: [SEQ]}}}",
            "",
            vec![(format!("{column}.row_value.source"), "DM.AGE", "row_value")],
        ),
        (
            "int",
            "{locf: {source: DM.AGE, window: {order_by: [SEQ]}}}",
            "",
            vec![(format!("{column}.locf.source"), "DM.AGE", "locf")],
        ),
        (
            "str",
            "{baseline_flag: {date: DM.RFSTDTC, reference_date: DM.RFSTDTC, value: DM.AGE, tie_break: [DM.AGE]}}",
            "",
            vec![
                (format!("{column}.baseline_flag.date"), "DM.RFSTDTC", "baseline_flag"),
                (
                    format!("{column}.baseline_flag.reference_date"),
                    "DM.RFSTDTC",
                    "baseline_flag",
                ),
                (format!("{column}.baseline_flag.value"), "DM.AGE", "baseline_flag"),
                (format!("{column}.baseline_flag.tie_break[0]"), "DM.AGE", "baseline_flag"),
            ],
        ),
        (
            "int",
            "{row_number: {window: {order_by: [SEQ], filter: 'DM.AGE > 35'}}}",
            "",
            vec![(
                format!("{column}.row_number.window.filter"),
                "DM.AGE",
                "row_number",
            )],
        ),
        (
            "int",
            "{row_number: {window: {group_by: [DM.USUBJID], order_by: [DM.AGE, SEQ]}}}",
            "",
            vec![
                (
                    format!("{column}.row_number.window.group_by[0]"),
                    "DM.USUBJID",
                    "row_number",
                ),
                (
                    format!("{column}.row_number.window.order_by[0]"),
                    "DM.AGE",
                    "row_number",
                ),
            ],
        ),
        // A named window is checked where it is used.
        (
            "int",
            "{row_number: {window: BYAGE}}",
            "windows:\n  BYAGE: {order_by: [DM.AGE]}\n",
            vec![(ORDER.to_owned(), "DM.AGE", "row_number")],
        ),
        // So is a window in a case result, even one only events rows select.
        (
            "int",
            "{case: [{when: \"SRC = 'AE'\", then: {row_number: {window: {order_by: [DM.AGE]}}}}, {otherwise: {literal: 0}}]}",
            "",
            vec![(
                format!("{column}.case[0].then.row_number.window.order_by[0]"),
                "DM.AGE",
                "row_number",
            )],
        ),
    ] {
        let expected = expected
            .iter()
            .map(|(path, identifier, operation)| (path.as_str(), *identifier, drivers, *operation))
            .collect::<Vec<_>>();
        Fixture::new(&specification(
            prefix,
            &[SUBJECTS, EVENTS],
            "",
            kind,
            derivation,
        ))
        .refused(&expected);
    }
}

#[test]
fn grouping_either_template_refuses_the_read_the_same_way() {
    let grouped_events = "  - {id: events, dataset: AE, group_by: [AE.USUBJID], derivations: {USUBJID: AE.USUBJID, SRC: {literal: AE}, SEQ: {literal: 1}}}\n";
    let grouped_subjects = "  - {id: subjects, dataset: DM, group_by: [DM.USUBJID], derivations: {USUBJID: DM.USUBJID, SRC: {literal: DM}, SEQ: {literal: 0}}}\n";
    for rows in [[SUBJECTS, grouped_events], [grouped_subjects, EVENTS]] {
        numbered("", &rows, "DM.AGE").refused(&[(ORDER, "DM.AGE", &["DM", "AE"], "row_number")]);
    }
}

#[test]
fn named_intermediate_drivers_and_the_input_they_read() {
    let grouped = "  - {id: serious, dataset: SER, group_by: [SER.USUBJID], derivations: {USUBJID: SER.USUBJID, SRC: {literal: SER}, SEQ: {literal: 1}}}\n";
    let other = "  - {id: other, dataset: NONSER, derivations: {USUBJID: NONSER.USUBJID, SRC: {literal: NONSER}, SEQ: NONSER.AESEQ}}\n";
    for (rows, order, drivers) in [
        (vec![SUBJECTS, SERIOUS], "DM.AGE", &["DM", "SER"][..]),
        (vec![SUBJECTS, grouped], "DM.AGE", &["DM", "SER"][..]),
        (vec![SERIOUS, SUBJECTS], "SER.AESEQ", &["SER", "DM"][..]),
        // The intermediate's records are AE's, read at their AE positions;
        // a subject row has none.
        (vec![SERIOUS, SUBJECTS], "AE.AESEQ", &["SER", "DM"][..]),
        // A serious row is built from SER, not from AE, although it reads
        // AE's records.
        (vec![EVENTS, SERIOUS], "AE.AESEQ", &["AE", "SER"][..]),
        (vec![SERIOUS, other], "AE.AESEQ", &["SER", "NONSER"][..]),
    ] {
        numbered(INTERMEDIATES, &rows, order).refused(&[(ORDER, order, drivers, "row_number")]);
    }
}

#[test]
fn a_window_whose_templates_share_the_driver_builds_unchanged() {
    // Ties keep row-template order, then record order.
    let again = SUBJECTS
        .replace("id: subjects", "id: again")
        .replace("literal: DM}", "literal: DM2}");
    numbered("", &[SUBJECTS, &again], "DM.AGE").saved(
        "USUBJID,SRC,SEQ,ORD\nS1,DM,0,3\nS2,DM,0,1\nS3,DM,0,5\nS1,DM2,0,4\nS2,DM2,0,2\nS3,DM2,0,6\n",
    );
    // A grouped template over the shared driver whose groups hold the field.
    let grouped = "  - {id: grouped, dataset: DM, group_by: [DM.USUBJID, DM.AGE], derivations: {USUBJID: DM.USUBJID, SRC: {literal: DMG}, SEQ: {literal: 1}}}\n";
    numbered("", &[SUBJECTS, grouped], "DM.AGE").saved(
        "USUBJID,SRC,SEQ,ORD\nS1,DM,0,3\nS2,DM,0,1\nS3,DM,0,5\nS1,DMG,1,4\nS2,DMG,1,2\nS3,DMG,1,6\n",
    );
    // Two templates over one named intermediate: the serious events S1 1,
    // S2 1 and S1 3, latest sequence number first.
    let again = SERIOUS
        .replace("id: serious", "id: again")
        .replace("literal: SER}", "literal: SER2}");
    numbered(
        INTERMEDIATES,
        &[SERIOUS, &again],
        "{variable: SER.AESEQ, direction: desc}",
    )
    .saved(
        "USUBJID,SRC,SEQ,ORD\nS1,SER,1,3\nS2,SER,1,4\nS1,SER,3,1\nS1,SER2,1,5\nS2,SER2,1,6\nS1,SER2,3,2\n",
    );
}

#[test]
fn a_grouped_template_over_the_shared_driver_still_reads_only_its_group_keys() {
    let grouped = "  - {id: grouped, dataset: DM, group_by: [DM.USUBJID], derivations: {USUBJID: DM.USUBJID, SRC: {literal: DMG}, SEQ: {literal: 1}}}\n";
    assert_eq!(
        numbered("", &[SUBJECTS, grouped], "DM.AGE").issues(),
        vec![yamaa_adapters::json!({
            "phase": "validation",
            "condition": "ungrouped_driver_field",
            "requirement": "REQ-0107",
            "spec_paths": [ORDER],
            "context": {"identifier": "DM.AGE", "dataset": "DM"}
        })]
    );
}

#[test]
fn a_column_every_row_completes_in_its_own_scope_orders_every_row() {
    // The repair: each row reads its subject's age as a column, an events row
    // through the implicit join on USUBJID, and the window orders by it. S2's
    // event and record come first (30), then S1's events and record (40).
    Fixture::new(&specification(
        "",
        &[SUBJECTS, EVENTS],
        "  - {name: AGE, type: int, derivation: DM.AGE}\n",
        "int",
        "{row_number: {window: {order_by: [AGE, SRC, SEQ]}}}",
    ))
    .saved("USUBJID,SRC,SEQ,ORD\nS1,DM,0,6\nS2,DM,0,2\nS3,DM,0,7\nS1,AE,1,3\nS1,AE,2,4\nS2,AE,1,1\nS1,AE,3,5\n");
}

#[test]
fn a_read_of_an_input_no_row_is_built_from_keeps_its_binding_finding() {
    assert_eq!(
        numbered("", &[SUBJECTS, EVENTS], "LB.LBSEQ").issues(),
        vec![yamaa_adapters::json!({
            "phase": "validation",
            "condition": "unknown_field",
            "requirement": "REQ-0103",
            "spec_paths": [ORDER],
            "context": {"identifier": "LB.LBSEQ"}
        })]
    );
}
