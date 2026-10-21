//! The reusable compile-once plan handle (#2821 step 2).
//!
//! `Engine::compile` reads no study data, one plan runs many studies with
//! independently expected results, a failed run leaves the plan usable, the
//! plan crosses threads, and a build is exactly a compile followed by a run.
//! Expected bytes are written here from the authored inputs: the output
//! declares exactly the input's columns, so each study's saved CSV is its
//! input text.
use std::sync::{Arc, Mutex};

use yamaa::{
    CancellationToken, Engine, EngineOptions, Error, Event, HostLanguage, MemoryResolver, Plan,
    Progress, Request, ResolverFailure, ResolverKind, ResolverReply, ResolverRequest,
    ResourceResolver, RunRequest,
};

const SPECIFICATION: &[u8] = b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\noutput: {path: out.csv, columns: [ID, NAME]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: NAME, type: str, derivation: SRC.NAME}\n";
const STUDY_A: &[u8] = b"ID,NAME\n1,alpha\n2,beta\n";
const STUDY_B: &[u8] = b"ID,NAME\n7,gamma\n";
const SAVED: &str = "memory:/out.csv";

fn options() -> EngineOptions {
    EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
}

/// The compile resolver: the specification and nothing else.
fn specification() -> Arc<MemoryResolver> {
    let resolver = Arc::new(MemoryResolver::new());
    resolver.insert("/", "spec.yaml", "spec:1", Arc::from(SPECIFICATION));
    resolver
}

/// One study's resolver: its input and nothing else.
fn study(reference: &str, bytes: &[u8]) -> Arc<MemoryResolver> {
    let resolver = Arc::new(MemoryResolver::new());
    resolver.insert("/spec.yaml", "input.csv", reference, Arc::from(bytes));
    resolver
}

fn compile(engine: &Engine) -> Plan {
    engine
        .compile(&Request::new("spec.yaml"), &CancellationToken::new())
        .unwrap()
}

#[test]
fn plans_and_run_requests_are_owned_shareable_values() {
    fn assert_shared<T: Send + Sync + Clone + 'static>() {}
    assert_shared::<Plan>();
    assert_shared::<RunRequest>();
}

#[test]
fn cancelled_tokens_fail_compile_and_run_before_entry() {
    let engine = Engine::new(options().with_resolver(specification()));
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        engine.compile(&Request::new("spec.yaml"), &cancelled),
        Err(Error::Cancelled)
    ));
    let plan = compile(&engine);
    assert!(matches!(
        plan.run(&RunRequest::new(), &cancelled),
        Err(Error::Cancelled)
    ));
}

#[cfg(feature = "yaml")]
mod runs {
    use super::*;

    /// Refuse, by panicking, any study request during compilation.
    struct NoInput(Arc<MemoryResolver>);
    impl ResourceResolver for NoInput {
        fn request(&self, request: ResolverRequest<'_>) -> Result<ResolverReply, ResolverFailure> {
            assert_ne!(
                request.kind,
                ResolverKind::Input,
                "compilation requested study input {}",
                request.written
            );
            self.0.request(request)
        }
    }

    /// Record every request a resolver receives.
    #[derive(Default)]
    struct Recording {
        inner: MemoryResolver,
        requests: Mutex<Vec<String>>,
    }
    impl ResourceResolver for Recording {
        fn request(&self, request: ResolverRequest<'_>) -> Result<ResolverReply, ResolverFailure> {
            self.requests.lock().unwrap().push(format!(
                "{} {} {} {}",
                request.operation.name(),
                request.kind.name(),
                request.declaring,
                request.written
            ));
            self.inner.request(request)
        }
    }

    /// Record every progress event as owned text.
    #[derive(Default)]
    struct Events(Mutex<Vec<String>>);
    impl Progress for Events {
        fn observe(&self, event: Event) {
            self.0.lock().unwrap().push(format!("{event:?}"));
        }
    }
    impl Events {
        fn take(&self) -> Vec<String> {
            std::mem::take(&mut *self.0.lock().unwrap())
        }
    }

