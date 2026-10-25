#![cfg(feature = "yaml")]
//! The staleness guard (PROJECT-15) through the shared Define service. A
//! specification that declares `output.report` composes only with the
//! artifact and the inputs its saved run report records. Each study here is
//! built and saved by a real run first, so a report that matches is the one
//! the engine wrote; each mismatch then changes one file after that save.
//! The compositions of a study as its build saved it, or of files staged as a
//! build leaves them, are define cases of
//! `tests/fixtures/diagnostics/shared_define.json` (#3798).
mod staleness {
    use crate::support::Edit as _;
    use std::fs;
    use yamaa_application::{
        file_application, file_define::Define, specification_report::Identity,
    };
    use yamaa_core::{
        diagnostic::{ConditionCode, Diagnostic},
        domain_entry::Request,
        json::Value,
        project_environment::Submission,
        project_function::Language,
    };

    const PROJECT: &str = r#"yamaa: "1.0"
type: project
study:
  id: CATH
  name: CATH
  description: Example study submitting demographics
  protocol_name: CATH-01
sdtm:
  standards:
    - name: SDTMIG
      version: "3.4"
      status: Final
  specs:
    - dm.yaml
  define: define.xml
"#;

    /// DM over one raw input, saving its artifact and its run report.
    const SPEC: &str = r#"yamaa: "1.0"
type: spec
model: SDTM
domain: DM
keys: [STUDYID, USUBJID]
input:
  RAW: raw.csv
output:
  path: dm.csv
  columns: [DOMAIN, STUDYID, USUBJID]
  report: dm-report.csv
columns:
  - name: DOMAIN
    type: str
    label: Domain Abbreviation
    derivation: "'DM'"
    submission:
      core: Req
      role: Identifier
      length: 2
      origins:
        - type: Assigned
          source: Sponsor
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: RAW.STUDYID
    submission:
      core: Req
      role: Identifier
      length: 4
      origins:
        - type: Protocol
          source: Sponsor
  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: RAW.USUBJID
    submission:
      core: Req
      role: Identifier
      length: 9
      origins:
        - type: Assigned
          source: Sponsor
submission:
  label: Demographics
  class: SPECIAL PURPOSE
  structure: One record per subject
  repeating: false
"#;

    const RAW: &str = "STUDYID,USUBJID\nCATH,CATH-0001\nCATH,CATH-0002\n";
    const DM: &str = "DOMAIN,STUDYID,USUBJID\nDM,CATH,CATH-0001\nDM,CATH,CATH-0002\n";

    struct Study(crate::support::Scratch);
    impl Study {
        /// A study whose project lists `spec` as `dm.yaml`, over [`RAW`].
        fn new(spec: &str) -> Self {
            let root = crate::support::Scratch::canonical("staleness");
            let study = Self(root);
            study.write("project.yaml", PROJECT);
            study.write("dm.yaml", spec);
            study.write("raw.csv", RAW);
            study
        }
        /// [`Study::new`], built and saved: the artifact and the report the
        /// engine writes.
        fn built(spec: &str) -> Self {
            let study = Self::new(spec);
            let mut result = study.run();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            study
        }
        fn write(&self, name: &str, contents: impl AsRef<[u8]>) {
            fs::write(self.0.join(name), contents).unwrap();
        }
        fn read(&self, name: &str) -> Vec<u8> {
            fs::read(self.0.join(name)).unwrap()
        }
        fn path(&self, name: &str) -> String {
            self.0.join(name).to_str().unwrap().to_owned()
        }
        /// One run of `dm.yaml` alone, which the composition reads back.
        fn run(&self) -> file_application::Domain {
            let path = self.path("dm.yaml");
            crate::support::study::domain(
                Language::Python,
                Request {
                    specification: &path,
                    environment: None,
                },
                Identity {
                    runtime: "test",
                    runtime_version: "test",
                    engine_version: "test",
                    example: "test",
                    specification: &path,
                    base_directory: ".",
                },
            )
            .unwrap()
        }
        fn compose(&self) -> Define {
            Define::prepare(
                &self.path("project.yaml"),
                Submission::Sdtm,
                Some("2026-09-11T00:00:00"),
                "0.0.0",
                Some(Language::Python),
                None,
            )
        }
    }

    fn text(value: &str) -> Value {
        Value::String(value.into())
    }

    /// The one issue a refused composition reports.
    fn issue(define: &Define) -> Diagnostic {
        assert!(
            define.plan().is_none(),
            "a refused composition saves nothing"
        );
        let [issue] = define.issues() else {
            panic!("one issue: {:?}", define.issues())
        };
        issue.clone()
    }

