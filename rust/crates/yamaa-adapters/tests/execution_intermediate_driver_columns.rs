#![cfg(feature = "yaml")]
//! A column-level derivation completed only for rows a named intermediate
//! drives (ROW-2). The column binds against that driver the way its row
//! template does: the driver qualifier reads the record that built the row,
//! another input is an implicit join on the key it declares (LOOKUP-1), and a
//! named lookup selects through its key, while a qualified aggregate still
//! reduces its declared input (LOOKUP-18). The column phase reads each row
//! through its own template's driver port (#2829), so an intermediate over
//! any input is addressable; a field it derives, or a driver read beside an
//! aggregate, is refused before any data is read, by `check` and `domain`
//! alike.
use crate::support::Edit as _;
use std::fs;
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::domain_entry::Request;

struct Fixture(crate::support::Scratch);
impl Fixture {
    fn new(specification: &str) -> Self {
        let root = crate::support::Scratch::new("intermediate-driver-columns");
        fs::write(root.join("spec.yaml"), specification).unwrap();
        fs::write(root.join("ae.csv"), AE).unwrap();
        fs::write(root.join("dm.csv"), DM).unwrap();
        Self(root)
    }
    fn specification(&self) -> String {
        self.0.join("spec.yaml").to_str().unwrap().to_owned()
    }
    fn identity(specification: &str) -> Identity<'_> {
        crate::support::study::identity("intermediate-driver-columns", specification)
    }
    fn domain(&self) -> Result<file_application::Domain, Vec<Diagnostic>> {
        let specification = self.specification();
        file_application::domain(
            yamaa_core::project_function::Language::Python,
            Request {
                specification: &specification,
                environment: None,
            },
            Self::identity(&specification),
        )
    }
    fn check(&self) -> Vec<Diagnostic> {
        let specification = self.specification();
        file_application::check(
            yamaa_core::project_function::Language::Python,
            Request {
                specification: &specification,
                environment: None,
            },
            Self::identity(&specification),
        )
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
    /// The one issue `domain` reports, whether preparation or the build fails.
    fn issue(&self) -> Diagnostic {
        let issues = match self.domain() {
            Ok(domain) => domain.issues(),
            Err(issues) => issues,
        };
        let [issue] =
            <[Diagnostic; 1]>::try_from(issues).unwrap_or_else(|issues| panic!("{issues:?}"));
        issue
    }
    /// `check` agrees with `domain`, which refuses as an unsupported operation
    /// at `path` before any data is read.
    fn refused(&self, path: &str) {
        let issue = self.issue();
        assert_eq!(self.check(), std::slice::from_ref(&issue));
        assert_eq!(
            (issue.phase(), issue.condition()),
            ("planning", "unsupported_operation")
        );
        assert_eq!(issue.spec_paths, [path]);
    }
}

// Three serious events build rows; S1's two serious events form one group
// when a template groups by subject. The non-serious event builds no row.
const AE: &str = "USUBJID,AESEQ,AESER,AETERM\n\
    S1,1,Y,HEADACHE\n\
    S1,2,N,NAUSEA\n\
    S2,1,Y,RASH\n\
    S1,3,Y,DIZZINESS\n";
const DM: &str = "USUBJID,SEX\nS1,F\nS2,M\n";
const TERMS: &str = "USUBJID,AESEQ,R\nS1,1,HEADACHE\nS2,1,RASH\nS1,3,DIZZINESS\n";

/// One record-driven template over the serious events derives the keys; the
/// column-level derivation of `R` completes every row after construction. AE
/// and DM declare the keys their implicit joins match on, and SUBJ writes its
/// key as a mapping, which states a match of its own beside DM's (LOOKUP-4).
fn records(head: &str, kind: &str, derivation: &str) -> String {
    format!(
        "yamaa: \"1.0\"\n\
         type: spec\n\
         domain: XX\n\
         keys: [USUBJID, AESEQ]\n\
         {head}\
         output: {{path: out.csv, columns: [USUBJID, AESEQ, R]}}\n\
         input:\n\
         \x20 AE: {{path: ae.csv, key: [USUBJID, AESEQ], types: {{AESEQ: int}}}}\n\
         \x20 DM: {{path: dm.csv, key: [USUBJID]}}\n\
         intermediates:\n\
         \x20 - {{id: SER, dataset: AE, filter: \"AE.AESER = 'Y'\"}}\n\
         \x20 - {{id: SUBJ, dataset: DM, key: {{USUBJID: USUBJID}}}}\n\
         rows:\n\
         \x20 - id: serious\n\
         \x20   dataset: SER\n\
         \x20   derivations: {{USUBJID: SER.USUBJID, AESEQ: SER.AESEQ}}\n\
         columns:\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: AESEQ, type: int}}\n\
         \x20 - {{name: R, type: {kind}, derivation: {derivation}}}\n"
    )
}