    /// Run once from `resolver` and save there; return the saved bytes.
    fn run_and_save(plan: &Plan, resolver: Arc<MemoryResolver>) -> Vec<u8> {
        let mut build = plan
            .run(
                &RunRequest::new().with_resolver(resolver.clone()),
                &CancellationToken::new(),
            )
            .unwrap();
        assert!(build.report().is_empty(), "{:?}", build.report());
        assert!(build.output().is_some());
        build.save(&CancellationToken::new()).unwrap();
        resolver.published(SAVED).unwrap().to_vec()
    }

    #[test]
    fn compile_makes_no_study_request() {
        let engine = Engine::new(options().with_resolver(Arc::new(NoInput(specification()))));
        let plan = compile(&engine);
        assert!(plan.report().is_empty(), "{:?}", plan.report());
        assert_eq!(run_and_save(&plan, study("study:a", STUDY_A)), STUDY_A);
    }

    #[test]
    fn one_plan_runs_two_studies_with_their_own_results() {
        let engine = Engine::new(options().with_resolver(specification()));
        let plan = compile(&engine);
        for (reference, bytes) in [("study:a", STUDY_A), ("study:b", STUDY_B)] {
            assert_eq!(run_and_save(&plan, study(reference, bytes)), bytes);
        }
        // The first study again, from a new resolver: nothing is left over.
        assert_eq!(run_and_save(&plan, study("study:a", STUDY_A)), STUDY_A);
    }

    #[test]
    fn a_failed_run_leaves_the_plan_ready_for_the_next() {
        let engine = Engine::new(options().with_resolver(specification()));
        let plan = compile(&engine);
        let empty = Arc::new(MemoryResolver::new());
        let mut failed = plan
            .run(
                &RunRequest::new().with_resolver(empty.clone()),
                &CancellationToken::new(),
            )
            .unwrap();
        assert!(failed.output().is_none());
        let first = &failed.report().diagnostics[0];
        assert_eq!(first.phase, "validation");
        assert_eq!(first.code, "resource_path_missing");
        assert_eq!(first.rule.as_deref(), Some("FILE-7"));
        assert_eq!(first.spec_path, ["input.SRC.path"]);
        assert_eq!(
            first.context.to_json(),
            r#"{"dataset":"SRC","path":"input.csv"}"#
        );
        assert!(failed.save(&CancellationToken::new()).is_err());
        assert!(empty.published(SAVED).is_none());
        assert_eq!(run_and_save(&plan, study("study:b", STUDY_B)), STUDY_B);
    }

    #[test]
    fn a_compile_authority_run_holds_no_snapshot_of_the_previous_run() {
        let resolver = specification();
        resolver.insert("/spec.yaml", "input.csv", "data:1", Arc::from(STUDY_A));
        let engine = Engine::new(options().with_resolver(resolver.clone()));
        let plan = compile(&engine);
        let mut first = plan
            .run(&RunRequest::new(), &CancellationToken::new())
            .unwrap();
        first.save(&CancellationToken::new()).unwrap();
        assert_eq!(resolver.published(SAVED).as_deref(), Some(STUDY_A));
        // A new object version behind the same path: had the second run kept
        // the first run's snapshot, its capture would be refused as changed.
        resolver.insert("/spec.yaml", "input.csv", "data:2", Arc::from(STUDY_B));
        let mut second = plan
            .run(&RunRequest::new(), &CancellationToken::new())
            .unwrap();
        assert!(second.report().is_empty(), "{:?}", second.report());
        second.save(&CancellationToken::new()).unwrap();
        assert_eq!(resolver.published(SAVED).as_deref(), Some(STUDY_B));
    }

