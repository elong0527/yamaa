//! One request composition selects held metadata before any execution authority.
use std::{cell::RefCell, rc::Rc};
use yamaa_core::schema::{Document, DocumentNode as N};
use yamaa_engine::{
    build::{self, EntrySelection, MetadataAdmissionPort, PreparationPort, Prepared},
    domain_entry::{self, Ports, Request},
};

struct Original(Rc<()>);
struct Entry(EntrySelection);
#[derive(Default)]
struct Fake {
    events: Vec<&'static str>,
    producer: bool,
    failure: Option<(&'static str, Original)>,
    phases: Option<Rc<RefCell<Vec<yamaa_engine::application_observer::Phase>>>>,
}
impl Fake {
    fn enter(&mut self, event: &'static str) -> Result<(), Original> {
        if let Some(phases) = &self.phases {
            use yamaa_engine::application_observer::Phase;
            let expected = if self
                .events
                .iter()
                .any(|event| matches!(*event, "plain build" | "project build" | "graph build"))
                || matches!(event, "plain build" | "project build" | "graph build")
            {
                Phase::Input
            } else {
                Phase::Preparation
            };
            assert_eq!(phases.borrow().last(), Some(&expected));
        }
        self.events.push(event);
        if self
            .failure
            .as_ref()
            .is_some_and(|(stage, _)| *stage == event)
        {
            return Err(self.failure.take().unwrap().1);
        }
        Ok(())
    }
    fn execute(&mut self, route: &'static str) -> Result<(), Original> {
        self.enter(route)?;
        self.enter("lock")?;
        self.enter("function tests")?;
        self.enter("study capture")?;
        self.enter("decode")?;
        self.enter("execute")?;
        self.enter("encode")?;
        self.enter("report")
    }
}
impl MetadataAdmissionPort for Fake {
    type Error = Original;
    fn charge_work(&mut self, _: usize) -> Result<(), Original> {
        self.enter("original arena")
    }
    fn verify_metadata(&mut self) -> Result<(), (String, Original)> {
        self.enter("snapshot verification")
            .map_err(|e| ("spec.yaml".into(), e))
    }
}
impl PreparationPort for Fake {
    type Entry = Entry;
    type Plain = ();
    type Project = ();
    type Producer = ();
    type Failure = Original;
    fn capture(&mut self, request: Request<'_>) -> Result<Entry, Original> {
        assert_eq!(request.specification, "spec.yaml");
        self.enter("root admission")?;
        if let Some(path) = request.environment {
            assert_eq!(path, "environment.yaml");
            self.enter("environment")?;
        }
        self.enter("entry")?;
        let document = Document::new(
            vec![
                N::Text("input".into()),
                N::Text("SRC".into()),
                N::Text("schema".into()),
                if self.producer {
                    N::Text("producer.yaml".into())
                } else {
                    N::Null
                },
                N::Mapping(vec![(2, 3)]),
                N::Mapping(vec![(1, 4)]),
                N::Mapping(vec![(0, 5)]),
            ],
            6,
            Default::default(),
        )
        .unwrap();
        let selected = build::select_entry(
            build::EntryView {
                original: &document,
                model_nodes: 7,
            },
            16,
            self,
        )
        .map_err(|error| match error {
            build::SelectionFailure::Resource(e)
            | build::SelectionFailure::Snapshot { error: e, .. } => e,
            _ => panic!("fixture capacity"),
        })?;
        Ok(Entry(selected))
    }
    fn selection(entry: &Entry) -> &EntrySelection {
        &entry.0
    }
    fn plain(&mut self, _: Entry) -> Result<(), Original> {
        self.enter("plain compilation")
    }
    fn project(&mut self, _: Entry) -> Result<(), Original> {
        self.enter("project compilation")
    }
    fn producer(&mut self, _: Entry) -> Result<(), Original> {
        self.enter("graph compilation")
    }
}
impl Ports for Fake {
    type Plain = ();
    type Project = ();
    type Producer = ();
    type CheckResult = ();
    type BuildResult = ();
    type Failure = Original;
    fn prepare(&mut self, request: Request<'_>) -> Result<Prepared<(), (), ()>, Original> {
        build::prepare(request, self)
    }
    fn check_plain(&mut self, _: ()) -> Result<(), Original> {
        self.enter("plain check")
    }
    fn check_project(&mut self, _: ()) -> Result<(), Original> {
        self.enter("project check")
    }
    fn check_producer(&mut self, _: ()) -> Result<(), Original> {
        self.enter("graph check")
    }
    fn build_plain(&mut self, _: ()) -> Result<(), Original> {
        self.execute("plain build")
    }
    fn build_project(&mut self, _: ()) -> Result<(), Original> {
        self.execute("project build")
    }
    fn build_producer(&mut self, _: ()) -> Result<(), Original> {
        self.execute("graph build")
    }
}
fn request(environment: bool) -> Request<'static> {
    Request {
        specification: "spec.yaml",
        environment: environment.then_some("environment.yaml"),
    }
}

