#![cfg(feature = "yaml")]
//! One prepared value runs many times, each run in its own scope (#2821 step 1).
//!
//! Every route's prepared value -- plain, project and producer -- runs twice
//! on different studies, fails and then succeeds, and never sees a snapshot an
//! earlier run captured. Expected artifacts are written here from the authored
//! inputs: the output declares exactly the input's columns, so each study's
//! saved CSV is its input text.
use std::sync::Arc;

use yamaa_adapters::{
    file_application::{self, Domain},
    file_graph::FileGraph,
    file_preparation::FileSpecification,
    file_project::FileProject,
    file_resources::RunScope,
    file_workflow,
    host_resources::{MemoryResolver, Resolver},
    issue_rows::Issue,
    producer_result::{self, RootResult},
    project_report,
    specification_report::Identity,
};
use yamaa_core::{
    application_observer::Scope,
    build::{Prepared, Request},
    function_invocation::{Argument, HostError},
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_activation::ActivationPort,
    project_environment::LockReference,
    project_function::Language,
    value::Value,
};

const PLAIN: &[u8] = b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {RAW: {path: raw.csv, types: {ID: int}}}\ncolumns:\n  - {name: ID, type: int, derivation: RAW.ID}\n  - {name: NAME, type: str, derivation: RAW.NAME}\noutput: {path: out.csv, columns: [ID, NAME]}\n";
const ENVIRONMENT: &[u8] = b"schema_version: '1.0'\n";
const PRODUCER: &[u8] = b"schema_version: '1.0'\ndomain: PRODUCER\nkeys: [ID]\ninput: {RAW: {path: raw.csv, types: {ID: int}}}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\n  - {name: NAME, type: str, label: Name, derivation: RAW.NAME}\noutput: {path: produced.csv, columns: [ID, NAME]}\n";
const CONSUMER: &[u8] = b"schema_version: '1.0'\ndomain: CONSUMER\nkeys: [ID]\ninput: {P: {path: produced.csv, schema: producer.yaml}}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: P.ID}\n  - {name: NAME, type: str, label: Name, derivation: P.NAME}\noutput: {path: final.csv, columns: [ID, NAME]}\n";

const STUDY_A: &[u8] = b"ID,NAME\n1,alpha\n2,beta\n";
const STUDY_B: &[u8] = b"ID,NAME\n7,gamma\n";

fn identity() -> Identity<'static> {
    Identity {
        runtime: "rust",
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example: "run-scopes",
        specification: "spec.yaml",
        base_directory: ".",
    }
}

/// Specifications hold no study; each study resolver holds only its input.
#[derive(Clone, Copy, PartialEq)]
enum Route {
    Plain,
    Project,
    Producer,
}

impl Route {
    /// The declaring identity of the one study input.
    fn input_owner(self) -> &'static str {
        if self == Route::Producer {
            "/producer.yaml"
        } else {
            "/spec.yaml"
        }
    }
    /// The saved artifacts a successful run publishes, in Save order.
    fn targets(self) -> &'static [&'static str] {
        match self {
            Route::Producer => &["memory:/produced.csv", "memory:/final.csv"],
            _ => &["memory:/out.csv"],
        }
    }
    fn compile_resolver(self) -> Arc<MemoryResolver> {
        let resolver = Arc::new(MemoryResolver::default());
        if self == Route::Producer {
            resolver.insert("/", "spec.yaml", "spec:1", Arc::from(CONSUMER));
            resolver.insert(
                "/spec.yaml",
                "producer.yaml",
                "producer:1",
                Arc::from(PRODUCER),
            );
        } else {
            resolver.insert("/", "spec.yaml", "spec:1", Arc::from(PLAIN));
        }
        if self == Route::Project {
            resolver.insert("/", "environment.yaml", "env:1", Arc::from(ENVIRONMENT));
        }
        resolver
    }
    fn study(self, reference: &str, bytes: &'static [u8]) -> Arc<MemoryResolver> {
        let resolver = Arc::new(MemoryResolver::default());
        resolver.insert(self.input_owner(), "raw.csv", reference, Arc::from(bytes));
        resolver
    }
    fn compile(self, resolver: Arc<MemoryResolver>) -> Compiled {
        let selected = file_workflow::select_with_resolver(
            Request {
                specification: "spec.yaml",
                environment: (self == Route::Project).then_some("environment.yaml"),
            },
            Language::Python,
            resolver,
        )
        .unwrap_or_else(|error| panic!("compile refused: {error:?}"));
        match (self, selected) {
            (Route::Plain, Prepared::Plain(prepared)) => Compiled::Plain(Box::new(prepared)),
            (Route::Project, Prepared::Ordinary(prepared)) => Compiled::Project(prepared),
            (Route::Producer, Prepared::Producer(prepared)) => Compiled::Producer(prepared),
            _ => panic!("unexpected route"),
        }
    }
}