    /// The facts a stale artifact's finding compared, after asserting its
    /// condition, its path at the listing's report and the dataset and report
    /// every such finding names.
    fn stale(define: &Define, reason: &str) -> Vec<(String, Value)> {
        let issue = issue(define);
        assert_eq!(issue.code, ConditionCode::StaleArtifact, "{issue:?}");
        assert_eq!(issue.spec_paths, ["sdtm.specs[0].output.report"]);
        let mut context = issue.context;
        assert_eq!(context.remove("dataset"), Some(text("DM")));
        assert_eq!(context.remove("report"), Some(text("dm-report.csv")));
        assert_eq!(context.remove("reason"), Some(text(reason)));
        context.into_iter().collect()
    }

    fn facts(values: &[(&str, Value)]) -> Vec<(String, Value)> {
        let mut facts: Vec<_> = values
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect();
        facts.sort_by(|left, right| left.0.cmp(&right.0));
        facts
    }

    /// Seam: the bytes the build of a composed study saves. The define cases
    /// `a_matching_report_composes` and
    /// `an_inherited_output_report_compares_the_entry_relative_output_path`
    /// of `shared_define.json` build `dm.yaml` first (`built`) and compose
    /// what it saved, and a `save` case states every file its save writes,
    /// but the run report saved beside the artifact is a CSV no authored file
    /// holds, which `select` does not read. The inherited build's run row is
    /// the case `an_inherited_output_s_run_report_names_its_entry_relative_path`.
    #[test]
    fn a_composed_study_s_build_saves_its_artifact() {
        let study = Study::built(SPEC);
        assert_eq!(study.read("dm.csv"), DM.as_bytes());
        let study = Study::new("yamaa: \"1.0\"\ntype: spec\nparent: base/parent.yaml\n");
        fs::create_dir(study.0.join("base")).unwrap();
        study.write("base/parent.yaml", SPEC);
        study.write("base/raw.csv", RAW);
        let mut result = study.run();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert!(result.save().unwrap());
        assert_eq!(study.read("base/dm.csv"), DM.as_bytes());
    }

    /// Seam: a file changed after the build that saved its report. A define
    /// case of `shared_define.json` builds its `built` specifications or
    /// stages its files, and a report no authored file holds is only built,
    /// so no case changes a file between that build and the composition.
    #[test]
    fn renaming_an_inherited_output_still_makes_its_report_stale() {
        let study = Study::new("yamaa: \"1.0\"\ntype: spec\nparent: base/parent.yaml\n");
        fs::create_dir(study.0.join("base")).unwrap();
        study.write("base/parent.yaml", SPEC);
        study.write("base/raw.csv", RAW);
        let mut result = study.run();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert!(result.save().unwrap());
        study.write(
            "base/parent.yaml",
            SPEC.edit("path: dm.csv", "path: dm2.csv"),
        );
        study.write("base/dm2.csv", DM);
        assert_eq!(
            stale(&study.compose(), "artifact_path"),
            facts(&[
                ("recorded", text("base/dm.csv")),
                ("actual", text("base/dm2.csv"))
            ])
        );
    }

    /// Seam: a file changed after the build that saved its report. A define
    /// case of `shared_define.json` builds its `built` specifications or
    /// stages its files, and a report no authored file holds is only built,
    /// so no case changes a file between that build and the composition.
    #[test]
    fn a_report_that_names_another_output_path_is_stale() {
        let study = Study::built(SPEC);
        // The output is renamed after the run; its report records dm.csv.
        study.write("dm.yaml", SPEC.edit("path: dm.csv", "path: dm2.csv"));
        study.write("dm2.csv", DM);
        assert_eq!(
            stale(&study.compose(), "artifact_path"),
            facts(&[("recorded", text("dm.csv")), ("actual", text("dm2.csv"))])
        );
    }

    /// Seam: a file changed after the build that saved its report. A define
    /// case of `shared_define.json` builds its `built` specifications or
    /// stages its files, and a report no authored file holds is only built,
    /// so no case changes a file between that build and the composition.
    #[test]
    fn a_report_of_a_failed_run_is_stale() {
        let study = Study::built(SPEC);
        // A later run fails on a missing key and replaces only the report,
        // leaving the earlier artifact (OUTPUT-10).
        study.write("raw.csv", "STUDYID,USUBJID\nCATH,\nCATH,CATH-0002\n");
        let mut failed = study.run();
        assert!(failed.output().is_none());
        assert!(failed.save_report().unwrap());
        assert_eq!(study.read("dm.csv"), DM.as_bytes());
        let condition = ConditionCode::OutputMissingKeyFailed.definition().condition;
        assert_eq!(
            stale(&study.compose(), "run_failed"),
            facts(&[("recorded", text(condition))])
        );
    }

    /// Seam: a file changed after the build that saved its report. A define
    /// case of `shared_define.json` builds its `built` specifications or
    /// stages its files, and a report no authored file holds is only built,
    /// so no case changes a file between that build and the composition.
    #[test]
    fn an_artifact_with_other_rows_than_its_report_records_is_stale() {
        let study = Study::built(SPEC);
        study.write("dm.csv", format!("{DM}DM,CATH,CATH-0003\n"));
        assert_eq!(
            stale(&study.compose(), "row_count"),
            facts(&[("recorded", Value::from(2)), ("actual", Value::from(3))])
        );
    }

