//! Inherited lookup/input collisions remain visible to semantic validation.
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
            "yamaa-inherited_identifier_collision-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn issues(&self, name: &str) -> Vec<yamaa_adapters::issue_rows::Issue> {
        let path = self.0.join(name);
        let path = path.to_str().unwrap();
        let identity = || Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "inherited_identifier_collision",
            specification: path,
            base_directory: ".",
        };
        let checked = file_application::check(
            Request {
                specification: path,
                environment: None,
            },
            identity(),
        );
        let built = file_application::domain(
            Request {
                specification: path,
                environment: None,
            },
            identity(),
        )
        .err()
        .unwrap();
        assert_eq!(checked, built);
        built
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn a_reachable_qualifier_collision_is_rejected_in_flat_and_composed_specs() {
    for inherited in [false, true] {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("obs.csv"), "ID\n1\n2\n").unwrap();
        fs::write(fixture.0.join("ref.csv"), "ID,TRT\n1,DRUG A\n2,DRUG B\n").unwrap();
        let parent = "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: OBS\ninput: {OBS: obs.csv, REF: ref.csv}\noutput: {path: result.csv, columns: [ID, TRT]}\ncolumns:\n  - {name: ID, type: str, derivation: OBS.ID}\n  - {name: TRT, type: str, derivation: REF.TRT}\n";
        let lookup = "intermediates:\n  - {id: REF, dataset: REF, filter: \"REF.TRT = 'DRUG A'\", no_match: NONE}\n";
        fs::write(fixture.0.join("parent.yaml"), parent).unwrap();
        let entry = if inherited {
            format!("schema_version: '1.0'\nparents: parent.yaml\n{lookup}")
        } else {
            format!("{parent}{lookup}")
        };
        fs::write(fixture.0.join("child.yaml"), entry).unwrap();
        let issues = fixture.issues("child.yaml");
        assert_eq!(issues.len(), 1, "{inherited}: {issues:?}");
        let issue = &issues[0];
        assert_eq!(issue.phase, "validation");
        assert_eq!(issue.condition, "duplicate_identifier");
        assert_eq!(issue.requirement.as_deref(), Some("REQ-0113"));
        assert_eq!(issue.context, "{\"identifier\":\"REF\"}");
        assert!(issue
            .spec_paths
            .iter()
            .any(|path| path == "intermediates[0].id"));
        assert!(issue.spec_paths.iter().any(|path| path == "input.REF"));
        assert!(!fixture.0.join("result.csv").exists());
    }
}
