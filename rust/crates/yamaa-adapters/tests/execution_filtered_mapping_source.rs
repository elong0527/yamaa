#![cfg(feature = "yaml")]
//! A mapping source carries the same filter, selection, and absent value as
//! a plain source binding, so a filtered raw column no longer needs a
//! helper column that exists only to feed the dictionary lookup.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
use yamaa_core::domain_entry::Request;
use yamaa_core::json::Value;

struct Fixture(PathBuf);
impl Fixture {
    fn new(specification: &str, files: &[(&str, &str)]) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "yamaa-filtered-mapping-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("spec.yaml"), specification).unwrap();
        for (name, body) in files {
            fs::write(root.join(name), body).unwrap();
        }
        Self(root)
    }
    fn run(&self) -> Result<file_application::Domain, Vec<Issue>> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        file_application::domain(
            Request {
                specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "filtered-mapping-source",
                specification,
                base_directory: ".",
            },
        )
    }
    fn saved(&self, output: &str) -> String {
        let mut domain = self.run().unwrap();
        assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        assert!(domain.save().unwrap());
        fs::read_to_string(self.0.join(output)).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn issue_rows(issues: &[Issue]) -> Value {
    Value::Array(
        issues
            .iter()
            .map(|issue| {
                let mut value = issue.as_value();
                value["context"] = issue.context.clone();
                value
            })
            .collect(),
    )
}

// The junk record (SEQ 0) is filtered out and the earliest surviving record
// wins, so SEX is mapped directly from DM.SEX: no _SEX helper column exists
// and the output holds exactly the three authored columns.
const VISITS: &str = "STUDYID,USUBJID,SEQ,VISIT,SEX\n\
    S1,001,0,JUNK,U\n\
    S1,001,1,SCREENING,m\n\
    S1,001,2,BASELINE,f\n\
    S1,002,0,JUNK,U\n\
    S1,002,1,SCREENING,F\n\
    S1,002,2,BASELINE,M\n";

fn adsl_specification(source: &str) -> String {
    format!(
        "schema_version: '1.0'\n\
         domain: ADSL\n\
         keys: [STUDYID, USUBJID]\n\
         input: {{DM: {{path: dm.csv, types: {{SEQ: int}}}}}}\n\
         output: {{path: adsl.csv, columns: [STUDYID, USUBJID, SEX]}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str, derivation: DM.STUDYID}}\n\
         \x20 - {{name: USUBJID, type: str, derivation: DM.USUBJID}}\n\
         \x20 - {{name: SEX, type: str, derivation: {{mapping: {{source: {source}, \
         case_sensitive: false, \
         dict: {{M: Male, F: Female, Male: Male, Female: Female, Intersex: Intersex}}, \
         missing: {{literal: null}}, unmapped: {{literal: null}}}}}}}}\n"
    )
}

#[test]
fn mapping_source_with_filter_and_selection_replaces_the_helper_column() {
    let fixture = Fixture::new(
        &adsl_specification(
            "{variable: DM.SEX, filter: \"DM.SEQ >= 1\", order_by: [DM.SEQ], keep: first}",
        ),
        &[("dm.csv", VISITS)],
    );
    assert_eq!(
        fixture.saved("adsl.csv"),
        "STUDYID,USUBJID,SEX\nS1,001,Male\nS1,002,Female\n"
    );
}

#[test]
fn mapping_source_with_a_filter_alone_reads_the_selected_record() {
    let fixture = Fixture::new(
        &adsl_specification("{variable: DM.SEX, filter: \"DM.VISIT = 'BASELINE'\"}"),
        &[("dm.csv", VISITS)],
    );
    assert_eq!(
        fixture.saved("adsl.csv"),
        "STUDYID,USUBJID,SEX\nS1,001,Female\nS1,002,Male\n"
    );
}

#[test]
fn mapping_source_absent_value_feeds_the_dictionary() {
    let fixture = Fixture::new(
        &adsl_specification("{variable: DM.SEX, absent: M}"),
        &[("dm.csv", "STUDYID,USUBJID,SEQ,VISIT\nS1,001,1,SCREENING\n")],
    );
    assert_eq!(
        fixture.saved("adsl.csv"),
        "STUDYID,USUBJID,SEX\nS1,001,Male\n"
    );
}

#[test]
fn mapping_source_selection_without_its_pair_is_rejected() {
    for source in [
        "{variable: DM.SEX, order_by: [DM.SEQ]}",
        "{variable: DM.SEX, keep: first}",
    ] {
        let fixture = Fixture::new(&adsl_specification(source), &[("dm.csv", VISITS)]);
        let mut domain = fixture.run().unwrap();
        assert!(domain.save().is_err(), "{source}");
        assert_eq!(
            issue_rows(&domain.issues()),
            yamaa_adapters::json!([{
                "phase": "validation",
                "condition": "unpaired_fields",
                "requirement": "REQ-0119",
                "spec_paths": ["columns.SEX.derivation.mapping.source"],
                "context": {
                    "operation": "source",
                    "declared": [if source.contains("keep") { "keep" } else { "order_by" }],
                    "missing": [if source.contains("keep") { "order_by" } else { "keep" }],
                },
            }]),
            "{source}"
        );
    }
}

#[test]
fn cut_source_with_a_filter_reads_only_the_selected_records() {
    let fixture = Fixture::new(
        "schema_version: '1.0'\n\
         domain: OUT\n\
         keys: [ID]\n\
         input: {SRC: {path: src.csv, types: {AGE: int}}}\n\
         output: {path: out.csv, columns: [ID, VALUE]}\n\
         columns:\n\
         \x20 - {name: ID, type: str, derivation: SRC.ID}\n\
         \x20 - {name: VALUE, type: str, derivation: {cut: {source: \
         {variable: SRC.AGE, filter: \"SRC.KIND = 'age'\"}, breaks: [65], \
         labels: ['<65', '>=65'], missing: UNKNOWN}}}\n",
        &[(
            "src.csv",
            "ID,KIND,AGE\n001,sex,\n001,age,34\n002,sex,\n002,age,71\n",
        )],
    );
    assert_eq!(fixture.saved("out.csv"), "ID,VALUE\n001,<65\n002,>=65\n");
}

#[test]
fn first_available_source_with_a_selection_reads_the_ordered_match() {
    let fixture = Fixture::new(
        "schema_version: '1.0'\n\
         domain: ADSL\n\
         keys: [STUDYID, USUBJID]\n\
         input: {DM: {path: dm.csv, types: {SEQ: int}}}\n\
         output: {path: adsl.csv, columns: [STUDYID, USUBJID, SEX]}\n\
         columns:\n\
         \x20 - {name: STUDYID, type: str, derivation: DM.STUDYID}\n\
         \x20 - {name: USUBJID, type: str, derivation: DM.USUBJID}\n\
         \x20 - {name: SEX, type: str, derivation: {first_available: {sources: \
         [{variable: DM.SEX, order_by: [DM.SEQ], keep: last}], missing: U}}}\n",
        &[(
            "dm.csv",
            "STUDYID,USUBJID,SEQ,SEX\nS1,001,1,M\nS1,001,2,Female\nS1,002,1,F\nS1,002,2,Male\n",
        )],
    );
    assert_eq!(
        fixture.saved("adsl.csv"),
        "STUDYID,USUBJID,SEX\nS1,001,Female\nS1,002,Male\n"
    );
}