#[test]
fn observed_request_brackets_actual_metadata_before_every_route_and_keeps_original_failure() {
    use yamaa_engine::application_observer::{Event, Observer, Phase, Scope};
    struct Stages(Rc<RefCell<Vec<Phase>>>);
    impl Observer for Stages {
        fn observe(&self, event: Event<'_>) {
            let Event::Phase { node: None, phase } = event else {
                panic!("this request spy invokes no real function port")
            };
            self.0.borrow_mut().push(phase);
        }
    }
    for producer in [false, true] {
        for environment in [false, true] {
            for failure in [None, Some("entry"), Some("study capture")] {
                let marker = Rc::new(());
                let phases = Rc::new(RefCell::new(vec![]));
                let observer = Stages(Rc::clone(&phases));
                let mut port = Fake {
                    producer,
                    failure: failure.map(|stage| (stage, Original(Rc::clone(&marker)))),
                    phases: Some(Rc::clone(&phases)),
                    ..Default::default()
                };
                let result = domain_entry::domain_with_observer(
                    request(environment),
                    &mut port,
                    Scope::new(&observer),
                );
                match failure {
                    None => assert!(result.is_ok()),
                    Some(stage) => {
                        let Err(error) = result else {
                            panic!("scripted original failure")
                        };
                        assert!(Rc::ptr_eq(&marker, &error.0));
                        assert_eq!(port.events.last(), Some(&stage));
                    }
                }
                let expected = if failure == Some("entry") {
                    vec![Phase::Preparation, Phase::Finished]
                } else {
                    vec![Phase::Preparation, Phase::Input, Phase::Finished]
                };
                assert_eq!(*phases.borrow(), expected);
            }
        }
    }
}
#[test]
fn both_entry_points_select_all_routes_from_the_original_held_entry() {
    for (producer, environment, compilation, check, build) in [
        (
            false,
            false,
            "plain compilation",
            "plain check",
            "plain build",
        ),
        (
            false,
            true,
            "project compilation",
            "project check",
            "project build",
        ),
        (
            true,
            false,
            "graph compilation",
            "graph check",
            "graph build",
        ),
        (
            true,
            true,
            "graph compilation",
            "graph check",
            "graph build",
        ),
    ] {
        let mut port = Fake {
            producer,
            ..Default::default()
        };
        assert!(domain_entry::check(request(environment), &mut port).is_ok());
        let mut expected = vec!["root admission"];
        if environment {
            expected.push("environment");
        }
        expected.extend([
            "entry",
            "original arena",
            "snapshot verification",
            compilation,
            check,
        ]);
        assert_eq!(port.events, expected);
        port.events.clear();
        assert!(domain_entry::domain(request(environment), &mut port).is_ok());
        expected.pop();
        expected.extend([
            build,
            "lock",
            "function tests",
            "study capture",
            "decode",
            "execute",
            "encode",
            "report",
        ]);
        assert_eq!(port.events, expected);
    }
}
#[test]
fn every_failure_retains_its_original_payload_and_stops_later_authority() {
    for producer in [false, true] {
        for stage in [
            "root admission",
            "environment",
            "entry",
            "original arena",
            "snapshot verification",
            if producer {
                "graph compilation"
            } else {
                "project compilation"
            },
            "lock",
            "function tests",
            "study capture",
            "decode",
            "execute",
            "encode",
            "report",
        ] {
            let marker = Rc::new(());
            let mut port = Fake {
                producer,
                failure: Some((stage, Original(Rc::clone(&marker)))),
                ..Default::default()
            };
            let Err(error) = domain_entry::domain(request(true), &mut port) else {
                panic!("scripted failure")
            };
            assert!(Rc::ptr_eq(&marker, &error.0));
            assert_eq!(port.events.last(), Some(&stage));
            if matches!(
                stage,
                "root admission"
                    | "environment"
                    | "entry"
                    | "original arena"
                    | "snapshot verification"
                    | "graph compilation"
                    | "project compilation"
                    | "lock"
                    | "function tests"
            ) {
                assert!(!port.events.contains(&"study capture"));
            }
        }
    }
}