/// The serious events with one more declaration beside their filter.
fn serious(declaration: &str, kind: &str, derivation: &str) -> String {
    records("", kind, derivation).edit(
        "filter: \"AE.AESER = 'Y'\"}",
        &format!("filter: \"AE.AESER = 'Y'\", {declaration}}}"),
    )
}

#[test]
fn literal_completes_every_intermediate_driven_row() {
    Fixture::new(&records("", "str", "\"'X'\"")).saved("USUBJID,AESEQ,R\nS1,1,X\nS2,1,X\nS1,3,X\n");
}

#[test]
fn driver_qualifier_reads_the_record_that_built_the_row() {
    Fixture::new(&records("", "str", "SER.AETERM")).saved(TERMS);
}

#[test]
fn underlying_input_is_an_implicit_join_on_the_output_keys() {
    // AE declares both output keys, so each row matches its own event.
    Fixture::new(&records("", "str", "AE.AETERM")).saved(TERMS);
}

#[test]
fn another_input_joins_on_the_keys_it_carries() {
    // DM declares USUBJID only, so each row matches its subject.
    Fixture::new(&records("", "str", "DM.SEX")).saved("USUBJID,AESEQ,R\nS1,1,F\nS2,1,M\nS1,3,F\n");
}

#[test]
fn case_reads_the_driver_record_in_conditions_and_branches() {
    Fixture::new(&records(
        "",
        "str",
        "{case: [{when: \"SER.AETERM = 'RASH'\", then: \"'SKIN'\"}, {otherwise: SER.AETERM}]}",
    ))
    .saved("USUBJID,AESEQ,R\nS1,1,HEADACHE\nS2,1,SKIN\nS1,3,DIZZINESS\n");
}

#[test]
fn keyed_lookup_selects_through_its_key() {
    Fixture::new(&records("", "str", "SUBJ.SEX"))
        .saved("USUBJID,AESEQ,R\nS1,1,F\nS2,1,M\nS1,3,F\n");
}

#[test]
fn declared_base_keeps_the_intermediate_driver() {
    Fixture::new(&records("base: AE\n", "str", "\"'X'\""))
        .saved("USUBJID,AESEQ,R\nS1,1,X\nS2,1,X\nS1,3,X\n");
}

#[test]
fn window_orders_the_driver_records() {
    Fixture::new(&records(
        "",
        "int",
        "{row_number: {window: {group_by: [SER.USUBJID], order_by: [SER.AESEQ]}}}",
    ))
    .saved("USUBJID,AESEQ,R\nS1,1,1\nS2,1,1\nS1,3,2\n");
}

#[test]
fn columns_list_limits_the_driver_fields() {
    let issue = Fixture::new(&serious("columns: [USUBJID, AESEQ]", "str", "SER.AETERM")).issue();
    assert_eq!(
        (issue.condition(), issue.spec_paths.as_slice()),
        (
            "unknown_field",
            ["columns.R.derivation.source".to_owned()].as_slice()
        )
    );
}

/// One template over the serious events grouped by subject derives the key.
fn grouped(derivation: &str) -> String {
    format!(
        "yamaa: \"1.0\"\n\
         type: spec\n\
         domain: XX\n\
         keys: [USUBJID]\n\
         output: {{path: out.csv, columns: [USUBJID, R]}}\n\
         input:\n\
         \x20 AE: {{path: ae.csv, types: {{AESEQ: int}}}}\n\
         intermediates:\n\
         \x20 - {{id: SER, dataset: AE, filter: \"AE.AESER = 'Y'\"}}\n\
         rows:\n\
         \x20 - id: subjects\n\
         \x20   dataset: SER\n\
         \x20   group_by: [SER.USUBJID]\n\
         \x20   derivations: {{USUBJID: SER.USUBJID}}\n\
         columns:\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: R, type: str, derivation: {derivation}}}\n"
    )
}