    #[test]
    fn concurrent_runs_on_one_plan_are_independent() {
        let engine = Engine::new(options().with_resolver(specification()));
        let plan = compile(&engine);
        std::thread::scope(|threads| {
            let runs = (0..8)
                .map(|index| {
                    let plan = plan.clone();
                    threads.spawn(move || {
                        let bytes = if index % 2 == 0 { STUDY_A } else { STUDY_B };
                        let saved = run_and_save(&plan, study(&format!("study:{index}"), bytes));
                        (saved, bytes)
                    })
                })
                .collect::<Vec<_>>();
            for run in runs {
                let (saved, bytes) = run.join().unwrap();
                assert_eq!(saved, bytes);
            }
        });
    }

    #[test]
    fn a_build_outlives_its_plan_and_engine() {
        let resolver = study("study:a", STUDY_A);
        let mut build = {
            let engine = Engine::new(options().with_resolver(specification()));
            let plan = compile(&engine);
            plan.run(
                &RunRequest::new().with_resolver(resolver.clone()),
                &CancellationToken::new(),
            )
            .unwrap()
        };
        assert_eq!(build.output().unwrap().rows(), 2);
        build.save(&CancellationToken::new()).unwrap();
        assert_eq!(resolver.published(SAVED).as_deref(), Some(STUDY_A));
    }

    #[test]
    fn a_build_emits_the_compile_events_followed_by_the_run_events() {
        let events = Arc::new(Events::default());
        let resolver = specification();
        resolver.insert("/spec.yaml", "input.csv", "data:1", Arc::from(STUDY_A));
        let engine = Engine::new(
            options()
                .with_resolver(resolver)
                .with_observer(events.clone()),
        );
        let token = CancellationToken::new();
        engine.build(&Request::new("spec.yaml"), &token).unwrap();
        let built = events.take();
        let plan = engine.compile(&Request::new("spec.yaml"), &token).unwrap();
        let mut separate = events.take();
        assert_eq!(
            separate,
            [
                "Phase { phase: Preparation }",
                "Exit { phase: Preparation, outcome: Succeeded }",
            ]
        );
        plan.run(&RunRequest::new(), &token).unwrap();
        separate.extend(events.take());
        assert_eq!(built, separate);
        assert_eq!(built.last().unwrap(), "Phase { phase: Finished }");
        // A run's own observer replaces the engine's for that run.
        let own = Arc::new(Events::default());
        plan.run(&RunRequest::new().with_observer(own.clone()), &token)
            .unwrap();
        assert!(events.take().is_empty());
        assert_eq!(own.take(), separate[2..]);
    }

    #[test]
    fn a_refused_compile_returns_a_plan_whose_runs_repeat_its_report_without_io() {
        let resolver = Arc::new(MemoryResolver::new());
        resolver.insert(
            "/",
            "spec.yaml",
            "spec:1",
            Arc::from(&b"schema_version: '1.0'\ndomain: TEST\nunknown_field: true\n"[..]),
        );
        let events = Arc::new(Events::default());
        let engine = Engine::new(
            options()
                .with_resolver(resolver)
                .with_observer(events.clone()),
        );
        let token = CancellationToken::new();
        let built = engine.build(&Request::new("spec.yaml"), &token).unwrap();
        let build_events = events.take();
        let plan = engine.compile(&Request::new("spec.yaml"), &token).unwrap();
        assert!(!plan.report().is_empty());
        assert_eq!(plan.report(), built.report());
        let recording = Arc::new(Recording::default());
        for _ in 0..2 {
            let mut run = plan
                .run(&RunRequest::new().with_resolver(recording.clone()), &token)
                .unwrap();
            assert_eq!(run.report(), plan.report());
            assert!(run.output().is_none());
            assert_eq!(run.preparation_sources(), built.preparation_sources());
            assert!(run.save(&CancellationToken::new()).is_err());
        }
        assert!(recording.requests.lock().unwrap().is_empty());
        // The refused build's events are the compile's, then a bare finish.
        let mut compile_and_runs = events.take();
        assert_eq!(compile_and_runs.len(), build_events.len() + 1);
        compile_and_runs.pop();
        assert_eq!(compile_and_runs, build_events);
        assert_eq!(build_events.last().unwrap(), "Phase { phase: Finished }");
    }
}
