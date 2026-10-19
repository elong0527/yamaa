#![cfg(feature = "yaml")]
//! A column-level derivation completes the rows of every row template that
//! leaves the column to the column phase, each row from the record that built
//! it (#2829). The column binds once for each driver those templates read,
//! against that driver's own record (REQ-0111): its qualifier reads the
//! record that built the row, and another input joins on the output keys it
//! carries. The bindings run one driver after another, in the order of the
//! first template each completes. A column holding a qualified aggregate
//! keeps the whole-dataset scope (REQ-0467).
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
use yamaa_core::domain_entry::Request;

struct Fixture(PathBuf);
impl Fixture {
    fn new(specification: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "yamaa-column-driver-bindings-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("spec.yaml"), specification).unwrap();
        fs::write(root.join("ae.csv"), AE).unwrap();
        fs::write(root.join("dm.csv"), DM).unwrap();
        fs::write(root.join("cm.csv"), CM).unwrap();
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
            example: "column-driver-bindings",
            specification,
            base_directory: ".",
        }
    }
    fn domain(&self) -> Result<file_application::Domain, Vec<Issue>> {
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
    fn check(&self) -> Vec<Issue> {
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
    fn issue(&self) -> Issue {
        let issues = match self.domain() {
            Ok(domain) => domain.issues(),
            Err(issues) => issues,
        };
        let [issue] = <[Issue; 1]>::try_from(issues).unwrap_or_else(|issues| panic!("{issues:?}"));
        issue
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

// S1 has two adverse events, S2 one, S3 none; every subject has one DM
// record. CODE is text: S2's adverse event and DM record carry a value that
// is not an integer.
const AE: &str = "USUBJID,AESEQ,AETERM,AESER,CODE\n\
    S1,1,HEADACHE,Y,7\n\
    S1,2,NAUSEA,N,8\n\
    S2,1,RASH,Y,bad-ae\n";
const DM: &str = "USUBJID,SEX,ARM,CODE\nS1,F,A,1\nS2,M,B,bad-dm\nS3,F,A,3\n";
const CM: &str = "USUBJID,CMSEQ,CMTRT\nS1,1,ASPIRIN\nS3,1,IBUPROFEN\n";

const HEAD: &str = "schema_version: \"1.0\"\n\
    domain: XX\n\
    input:\n\
    \x20 AE: {path: ae.csv, types: {AESEQ: int}}\n\
    \x20 DM: dm.csv\n\
    \x20 CM: {path: cm.csv, types: {CMSEQ: int}}\n\
    keys: [USUBJID, SRC, SEQ]\n\
    output: {path: out.csv, columns: [USUBJID, SRC, SEQ, X]}\n";

/// The declared columns, `X` of `kind` stated once by `derivation`.
fn columns(kind: &str, derivation: &str) -> String {
    format!(
        "columns:\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: SRC, type: str}}\n\
         \x20 - {{name: SEQ, type: int}}\n\
         \x20 - {{name: X, type: {kind}, derivation: {derivation}}}\n"
    )
}

/// One row template per driver; `own` names the templates that state `X`
/// themselves, so it is a mixed column.
fn templates(order: &[&str], own: &[&str]) -> String {
    let mut rows = String::from("rows:\n");
    for &template in order {
        let (driver, seq) = match template {
            "ae" => ("AE", "AE.AESEQ"),
            "cm" => ("CM", "CM.CMSEQ"),
            _ => ("DM", "{literal: 0}"),
        };
        let x = if own.contains(&template) {
            ", X: {literal: OWN}"
        } else {
            ""
        };
        rows.push_str(&format!(
            "\x20 - id: {template}\n\
             \x20   dataset: {driver}\n\
             \x20   derivations: {{USUBJID: {driver}.USUBJID, SRC: {{literal: {driver}}}, \
             SEQ: {seq}{x}}}\n"
        ));
    }
    rows
}

fn specification(kind: &str, derivation: &str, order: &[&str], own: &[&str]) -> String {
    format!(
        "{HEAD}{}{}",
        columns(kind, derivation),
        templates(order, own)
    )
}

const OWN_AE: &str = "S1,AE,1,OWN\nS1,AE,2,OWN\nS2,AE,1,OWN\n";

#[test]
fn second_template_driver_reads_its_own_record_through_a_bare_source() {
    // The first row template derives SEX itself; only the DM rows complete it
    // from the column, and they read their own record, not the first
    // template's input.
    Fixture::new(&specification("str", "DM.SEX", &["ae", "dm"], &["ae"])).saved(&format!(
        "USUBJID,SRC,SEQ,X\n{OWN_AE}S1,DM,0,F\nS2,DM,0,M\nS3,DM,0,F\n"
    ));
}

#[test]
fn second_template_driver_reads_its_own_record_inside_an_operation() {
    Fixture::new(&specification(
        "str",
        "{str_case: {source: DM.SEX, to: lower}}",
        &["ae", "dm"],
        &["ae"],
    ))
    .saved(&format!(
        "USUBJID,SRC,SEQ,X\n{OWN_AE}S1,DM,0,f\nS2,DM,0,m\nS3,DM,0,f\n"
    ));
}

#[test]
fn second_template_driver_reads_its_own_record_in_a_flag_condition() {
    Fixture::new(&specification(
        "str",
        "{flag: \"DM.SEX = 'F'\"}",
        &["ae", "dm"],
        &["ae"],
    ))
    .saved(&format!(
        "USUBJID,SRC,SEQ,X\n{OWN_AE}S1,DM,0,Y\nS2,DM,0,\nS3,DM,0,Y\n"
    ));
}

#[test]
fn each_driver_reads_its_own_record_and_joins_the_other() {
    // AE and DM both leave X to the column; CM states it. The AE rows join DM
    // on USUBJID, the one output key DM carries; the DM rows read their own
    // record.
    Fixture::new(&specification(
        "str",
        "DM.SEX",
        &["ae", "dm", "cm"],
        &["cm"],
    ))
    .saved(
        "USUBJID,SRC,SEQ,X\nS1,AE,1,F\nS1,AE,2,F\nS2,AE,1,M\nS1,DM,0,F\nS2,DM,0,M\n\
         S3,DM,0,F\nS1,CM,1,OWN\nS3,CM,1,OWN\n",
    );
}

#[test]
fn each_driver_reads_its_own_record_where_a_condition_selects_it() {
    // Every row reads the stored field only its own driver carries.
    Fixture::new(&specification(
        "str",
        "{case: [{when: \"SRC = 'AE'\", then: AE.AETERM}, {otherwise: DM.SEX}]}",
        &["ae", "dm", "cm"],
        &["cm"],
    ))
    .saved(
        "USUBJID,SRC,SEQ,X\nS1,AE,1,HEADACHE\nS1,AE,2,NAUSEA\nS2,AE,1,RASH\nS1,DM,0,F\n\
         S2,DM,0,M\nS3,DM,0,F\nS1,CM,1,OWN\nS3,CM,1,OWN\n",
    );
}

#[test]
fn another_drivers_records_still_join_on_the_output_keys() {
    // The DM rows join AE on USUBJID, and S1 has two adverse events: the
    // implicit join fails as it would in any other column.
    let fixture = Fixture::new(&specification(
        "str",
        "AE.AETERM",
        &["ae", "dm", "cm"],
        &["cm"],
    ));
    assert!(fixture.check().is_empty());
    let issue = fixture.issue();
    assert_eq!(
        (
            issue.phase.as_str(),
            issue.condition.as_str(),
            issue.spec_paths.as_slice()
        ),
        (
            "join",
            "multiple_matches",
            ["columns.X.derivation.source".to_owned()].as_slice()
        )
    );
}

#[test]
fn column_bindings_run_one_driver_after_another() {
    // The DM templates arm_a and arm_b share one binding, which runs before
    // the AE binding because arm_a comes first. S2's DM record fails in
    // arm_b before S2's adverse event is converted, although the event row
    // comes first.
    let fixture = Fixture::new(&format!(
        "{HEAD}{}rows:\n\
         \x20 - {{id: arm_a, dataset: DM, filter: \"DM.ARM = 'A'\", derivations: {{USUBJID: DM.USUBJID, SRC: {{literal: A}}, SEQ: {{literal: 0}}}}}}\n\
         \x20 - {{id: events, dataset: AE, derivations: {{USUBJID: AE.USUBJID, SRC: {{literal: AE}}, SEQ: AE.AESEQ}}}}\n\
         \x20 - {{id: arm_b, dataset: DM, filter: \"DM.ARM = 'B'\", derivations: {{USUBJID: DM.USUBJID, SRC: {{literal: B}}, SEQ: {{literal: 0}}}}}}\n",
        columns(
            "int",
            "{case: [{when: \"SRC = 'AE'\", then: AE.CODE}, {otherwise: DM.CODE}]}"
        ),
    ));
    let issue = fixture.issue();
    assert_eq!(
        (issue.phase.as_str(), issue.condition.as_str()),
        ("convert", "conversion_failed")
    );
    assert_eq!(
        issue.context,
        yamaa_adapters::json!({
            "from": "str",
            "keys": [{"SEQ": 0, "SRC": "B", "USUBJID": "S2"}],
            "to": "int",
            "value": "bad-dm",
        })
    );
}

#[test]
fn a_column_holding_an_aggregate_keeps_the_whole_dataset() {
    // A qualified aggregate reduces its declared input for every row; DM is
    // read through the join on USUBJID, for the DM rows as well.
    Fixture::new(&specification(
        "int",
        "{case: [{when: \"DM.SEX = 'F'\", then: {aggregate: {expr: \"COUNT(AE.AESEQ)\", key: [USUBJID]}}}, {otherwise: {literal: 0}}]}",
        &["ae", "dm", "cm"],
        &["cm"],
    )
    .replace("X: {literal: OWN}", "X: {literal: -1}"))
    .saved(
        "USUBJID,SRC,SEQ,X\nS1,AE,1,2\nS1,AE,2,2\nS2,AE,1,0\nS1,DM,0,2\nS2,DM,0,0\n\
         S3,DM,0,\nS1,CM,1,-1\nS3,CM,1,-1\n",
    );
}

#[test]
fn a_column_window_spans_the_rows_of_every_driver() {
    // No template states X: one window numbers the rows of all three drivers
    // within each subject, in construction order.
    Fixture::new(&specification(
        "int",
        "{row_number: {window: {group_by: [USUBJID], order_by: [SEQ]}}}",
        &["ae", "dm", "cm"],
        &[],
    ))
    .saved(
        "USUBJID,SRC,SEQ,X\nS1,AE,1,2\nS1,AE,2,4\nS2,AE,1,2\nS1,DM,0,1\nS2,DM,0,1\n\
         S3,DM,0,1\nS1,CM,1,3\nS3,CM,1,2\n",
    );
}

/// The serious events drive one template, DM another; CM states X itself.
fn serious(derivation: &str, intermediate: &str) -> String {
    format!(
        "{HEAD}intermediates:\n\
         \x20 - {{id: SER, dataset: AE, filter: \"AE.AESER = 'Y'\"{intermediate}}}\n\
         {}rows:\n\
         \x20 - {{id: cm, dataset: CM, derivations: {{USUBJID: CM.USUBJID, SRC: {{literal: CM}}, SEQ: CM.CMSEQ, X: {{literal: OWN}}}}}}\n\
         \x20 - {{id: ser, dataset: SER, derivations: {{USUBJID: SER.USUBJID, SRC: {{literal: SER}}, SEQ: SER.AESEQ}}}}\n\
         \x20 - {{id: dm, dataset: DM, derivations: {{USUBJID: DM.USUBJID, SRC: {{literal: DM}}, SEQ: {{literal: 0}}}}}}\n",
        columns("str", derivation),
    )
}

#[test]
fn an_intermediate_driver_over_another_input_reads_its_own_record() {
    // The SER rows read the serious event that built them in AE, which is not
    // the input behind the first template; the DM rows read their own SEX.
    let own = "USUBJID,SRC,SEQ,X\nS1,CM,1,OWN\nS3,CM,1,OWN\n";
    Fixture::new(&serious(
        "{case: [{when: \"SRC = 'SER'\", then: SER.AETERM}, {otherwise: DM.SEX}]}",
        "",
    ))
    .saved(&format!(
        "{own}S1,SER,1,HEADACHE\nS2,SER,1,RASH\nS1,DM,0,F\nS2,DM,0,M\nS3,DM,0,F\n"
    ));
}

#[test]
fn a_field_an_intermediate_driver_derives_is_refused_beside_another_driver() {
    let fixture = Fixture::new(&serious(
        "SER.LOWER",
        ", derivations: {LOWER: {str_case: {source: AE.AETERM, to: lower}}}",
    ));
    let issue = fixture.issue();
    assert_eq!(fixture.check(), std::slice::from_ref(&issue));
    assert_eq!(
        (
            issue.phase.as_str(),
            issue.condition.as_str(),
            issue.spec_paths.as_slice()
        ),
        (
            "planning",
            "unsupported_operation",
            ["columns.X.derivation.source".to_owned()].as_slice()
        )
    );
}
