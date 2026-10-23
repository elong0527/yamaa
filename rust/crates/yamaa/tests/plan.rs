//! Plan reuse through a [`Plans`] holder, as a host's lazy object runs
//! (#2821). There is no public compile-once plan handle (#3030 D22, D72):
//! a holder keeps what a run compiled, and a later run reuses it.
//!
//! A run that fails on its input keeps its plan, and the next run reuses it
//! and reads its input afresh; runs through one holder from several threads
//! are independent; and a run outlives its engine. Expected bytes are
//! written here from the authored inputs: the output declares exactly the
//! input's columns, so each study's saved CSV is its input text.
#![cfg(feature = "yaml")]

use std::sync::Arc;

use yamaa::{CancellationToken, Engine, EngineOptions, HostLanguage, MemoryResolver, Plans, Run};

const SPECIFICATION: &[u8] = b"yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\noutput: {path: out.csv, columns: [ID, NAME]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: NAME, type: str, derivation: SRC.NAME}\n";
const STUDY_A: &[u8] = b"ID,NAME\n1,alpha\n2,beta\n";
const STUDY_B: &[u8] = b"ID,NAME\n7,gamma\n";
const SAVED: &str = "memory:/out.csv";

/// The specification, and the input `study` when there is one.
fn resolver(study: Option<&[u8]>) -> Arc<MemoryResolver> {
    let resolver = Arc::new(MemoryResolver::new());
    resolver.insert("/", "spec.yaml", "spec:1", Arc::from(SPECIFICATION));
    if let Some(study) = study {
        resolver.insert("/spec.yaml", "input.csv", "data:1", Arc::from(study));
    }
    resolver
}

fn engine(resolver: &Arc<MemoryResolver>) -> Engine {
    Engine::new(
        EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
            .with_resolver(resolver.clone()),
    )
}

fn run(engine: &Engine, plans: &Plans) -> Run {
    engine
        .spec_run_reusing("spec.yaml", None, plans, &CancellationToken::new())
        .unwrap()
}

/// How the run got its plan, from its private measurements.
fn plan_of(run: &Run) -> &'static str {
    let observed = run.observations(true).unwrap();
    ["compiled", "refused", "reused"]
        .into_iter()
        .find(|status| observed.contains(&format!("\"plan\":\"{status}\"")))
        .unwrap()
}

/// Save the run there and return the saved bytes.
fn saved(mut run: Run, resolver: &MemoryResolver) -> Vec<u8> {
    assert!(run.diagnostics().is_empty(), "{:?}", run.diagnostics());
    run.save(&CancellationToken::new()).unwrap();
    resolver.published(SAVED).unwrap().to_vec()
}

#[test]
fn a_failed_run_keeps_its_plan_and_the_next_run_reads_its_input_afresh() {
    let resolver = resolver(None);
    let engine = engine(&resolver);
    let plans = Plans::new();
    let mut failed = run(&engine, &plans);
    assert_eq!(plan_of(&failed), "compiled");
    assert!(failed.output().is_none());
    let first = &failed.diagnostics()[0];
    assert_eq!(first.phase, "validation");
    assert_eq!(first.code, "resource_path_missing");
    assert_eq!(first.rule.as_deref(), Some("FILE-7"));
    assert_eq!(first.spec_path, ["input.SRC.path"]);
    assert!(failed.save(&CancellationToken::new()).is_err());
    assert!(resolver.published(SAVED).is_none());
    // The input arrives; the kept plan runs it.
    resolver.insert("/spec.yaml", "input.csv", "data:1", Arc::from(STUDY_A));
    let second = run(&engine, &plans);
    assert_eq!(plan_of(&second), "reused");
    assert_eq!(saved(second, &resolver), STUDY_A);
    // A new object version behind the same path: had the kept plan held the
    // previous run's input, its capture would be refused as changed.
    resolver.insert("/spec.yaml", "input.csv", "data:2", Arc::from(STUDY_B));
    let third = run(&engine, &plans);
    assert_eq!(plan_of(&third), "reused");
    assert_eq!(saved(third, &resolver), STUDY_B);
}

#[test]
fn runs_through_one_holder_from_several_threads_are_independent() {
    let resolver = resolver(Some(STUDY_A));
    let engine = engine(&resolver);
    let plans = Plans::new();
    let expected = run(&engine, &plans).output().unwrap().batches().to_vec();
    std::thread::scope(|threads| {
        let runs = (0..8)
            .map(|_| threads.spawn(|| run(&engine, &plans)))
            .collect::<Vec<_>>();
        for handle in runs {
            let run = handle.join().unwrap();
            assert_eq!(plan_of(&run), "reused");
            assert_eq!(run.output().unwrap().batches(), expected);
        }
    });
}

#[test]
fn a_run_outlives_its_engine_and_holder() {
    let resolver = resolver(Some(STUDY_A));
    let run = {
        let plans = Plans::new();
        run(&engine(&resolver), &plans)
    };
    assert_eq!(run.output().unwrap().rows(), 2);
    assert_eq!(saved(run, &resolver), STUDY_A);
}
