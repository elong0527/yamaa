#![cfg(feature = "yaml")]
//! A kept plan is reused only while the object's paths still name the files
//! it was compiled from (#3069).
//!
//! Compilation resolves a relative path against the current working
//! directory. After the working directory changes, the same spelling names
//! another study, so the run compiles that study instead of reusing the plan
//! of the old one. This file holds one test because the working directory is
//! shared by every thread of the process.
#[allow(dead_code)]
mod support;

use std::path::PathBuf;
use std::sync::Arc;

use yamaa::{
    Argument, CallError, CancellationToken, Engine, EngineOptions, FunctionError, FunctionHandle,
    FunctionIdentity, HostLanguage, LockContext, Plans, ProjectFunctions, Run, Scalar, Signature,
};

const PLAIN: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: SRC.ID}\n";
const PROJECT: &str = "schema_version: '1.0'\nsdtm:\n  specs: [dm.yaml]\n";

struct NoFunctions;
impl ProjectFunctions for NoFunctions {
    fn verify_lock(
        &self,
        _: &LockContext<'_>,
        _: &[FunctionIdentity],
    ) -> Result<(), FunctionError> {
        Ok(())
    }
    fn bind(&self, _: &FunctionIdentity, _: &Signature) -> Result<FunctionHandle, FunctionError> {
        unreachable!("these specifications call no project function")
    }
    fn invoke(&self, _: &FunctionHandle, _: &[Argument<'_>]) -> Result<Scalar, CallError> {
        unreachable!("these specifications call no project function")
    }
}

/// How the run got its plan, from its private measurements.
fn plan_of(run: &Run) -> &'static str {
    let observed = run.build().observations(true).unwrap();
    ["compiled", "refused", "reused"]
        .into_iter()
        .find(|status| observed.contains(&format!("\"plan\":\"{status}\"")))
        .unwrap()
}

/// Two studies with the same file names; B's input has one more row.
struct Studies {
    root: support::Scratch,
    previous: PathBuf,
}
impl Studies {
    fn new() -> Self {
        let root = support::Scratch::new("plan-identity");
        for (name, input) in [("a", "ID\n1\n2\n"), ("b", "ID\n1\n2\n3\n")] {
            let study = root.join(name);
            std::fs::create_dir_all(&study).unwrap();
            std::fs::write(study.join("spec.yaml"), PLAIN).unwrap();
            std::fs::write(
                study.join("dm.yaml"),
                PLAIN
                    .replace("domain: TEST", "domain: DM")
                    .replace("out.csv", "dm.csv"),
            )
            .unwrap();
            std::fs::write(study.join("project.yaml"), PROJECT).unwrap();
            std::fs::write(study.join("input.csv"), input).unwrap();
        }
        Self {
            root,
            previous: std::env::current_dir().unwrap(),
        }
    }
    fn enter(&self, name: &str) {
        std::env::set_current_dir(self.root.join(name)).unwrap();
    }
}
impl Drop for Studies {
    fn drop(&mut self) {
        // The scratch field drops after this, once the process has left it.
        let _ = std::env::set_current_dir(&self.previous);
    }
}

#[test]
fn relative_paths_after_a_working_directory_change_compile_the_study_they_now_name() {
    let studies = Studies::new();
    let engine = Engine::new(
        EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
            .with_functions(Arc::new(NoFunctions)),
    );
    let token = CancellationToken::new();

    // A spec object with a relative path.
    let plans = Plans::new();
    let spec = |plans: &Plans| {
        let run = engine
            .spec_run_reusing("spec.yaml", None, plans, &token)
            .unwrap();
        (plan_of(&run), run.output().unwrap().rows())
    };
    studies.enter("a");
    assert_eq!(spec(&plans), ("compiled", 2));
    assert_eq!(spec(&plans), ("reused", 2));
    studies.enter("b");
    assert_eq!(spec(&plans), ("compiled", 3));
    assert_eq!(spec(&plans), ("reused", 3));
    // The object keeps one plan, so the way back compiles study A again.
    studies.enter("a");
    assert_eq!(spec(&plans), ("compiled", 2));

    // A project object with a relative project path.
    let plans = Plans::new();
    let project = |plans: &Plans| {
        let runs = engine
            .project_run_reusing("project.yaml", None, plans, &token)
            .unwrap();
        let run = runs.get("DM").unwrap();
        (plan_of(run), run.output().unwrap().rows())
    };
    studies.enter("a");
    assert_eq!(project(&plans), ("compiled", 2));
    assert_eq!(project(&plans), ("reused", 2));
    studies.enter("b");
    assert_eq!(project(&plans), ("compiled", 3));
    assert_eq!(project(&plans), ("reused", 3));

    // `project.spec(name)`: the listed specification's located path with the
    // project's relative path. The project path now names study B's project,
    // so the plan compiled with study A's project is not reused.
    studies.enter("a");
    let listed = engine.project_spec("project.yaml", "DM").unwrap();
    let plans = Plans::new();
    let member = |plans: &Plans| {
        let run = engine
            .spec_run_reusing(&listed, Some("project.yaml"), plans, &token)
            .unwrap();
        (plan_of(&run), run.output().unwrap().rows())
    };
    assert_eq!(member(&plans), ("compiled", 2));
    assert_eq!(member(&plans), ("reused", 2));
    studies.enter("b");
    assert_eq!(member(&plans), ("compiled", 2));
    assert_eq!(member(&plans), ("reused", 2));
}