#[test]
fn grouped_intermediate_driver_reads_its_group_key() {
    // S1's two serious events are one group; its key is a single value
    // (NAME-4), never a lookup that could match both events.
    Fixture::new(&grouped("\"'X'\"")).saved("USUBJID,R\nS1,X\nS2,X\n");
    Fixture::new(&grouped("SER.USUBJID")).saved("USUBJID,R\nS1,S1\nS2,S2\n");
}

#[test]
fn grouped_driver_field_outside_the_groups_fails_validation() {
    let issue = Fixture::new(&grouped("SER.AETERM")).issue();
    assert_eq!(
        (issue.condition(), issue.rule(), issue.spec_paths.as_slice()),
        (
            "ungrouped_driver_field",
            Some("NAME-4"),
            ["columns.R.derivation.source".to_owned()].as_slice()
        )
    );
}

/// An input-driven template derives `R` itself; only the intermediate-driven
/// template completes it from the column-level derivation.
fn mixed(derivation: &str) -> String {
    format!(
        "yamaa: \"1.0\"\n\
         type: spec\n\
         domain: XX\n\
         keys: [USUBJID, AESEQ, SRC]\n\
         output: {{path: out.csv, columns: [USUBJID, AESEQ, SRC, R]}}\n\
         input:\n\
         \x20 AE: {{path: ae.csv, types: {{AESEQ: int}}}}\n\
         intermediates:\n\
         \x20 - {{id: SER, dataset: AE, filter: \"AE.AESER = 'Y'\"}}\n\
         rows:\n\
         \x20 - id: all\n\
         \x20   dataset: AE\n\
         \x20   derivations: {{USUBJID: AE.USUBJID, AESEQ: AE.AESEQ, SRC: \"'ALL'\", R: \"'OWN'\"}}\n\
         \x20 - id: serious\n\
         \x20   dataset: SER\n\
         \x20   derivations: {{USUBJID: SER.USUBJID, AESEQ: SER.AESEQ, SRC: \"'SER'\"}}\n\
         columns:\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: AESEQ, type: int}}\n\
         \x20 - {{name: SRC, type: str}}\n\
         \x20 - {{name: R, type: str, derivation: {derivation}}}\n"
    )
}

#[test]
fn mixed_specification_completes_only_the_intermediate_rows() {
    let all = "S1,1,ALL,OWN\nS1,2,ALL,OWN\nS2,1,ALL,OWN\nS1,3,ALL,OWN\n";
    Fixture::new(&mixed("\"'X'\"")).saved(&format!(
        "USUBJID,AESEQ,SRC,R\n{all}S1,1,SER,X\nS2,1,SER,X\nS1,3,SER,X\n"
    ));
    Fixture::new(&mixed("SER.AETERM")).saved(&format!(
        "USUBJID,AESEQ,SRC,R\n{all}S1,1,SER,HEADACHE\nS2,1,SER,RASH\nS1,3,SER,DIZZINESS\n"
    ));
}

/// An input-driven template beside a template grouping the serious events by
/// subject. The template named by `own` derives `R` itself, so only the
/// other completes it from the column-level derivation.
fn grouped_mixed(derivation: &str, own: &str) -> String {
    let (all, subjects) = match own {
        "all" => (", R: \"'OWN'\"", ""),
        _ => ("", ", R: \"'OWN'\""),
    };
    format!(
        "yamaa: \"1.0\"\n\
         type: spec\n\
         domain: XX\n\
         keys: [USUBJID, AESEQ, SRC]\n\
         output: {{path: out.csv, columns: [USUBJID, AESEQ, SRC, R]}}\n\
         input:\n\
         \x20 AE: {{path: ae.csv, types: {{AESEQ: int}}}}\n\
         intermediates:\n\
         \x20 - {{id: SER, dataset: AE, filter: \"AE.AESER = 'Y'\"}}\n\
         rows:\n\
         \x20 - id: all\n\
         \x20   dataset: AE\n\
         \x20   derivations: {{USUBJID: AE.USUBJID, AESEQ: AE.AESEQ, SRC: \"'ALL'\"{all}}}\n\
         \x20 - id: subjects\n\
         \x20   dataset: SER\n\
         \x20   group_by: [SER.USUBJID]\n\
         \x20   derivations: {{USUBJID: SER.USUBJID, AESEQ: {{aggregate: 'MIN(SER.AESEQ)'}}, SRC: \"'SER'\"{subjects}}}\n\
         columns:\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: AESEQ, type: int}}\n\
         \x20 - {{name: SRC, type: str}}\n\
         \x20 - {{name: R, type: str, derivation: {derivation}}}\n"
    )
}

