#![cfg(feature = "yaml")]
//! Issue #2696 (Option B): a specification that declares no `parent` is a
//! composition of one layer and is pruned on the same terms as one that
//! writes `parent: null` or is inherited by an empty child. A dead
//! declaration's unresolved reference is discarded with it and does not fail
//! validation; the three forms build with equal outputs.

use std::fs;
use yamaa_adapters::specification_report::Identity;
use yamaa_core::domain_entry::Request;

struct Study(crate::support::Scratch);
impl Study {
    fn new(name: &str) -> Self {
        let root = crate::support::Scratch::new("prune-all");
        let dir = root.join(name);
        fs::create_dir_all(dir.join("input")).unwrap();
        fs::write(dir.join("input/obs.csv"), "ID,AGE\n1,40\n2,52\n").unwrap();
        Self(root.resolved(std::fs::canonicalize))
    }
    fn text(&self, path: &str) -> String {
        let value = self.0.join(path).to_string_lossy().into_owned();
        #[cfg(windows)]
        {
            value
                .strip_prefix(r"\\?\")
                .unwrap_or(&value)
                .replace('\\', "/")
        }
        #[cfg(unix)]
        {
            value
        }
    }
    /// Build one entry form and return its saved artifact bytes.
    fn build(&self, case: &str, entry: &str) -> Vec<u8> {
        let path = self.text(&format!("{case}/{entry}"));
        let mut result = crate::support::study::domain(
            yamaa_core::project_function::Language::Python,
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
        .unwrap_or_else(|failure| panic!("{case}/{entry} domain failed: {failure:?}"));
        assert!(
            result.issues().is_empty(),
            "{case}/{entry} issues: {:?}",
            result.issues()
        );
        assert!(result.output().is_some(), "{case}/{entry} has no output");
        assert!(result.save().unwrap(), "{case}/{entry} did not save");
        fs::read(self.0.join(case).join("xa.csv"))
            .unwrap_or_else(|error| panic!("{case}/{entry} saved artifact: {error}"))
    }
    /// Write the two entry forms for one specification body and assert that
    /// both build with identical saved outputs.
    fn assert_forms_agree(&self, case: &str, body: &str) {
        let dir = self.0.join(case);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("flat.yaml"), body).unwrap();
        fs::write(
            dir.join("child.yaml"),
            "yamaa: \"1.0\"\ntype: spec\nparent: flat.yaml\n",
        )
        .unwrap();
        let flat = self.build(case, "flat.yaml");
        assert_eq!(flat, b"ID,AGE\n1,40\n2,52\n", "{case} flat output");
        assert_eq!(self.build(case, "child.yaml"), flat, "{case} child");
    }
}

const HEADER: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: XA\nkeys: [ID]\ninput:\n  OBS: {path: input/obs.csv, types: {AGE: int}}\noutput:\n  path: xa.csv\n  columns: [ID, AGE]\n";
const COLUMNS: &str = "columns:\n  - name: ID\n    type: str\n    derivation: OBS.ID\n  - name: AGE\n    type: int\n    derivation: OBS.AGE\n";

#[test]
fn dead_intermediate_with_unknown_dataset_is_pruned_in_every_form() {
    let study = Study::new("unknown-dataset");
    let body = format!(
        "{HEADER}intermediates:\n  - id: UNUSED\n    dataset: NODS\n    key: [ID]\n    filter: \"NODS.FLAG = 'X'\"\n{COLUMNS}"
    );
    study.assert_forms_agree("unknown-dataset", &body);
}

#[test]
fn dead_intermediate_with_unknown_filter_field_is_pruned_in_every_form() {
    let study = Study::new("unknown-filter-field");
    let body = format!(
        "{HEADER}intermediates:\n  - id: UNUSED\n    dataset: OBS\n    key: [ID]\n    filter: \"OBS.NOSUCHFIELD = 'X'\"\n{COLUMNS}"
    );
    study.assert_forms_agree("unknown-filter-field", &body);
}

#[test]
fn dead_spare_column_is_pruned_in_every_form() {
    let study = Study::new("dead-spare-column");
    let body =
        format!("{HEADER}{COLUMNS}  - name: SPARE\n    type: int\n    derivation: NOSUCHCOL\n");
    study.assert_forms_agree("dead-spare-column", &body);
}

#[test]
fn valid_dead_declarations_build_with_equal_outputs_in_every_form() {
    let study = Study::new("valid-dead");
    let body = format!(
        "{HEADER}intermediates:\n  - id: UNUSED\n    dataset: OBS\n    key: [ID]\n    filter: \"OBS.AGE > 45\"\n{COLUMNS}  - name: SPARE\n    type: int\n    derivation: OBS.AGE\n"
    );
    study.assert_forms_agree("valid-dead", &body);
}
