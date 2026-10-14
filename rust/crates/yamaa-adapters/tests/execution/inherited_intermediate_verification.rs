//! An intermediate verification remains load-bearing through inheritance.
//!
//! Issue #2189: an assertion-only intermediate (one that no column reads)
//! was pruned by the inheritance resolver, so an empty child of a parent
//! that fails `duplicate_intermediate_records` (REQ-1245) built and saved
//! with no issue. Intermediates carrying nonempty `verifications` are
//! reachability roots (REQ-0638), so parent and child now agree.
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

const PARENT_HEAD: &str = "schema_version: '1.0'\ndomain: PV\nkeys: [ID]\ninput: {OBS: obs.csv, REF: ref.csv}\nbase: OBS\nintermediates:\n  - id: REFCHK\n    dataset: REF\n    key: {CODE: CODE}\n    filter: \"REF.KIND = 'X'\"\n    verifications:\n      - unique: [CODE]\n";

#[test]
fn assertion_only_intermediate_keeps_its_check_through_inheritance() {
    // `read` adds a column that reads the intermediate; the failure must be
    // identical whether or not anything reads it.
    for read in [false, true] {
        for duplicate in [false, true] {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("obs.csv"), "ID,CODE\n1,A\n2,B\n").unwrap();
            let donor = if duplicate {
                "CODE,KIND,LABEL\nA,X,Alpha\nA,X,Alpha again\nB,X,Beta\nB,Y,Beta (Y)\n"
            } else {
                "CODE,KIND,LABEL\nA,X,Alpha\nB,X,Beta\nB,Y,Beta (Y)\n"
            };
            fs::write(fixture.0.join("ref.csv"), donor).unwrap();
            let (output_columns, label_column) = if read {
                (
                    "[ID, CODE, LABEL]",
                    "  - {name: LABEL, type: str, derivation: REFCHK.LABEL}\n",
                )
            } else {
                ("[ID, CODE]", "")
            };
            fs::write(
                fixture.0.join("parent.yaml"),
                format!(
                    "{PARENT_HEAD}output: {{path: result.csv, columns: {output_columns}}}\ncolumns:\n  - {{name: ID, type: str, derivation: OBS.ID}}\n  - {{name: CODE, type: str, derivation: OBS.CODE}}\n{label_column}"
                ),
            )
            .unwrap();
            fs::write(
                fixture.0.join("child.yaml"),
                "schema_version: '1.0'\nparents: parent.yaml\n",
            )
            .unwrap();
            let mut parent_issues = None;
            for entry in ["parent.yaml", "child.yaml"] {
                let mut result = fixture.build(entry);
                if duplicate {
                    assert!(
                        result.output().is_none(),
                        "read={read} {entry}: a failed build has no output"
                    );
                    let issues = result.issues();
                    assert_eq!(issues.len(), 1, "read={read} {entry}: {issues:?}");
                    let issue = &issues[0];
                    assert_eq!(issue.phase, "verification", "read={read} {entry}");
                    assert_eq!(
                        issue.condition, "duplicate_intermediate_records",
                        "read={read} {entry}"
                    );
                    assert_eq!(
                        issue.requirement.as_deref(),
                        Some("REQ-1245"),
                        "read={read} {entry}"
                    );
                    assert_eq!(
                        issue.spec_paths,
                        ["intermediates[0].verifications[0].unique"],
                        "read={read} {entry}"
                    );
                    assert_eq!(
                        issue.context,
                        "{\"columns\":[\"CODE\"],\"dataset\":\"REF\",\"duplicate_count\":1,\"intermediate\":\"REFCHK\"}",
                        "read={read} {entry}"
                    );
                    match &parent_issues {
                        None => parent_issues = Some(issues),
                        Some(expected) => {
                            // The child must report the parent's issue: same
                            // phase, condition, requirement and spec path. On
                            // the read path the parent's context additionally
                            // embeds its own spec path (pre-existing composed
                            // provenance), so context is compared verbatim
                            // only for the assertion-only case this issue is
                            // about.
                            assert_eq!(issues.len(), expected.len(), "read={read}");
                            for (actual, want) in issues.iter().zip(expected.iter()) {
                                assert_eq!(actual.phase, want.phase, "read={read}");
                                assert_eq!(actual.condition, want.condition, "read={read}");
                                assert_eq!(actual.requirement, want.requirement, "read={read}");
                                assert_eq!(actual.spec_paths, want.spec_paths, "read={read}");
                                if !read {
                                    assert_eq!(actual.context, want.context, "read={read}");
                                }
                            }
                        }
                    }
                    assert!(
                        result.save().is_err(),
                        "read={read} {entry}: a failed build cannot save"
                    );
                    assert!(
                        !fixture.0.join("result.csv").exists(),
                        "read={read} {entry}: a failed build publishes no artifact"
                    );
                } else {
                    assert!(
                        result.issues().is_empty(),
                        "read={read} {entry}: {:?}",
                        result.issues()
                    );
                    assert!(result.save().unwrap(), "read={read} {entry}");
                    let expected = if read {
                        "ID,CODE,LABEL\n1,A,Alpha\n2,B,Beta\n"
                    } else {
                        "ID,CODE\n1,A\n2,B\n"
                    };
                    assert_eq!(
                        fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                        expected,
                        "read={read} {entry}"
                    );
                    fs::remove_file(fixture.0.join("result.csv")).unwrap();
                }
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
        )
        .unwrap();
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