#[test]
fn input_rows_read_their_own_record_beside_a_grouped_intermediate() {
    // Only the input-driven template completes R. The intermediate's groups
    // are not groups of AE, so each AE row reads its own event.
    Fixture::new(&grouped_mixed("AE.AETERM", "subjects")).saved(
        "USUBJID,AESEQ,SRC,R\nS1,1,ALL,HEADACHE\nS1,2,ALL,NAUSEA\nS2,1,ALL,RASH\n\
         S1,3,ALL,DIZZINESS\nS1,1,SER,OWN\nS2,1,SER,OWN\n",
    );
}

#[test]
fn grouped_intermediate_rows_read_only_its_group_keys() {
    // Only the grouped template completes R, and S1's group holds two
    // serious events.
    let issue = Fixture::new(&grouped_mixed("SER.AETERM", "all")).issue();
    assert_eq!(
        (issue.condition(), issue.rule(), issue.spec_paths.as_slice()),
        (
            "ungrouped_driver_field",
            Some("NAME-4"),
            ["columns.R.derivation.source".to_owned()].as_slice()
        )
    );
    assert_eq!(
        yamaa_core::json::Value::Object(issue.context.clone()),
        yamaa_adapters::json!({"identifier": "SER.AETERM", "dataset": "SER"})
    );
}

const LOWER: &str = "derivations: {LOWER: {str_case: {source: AE.AETERM, to: lower}}}";

#[test]
fn stored_field_of_a_deriving_driver_is_read_from_its_record() {
    Fixture::new(&serious(LOWER, "str", "SER.AETERM")).saved(TERMS);
}

#[test]
fn derived_driver_field_is_refused_before_reading_data() {
    Fixture::new(&serious(LOWER, "str", "SER.LOWER")).refused("columns.R.derivation.source");
    Fixture::new(&serious(
        LOWER,
        "int",
        "{row_number: {window: {group_by: [SER.USUBJID], order_by: [SER.LOWER]}}}",
    ))
    .refused("columns.R.derivation.row_number");
}

#[test]
fn derived_field_read_in_the_template_feeds_a_column() {
    // The template reads the derived field into its own column, which the
    // column-level derivation then reads.
    Fixture::new(
        &serious(
            LOWER,
            "str",
            "{case: [{when: \"TERM = 'rash'\", then: \"'SKIN'\"}, {otherwise: TERM}]}",
        )
        .edit(
            "derivations: {USUBJID: SER.USUBJID, AESEQ: SER.AESEQ}",
            "derivations: {USUBJID: SER.USUBJID, AESEQ: SER.AESEQ, TERM: SER.LOWER}",
        )
        .edit(
            "\x20 - {name: R,",
            "\x20 - {name: TERM, type: str}\n\x20 - {name: R,",
        ),
    )
    .saved("USUBJID,AESEQ,R\nS1,1,headache\nS2,1,SKIN\nS1,3,dizziness\n");
}

#[test]
fn derived_field_in_an_imputation_bound_is_refused() {
    let fixture = Fixture::new(
        &records(
            "",
            "date",
            "{impute: {type: date, source: SER.AEDTC, day: 15, not_before: SER.FLOOR}}",
        )
        .edit("types: {AESEQ: int}", "types: {AESEQ: int, AESTDT: date}")
        .edit(
            "filter: \"AE.AESER = 'Y'\"}",
            "filter: \"AE.AESER = 'Y'\", derivations: {FLOOR: AE.AESTDT}}",
        ),
    );
    fs::write(
        fixture.0.join("ae.csv"),
        "USUBJID,AESEQ,AESER,AEDTC,AESTDT\nS1,1,Y,2024-03,2024-03-20\n",
    )
    .unwrap();
    fixture.refused("columns.R.derivation.impute");
}

/// A count of every collected event of the row's subject.
const COUNT: &str = "{aggregate: {expr: \"COUNT(AE.AESEQ)\", key: [USUBJID]}}";

