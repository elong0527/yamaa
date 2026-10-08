use yamaa_engine::domain_entry::{self, Ports, Request};

#[derive(Default)]
struct Fake {
    events: Vec<&'static str>,
    failure: Option<&'static str>,
}
impl Fake {
    fn enter(&mut self, event: &'static str) -> Result<(), &'static str> {
        self.events.push(event);
        if self.failure == Some(event) {
            Err(event)
        } else {
            Ok(())
        }
    }
}
impl Ports for Fake {
    type Environment = ();
    type Prepared = ();
    type CheckResult = ();
    type BuildResult = ();
    type Failure = &'static str;
    fn environment(&mut self, path: Option<&str>) -> Result<(), &'static str> {
        assert_eq!(path, Some("environment.yaml"));
        self.enter("environment")
    }
    fn prepare(&mut self, path: &str) -> Result<(), &'static str> {
        assert_eq!(path, "spec.yaml");
        self.enter("unsupported admission")
    }
    fn check(&mut self, _: (), _: &()) -> Result<(), &'static str> {
        self.enter("static check")
    }
    fn activate(&mut self, _: &(), _: &()) -> Result<(), &'static str> {
        self.enter("lock")?;
        self.enter("function tests")
    }
    fn build(&mut self, _: (), _: &()) -> Result<(), &'static str> {
        self.enter("study")
    }
}
fn request() -> Request<'static> {
    Request {
        specification: "spec.yaml",
        environment: Some("environment.yaml"),
    }
}

#[test]
fn every_admission_failure_stops_before_study_authority() {
    for (failure, expected) in [
        ("environment", vec!["environment"]),
        (
            "unsupported admission",
            vec!["environment", "unsupported admission"],
        ),
        ("lock", vec!["environment", "unsupported admission", "lock"]),
        (
            "function tests",
            vec![
                "environment",
                "unsupported admission",
                "lock",
                "function tests",
            ],
        ),
    ] {
        let mut port = Fake {
            failure: Some(failure),
            ..Default::default()
        };
        assert_eq!(domain_entry::domain(request(), &mut port), Err(failure));
        assert_eq!(port.events, expected);
    }
}

#[test]
fn checks_never_activate_code_or_enter_study_and_builds_revalidate_every_time() {
    let mut port = Fake::default();
    domain_entry::check(request(), &mut port).unwrap();
    assert_eq!(
        port.events,
        ["environment", "unsupported admission", "static check"]
    );
    port.events.clear();
    for _ in 0..2 {
        domain_entry::domain(request(), &mut port).unwrap();
    }
    assert_eq!(
        port.events,
        [
            "environment",
            "unsupported admission",
            "lock",
            "function tests",
            "study",
            "environment",
            "unsupported admission",
            "lock",
            "function tests",
            "study"
        ]
    );
}