/// A prepared value of one route. It is only ever borrowed by its runs.
enum Compiled {
    Plain(Box<FileSpecification>),
    Project(Box<FileProject>),
    Producer(Box<FileGraph>),
}

/// One run's public result, whichever route produced it.
enum Outcome {
    Domain(Domain),
    Root(RootResult),
    Refused(Vec<Issue>),
}

impl Outcome {
    fn issues(&self) -> Vec<Issue> {
        match self {
            Outcome::Domain(domain) => domain.issues(),
            Outcome::Root(root) => root.issues().to_vec(),
            Outcome::Refused(issues) => issues.clone(),
        }
    }
    fn has_output(&self) -> bool {
        match self {
            Outcome::Domain(domain) => domain.output().is_some(),
            Outcome::Root(root) => root.output().is_some(),
            Outcome::Refused(_) => false,
        }
    }
    fn save(&mut self) -> bool {
        match self {
            Outcome::Domain(domain) => domain.save().unwrap(),
            Outcome::Root(root) => root.save().unwrap(),
            Outcome::Refused(_) => panic!("a refused run cannot save"),
        }
    }
}

/// No study here calls a project function; activation never reaches the host.
struct NoActivation;
impl ActivationPort for NoActivation {
    type Handle = ();
    type Error = String;
    fn verify_lock(
        &mut self,
        _: Language,
        _: &LockReference,
        _: &[ProjectFunctionIdentity],
    ) -> Result<(), String> {
        panic!("no lock is declared")
    }
    fn bind(&mut self, _: &ProjectFunctionIdentity, _: &LogicalSignature) -> Result<(), String> {
        panic!("no function is declared")
    }
    fn invoke(&mut self, _: &(), _: &[Argument<'_>]) -> Result<Value, HostError<String>> {
        panic!("no function is called")
    }
}

impl Compiled {
    fn run_scope(&self, resolver: Option<Arc<dyn Resolver>>) -> RunScope {
        match self {
            Compiled::Plain(prepared) => prepared.run_scope(resolver),
            Compiled::Project(project) => project.run_scope(resolver),
            Compiled::Producer(graph) => graph.build().run_scope(resolver),
        }
    }
    /// Run once in `run`, borrowing this prepared value.
    fn run(&self, run: &mut RunScope) -> Outcome {
        match self {
            Compiled::Plain(prepared) => {
                match file_application::plain(prepared, run, identity(), Scope::default()) {
                    Ok(domain) => Outcome::Domain(domain),
                    Err(issues) => Outcome::Refused(issues),
                }
            }
            Compiled::Project(project) => {
                let attempt = project.build(run, &mut NoActivation, Scope::default());
                let result = project_report::build_result(project.run(), &attempt, identity(), &[])
                    .unwrap_or_else(|_| panic!("project projection refused"));
                Outcome::Domain(Domain::from_project(result, project, run).unwrap())
            }
            Compiled::Producer(graph) => {
                let owned = graph.build().build_owned_reported(
                    run,
                    &mut NoActivation,
                    identity(),
                    Scope::default(),
                );
                match producer_result::prepare_owned_classified(&owned, identity(), &[]) {
                    Ok(root) => Outcome::Root(root),
                    Err(error) => panic!("producer projection refused: {error:?}"),
                }
            }
        }
    }
}

const ROUTES: [Route; 3] = [Route::Plain, Route::Project, Route::Producer];

/// A run that saved returns exactly its study's text at every target.
fn assert_saved(route: Route, resolver: &MemoryResolver, study: &[u8]) {
    for target in route.targets() {
        assert_eq!(
            resolver.published(target).as_deref(),
            Some(study),
            "{target}"
        );
    }
}

#[test]
fn each_route_runs_two_studies_from_one_prepared_value_with_their_own_results() {
    for route in ROUTES {
        let compiled = route.compile(route.compile_resolver());
        for (study, bytes) in [("a", STUDY_A), ("b", STUDY_B), ("a", STUDY_A)] {
            let resolver = route.study(&format!("study:{study}"), bytes);
            let mut run = compiled.run_scope(Some(resolver.clone()));
            let mut outcome = compiled.run(&mut run);
            assert!(outcome.issues().is_empty(), "{:?}", outcome.issues());
            assert!(outcome.has_output());
            assert!(outcome.save());
            // Save targets resolve from this run's resolver alone.
            assert_saved(route, &resolver, bytes);
            // The run's retained evidence lists the compile metadata, then
            // this study's input: never another study's bytes.
            let retained = run.retained_sources();
            assert!(retained.iter().any(|(_, held)| &held[..] == bytes));
            let other = if bytes == STUDY_A { STUDY_B } else { STUDY_A };
            assert!(retained.iter().all(|(_, held)| &held[..] != other));
        }
    }
}

#[test]
fn each_route_fails_a_run_then_succeeds_on_the_same_prepared_value() {
    for route in ROUTES {
        let compiled = route.compile(route.compile_resolver());
        // An empty study resolver: the declared input reaches no entry.
        let missing = Arc::new(MemoryResolver::default());
        let failed = compiled.run(&mut compiled.run_scope(Some(missing.clone())));
        assert!(!failed.has_output());
        let issues = failed.issues();
        let issue = &issues[0];
        assert_eq!(issue.phase, "validation");
        assert_eq!(issue.condition, "resource_path_missing");
        assert_eq!(issue.requirement.as_deref(), Some("REQ-0785"));
        assert_eq!(issue.spec_paths, ["input.RAW.path"]);
        assert_eq!(
            issue.context,
            yamaa_adapters::json!({"dataset": "RAW", "path": "raw.csv"})
        );
        assert!(route
            .targets()
            .iter()
            .all(|target| missing.published(target).is_none()));
        let resolver = route.study("study:b", STUDY_B);
        let mut good = compiled.run(&mut compiled.run_scope(Some(resolver.clone())));
        assert!(good.issues().is_empty(), "{:?}", good.issues());
        assert!(good.save());
        assert_saved(route, &resolver, STUDY_B);
    }
}

#[test]
fn a_second_run_through_the_compile_authority_holds_no_snapshot_of_the_first() {
    for route in ROUTES {
        let resolver = route.compile_resolver();
        resolver.insert(route.input_owner(), "raw.csv", "data:1", Arc::from(STUDY_A));
        let compiled = route.compile(resolver.clone());
        let mut first = compiled.run_scope(None);
        let mut outcome = compiled.run(&mut first);
        assert!(outcome.issues().is_empty(), "{:?}", outcome.issues());
        assert!(outcome.save());
        assert_saved(route, &resolver, STUDY_A);
        // A new object version replaces the study input behind the same path.
        resolver.insert(route.input_owner(), "raw.csv", "data:2", Arc::from(STUDY_B));
        // The first run's scope still holds its snapshot, so building in it
        // again refuses the replacement instead of reading it (REQ-0788).
        let stale = compiled.run(&mut first);
        assert!(!stale.has_output());
        assert_eq!(
            stale.issues()[0].condition,
            "resource_path_content_changed",
            "{:?}",
            stale.issues()
        );
        // A fresh scope starts from the compile snapshots only: it captures
        // the new version and sees nothing the first run captured.
        let mut second = compiled.run_scope(None);
        assert!(second
            .retained_sources()
            .iter()
            .all(|(_, held)| &held[..] != STUDY_A));
        let mut outcome = compiled.run(&mut second);
        assert!(outcome.issues().is_empty(), "{:?}", outcome.issues());
        assert!(outcome.save());
        assert_saved(route, &resolver, STUDY_B);
        let retained = second.retained_sources();
        assert!(retained.iter().any(|(_, held)| &held[..] == STUDY_B));
        assert!(retained.iter().all(|(_, held)| &held[..] != STUDY_A));
    }
}