#[test]
fn aggregate_reduces_the_declared_input() {
    // The reduction reads AE itself, so S1's non-serious event counts too.
    Fixture::new(&records("", "int", COUNT)).saved("USUBJID,AESEQ,R\nS1,1,3\nS2,1,1\nS1,3,3\n");
}

#[test]
fn aggregate_never_reduces_the_intermediate() {
    // An aggregate reduces a declared input; the intermediate is none.
    let issue = Fixture::new(&records(
        "",
        "int",
        "{aggregate: {expr: \"COUNT(SER.AESEQ)\", key: [USUBJID]}}",
    ))
    .issue();
    assert_eq!(
        (issue.condition(), issue.spec_paths.as_slice()),
        (
            "unknown_field",
            ["columns.R.derivation.aggregate.expr".to_owned()].as_slice()
        )
    );
}

#[test]
fn driver_read_beside_an_aggregate_is_refused() {
    Fixture::new(&records(
        "",
        "int",
        &format!("{{case: [{{when: \"SER.AETERM = 'RASH'\", then: {COUNT}}}, {{otherwise: 0}}]}}"),
    ))
    .refused("columns.R.derivation.case");
}

/// A subject-driven template comes first, so the serious events are read
/// from an input other than the one that drives the first template.
fn secondary(derivation: &str) -> String {
    format!(
        "yamaa: \"1.0\"\n\
         type: spec\n\
         domain: XX\n\
         keys: [USUBJID, AESEQ, SRC]\n\
         output: {{path: out.csv, columns: [USUBJID, AESEQ, SRC, R]}}\n\
         input:\n\
         \x20 DM: dm.csv\n\
         \x20 AE: {{path: ae.csv, key: [USUBJID, AESEQ], types: {{AESEQ: int}}}}\n\
         intermediates:\n\
         \x20 - {{id: SER, dataset: AE, filter: \"AE.AESER = 'Y'\"}}\n\
         rows:\n\
         \x20 - id: subjects\n\
         \x20   dataset: DM\n\
         \x20   derivations: {{USUBJID: DM.USUBJID, AESEQ: 0, SRC: \"'DM'\", R: \"'OWN'\"}}\n\
         \x20 - id: serious\n\
         \x20   dataset: SER\n\
         \x20   derivations: {{USUBJID: SER.USUBJID, AESEQ: SER.AESEQ, SRC: \"'SER'\"}}\n\
         columns:\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: AESEQ, type: int}}\n\
         \x20 - {{name: SRC, type: str}}\n\
         \x20 - {{name: R, type: str, derivation: {derivation}}}\n"
    )
}

#[test]
fn driver_over_another_input_completes_reads_it_can_address() {
    let subjects = "S1,0,DM,OWN\nS2,0,DM,OWN\n";
    Fixture::new(&secondary("\"'X'\"")).saved(&format!(
        "USUBJID,AESEQ,SRC,R\n{subjects}S1,1,SER,X\nS2,1,SER,X\nS1,3,SER,X\n"
    ));
    Fixture::new(&secondary("AE.AETERM")).saved(&format!(
        "USUBJID,AESEQ,SRC,R\n{subjects}S1,1,SER,HEADACHE\nS2,1,SER,RASH\nS1,3,SER,DIZZINESS\n"
    ));
}

#[test]
fn driver_over_another_input_reads_the_record_that_built_the_row() {
    // The column phase reads each row through its own template's driver
    // port (#2829): the serious event in AE, not the DM record at the same
    // position.
    Fixture::new(&secondary("SER.AETERM")).saved(
        "USUBJID,AESEQ,SRC,R\nS1,0,DM,OWN\nS2,0,DM,OWN\n\
         S1,1,SER,HEADACHE\nS2,1,SER,RASH\nS1,3,SER,DIZZINESS\n",
    );
}

#[test]
fn failed_driver_intermediate_reports_only_its_own_finding() {
    // The intermediate's filter names a field AE lacks. Its template is
    // skipped, and so is the column completed for that template's rows.
    let issue =
        Fixture::new(&records("", "str", "\"'X'\"").edit("AE.AESER = 'Y'", "AE.SERIOUS = 'Y'"))
            .issue();
    assert_eq!(
        (issue.condition(), issue.spec_paths.as_slice()),
        (
            "unknown_field",
            ["intermediates.SER.filter".to_owned()].as_slice()
        )
    );
}