    /// Seam: a file changed after the build that saved its report. A define
    /// case of `shared_define.json` builds its `built` specifications or
    /// stages its files, and a report no authored file holds is only built,
    /// so no case changes a file between that build and the composition.
    #[test]
    fn an_input_the_report_does_not_record_is_stale() {
        let study = Study::built(SPEC);
        // The input is renamed after the run: its report records RAW.
        study.write("dm.yaml", SPEC.replace("RAW", "SOURCE"));
        assert_eq!(
            stale(&study.compose(), "input_missing"),
            facts(&[("input", text("SOURCE"))])
        );
    }

    /// Seam: a file changed after the build that saved its report. A define
    /// case of `shared_define.json` builds its `built` specifications or
    /// stages its files, and a report no authored file holds is only built,
    /// so no case changes a file between that build and the composition.
    #[test]
    fn an_input_with_another_byte_length_is_stale() {
        let study = Study::built(SPEC);
        study.write("raw.csv", format!("{RAW}CATH,CATH-0003\n"));
        assert_eq!(
            stale(&study.compose(), "input_bytes"),
            facts(&[
                ("input", text("RAW")),
                ("recorded", Value::from(RAW.len())),
                ("actual", Value::from(RAW.len() + 15)),
            ])
        );
    }

    /// Seam: a file changed after the build that saved its report. A define
    /// case of `shared_define.json` builds its `built` specifications or
    /// stages its files, and a report no authored file holds is only built,
    /// so no case changes a file between that build and the composition.
    #[test]
    fn an_input_with_other_records_in_as_many_bytes_is_stale() {
        let study = Study::built(SPEC);
        // One quoted record of the recorded length: the same bytes count,
        // one record instead of two.
        let header = "STUDYID,USUBJID\n";
        let padding = RAW.len() - header.len() - "CATH,\"\"\n".len();
        let one = format!("{header}CATH,\"{}\"\n", "X".repeat(padding));
        assert_eq!(one.len(), RAW.len());
        study.write("raw.csv", &one);
        assert_eq!(
            stale(&study.compose(), "input_records"),
            facts(&[
                ("input", text("RAW")),
                ("recorded", Value::from(2)),
                ("actual", Value::from(1)),
            ])
        );
        // Bytes that no longer decode count no records.
        let mut undecodable = RAW.as_bytes().to_vec();
        *undecodable.last_mut().unwrap() = b'"';
        study.write("raw.csv", undecodable);
        assert_eq!(
            stale(&study.compose(), "input_records"),
            facts(&[
                ("input", text("RAW")),
                ("recorded", Value::from(2)),
                ("actual", Value::Null),
            ])
        );
    }

    /// Seam: a Parquet report read back after the build that saved it, and an
    /// artifact changed after that build; a Parquet report no authored file
    /// holds is only built.
    #[cfg(feature = "parquet")]
    #[test]
    fn a_parquet_report_is_read_back_as_it_was_written() {
        let study = Study::built(&SPEC.edit("report: dm-report.csv", "report: dm-report.parquet"));
        let define = study.compose();
        assert!(define.issues().is_empty(), "{:?}", define.issues());
        study.write("dm.csv", format!("{DM}DM,CATH,CATH-0003\n"));
        let issue = issue(&study.compose());
        assert_eq!(issue.code, ConditionCode::StaleArtifact, "{issue:?}");
        assert_eq!(issue.context["report"], text("dm-report.parquet"));
        assert_eq!(issue.context["reason"], text("row_count"));
    }

    /// Seam: an ODM XML input changed after the build that saved its report.
    /// An ODM XML input's records are the clinical items its profile
    /// decodes, as its run counted them (CHECK-24, ODM-1).
    #[cfg(feature = "odm")]
    #[test]
    fn an_odm_xml_input_counts_the_items_its_run_read() {
        let snapshot = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../benchmarks/sdtm-dm-odm-snapshot/input/odm.xml");
        let spec = SPEC
            .edit("  RAW: raw.csv\n", "  RAW: odm.xml\n")
            .edit("RAW.STUDYID", "RAW.StudyOID")
            .edit("RAW.USUBJID", "RAW.SubjectKey")
            .edit("      length: 4\n", "      length: 7\n");
        let study = Study::new(&spec);
        let xml = fs::read(snapshot).unwrap();
        study.write("odm.xml", &xml);
        let mut result = study.run();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert!(result.save().unwrap());
        let define = study.compose();
        assert!(define.issues().is_empty(), "{:?}", define.issues());
        // A later export of more bytes is not the one the run read.
        study.write("odm.xml", [xml.as_slice(), b"\n"].concat());
        let facts = stale(&study.compose(), "input_bytes");
        assert_eq!(facts[0], ("actual".to_owned(), Value::from(xml.len() + 1)));
        assert_eq!(facts[1], ("input".to_owned(), text("RAW")));
    }
}
