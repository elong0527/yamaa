//! An intermediate verification remains load-bearing through inheritance.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-inherited_intermediate_verification-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn build(&self, name: &str) -> file_application::Domain {
        let path = self.0.join(name);
        let path = path.to_str().unwrap();
        let identity = || Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "inherited_intermediate_verification",
            specification: path,
            base_directory: ".",
        };
        file_application::domain(
            Request {
                specification: path,
                environment: None,
            },
            identity(),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn assertion_only_intermediate_keeps_its_filtered_records_and_blocks_failed_publication() {
    for duplicate in [false, true] {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("obs.csv"), "ID,CODE\n1,A\n2,B\n").unwrap();
        let donor = if duplicate {
            "CODE,KIND\nA,X\nA,X\nB,X\nB,Y\n"
        } else {
            "CODE,KIND\nA,X\nB,X\nB,Y\n"
        };
        fs::write(fixture.0.join("ref.csv"), donor).unwrap();
        fs::write(fixture.0.join("parent.yaml"), "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: OBS\ninput: {OBS: obs.csv, REF: ref.csv}\noutput: {path: result.csv, columns: [ID, CODE]}\nintermediates:\n  - id: REFCHK\n    dataset: REF\n    key: {CODE: CODE}\n    filter: \"REF.KIND = 'X'\"\n    verifications:\n      - unique: [CODE]\ncolumns:\n  - {name: ID, type: str, derivation: OBS.ID}\n  - {name: CODE, type: str, derivation: OBS.CODE}\n").unwrap();
        fs::write(
            fixture.0.join("child.yaml"),
            "schema_version: '1.0'\nparents: parent.yaml\n",
        )
        .unwrap();
        for entry in ["parent.yaml", "child.yaml"] {
            let mut result = fixture.build(entry);
            if duplicate {
                assert!(result.output().is_none());
                assert_eq!(result.issues().len(), 1, "{entry}: {:?}", result.issues());
                let issue = &result.issues()[0];
                assert_eq!(issue.phase, "verification");
                assert_eq!(issue.condition, "duplicate_intermediate_records");
                assert_eq!(issue.requirement.as_deref(), Some("REQ-1245"));
                assert_eq!(issue.context, "{\"columns\":[\"CODE\"],\"dataset\":\"REF\",\"duplicate_count\":1,\"intermediate\":\"REFCHK\"}");
                assert!(result.save().is_err());
                assert!(!fixture.0.join("result.csv").exists());
            } else {
                assert!(result.issues().is_empty(), "{entry}: {:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(
                    fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                    "ID,CODE\n1,A\n2,B\n"
                );
            }
        }
    }
}

#[test]
fn empty_intermediate_verifications_do_not_keep_an_unread_lookup_or_input_live() {
    for verifications in ["", "    verifications: []\n"] {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("obs.csv"), "ID\n1\n2\n").unwrap();
        fs::write(
            fixture.0.join("parent.yaml"),
            format!(
                "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: OBS\ninput: {{OBS: obs.csv, UNUSED: absent.csv}}\noutput: {{path: result.csv, columns: [ID]}}\nintermediates:\n  - id: UNREAD\n    dataset: UNUSED\n{verifications}columns:\n  - {{name: ID, type: str, derivation: OBS.ID}}\n"
            ),
        ).unwrap();
        fs::write(
            fixture.0.join("child.yaml"),
            "schema_version: '1.0'\nparents: parent.yaml\n",
        )
        .unwrap();
        let mut result = fixture.build("child.yaml");
        assert!(
            result.issues().is_empty(),
            "{verifications:?}: {:?}",
            result.issues()
        );
        assert!(result.save().unwrap());
        assert_eq!(
            fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
            "ID\n1\n2\n"
        );
        assert!(!fixture.0.join("absent.csv").exists());
    }
}
