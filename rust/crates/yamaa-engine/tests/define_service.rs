//! Independently authored engine use-case controls; no native filesystem or host.
use std::{cell::Cell, rc::Rc};
use yamaa_core::{
    diagnostic::ConditionCode,
    project_terminology::Catalogue,
    schema::{Document, DocumentNode as N},
    table::{CellError, TableAccess, TableSchema, ValueRef},
};
use yamaa_engine::define::{self, Application, Failure, Ports, PublicationState, SaveFailure};

#[derive(Debug)]
struct Opaque(Box<u8>);
impl Opaque {
    fn identity(&self) -> *const u8 {
        &*self.0
    }
}
struct Environment {
    document: Document,
    catalogue: Catalogue,
}
struct EmptyTable(TableSchema);
impl TableAccess for EmptyTable {
    type Error = ();
    fn schema(&self) -> &TableSchema {
        &self.0
    }
    fn row_count(&self) -> usize {
        0
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<()>> {
        Err(CellError::OutOfBounds { row, column })
    }
}
enum Tree {
    Text(&'static str),
    Map(Vec<(&'static str, Tree)>),
    Sequence(Vec<Tree>),
}
fn text(value: &'static str) -> Tree {
    Tree::Text(value)
}
fn document(tree: Tree) -> Document {
    fn enter(tree: Tree, nodes: &mut Vec<N>) -> usize {
        let node = match tree {
            Tree::Text(value) => N::Text(value.into()),
            Tree::Sequence(values) => N::Sequence(
                values
                    .into_iter()
                    .map(|value| enter(value, nodes))
                    .collect(),
            ),
            Tree::Map(fields) => N::Mapping(
                fields
                    .into_iter()
                    .map(|(key, value)| {
                        let key = enter(text(key), nodes);
                        (key, enter(value, nodes))
                    })
                    .collect(),
            ),
        };
        nodes.push(node);
        nodes.len() - 1
    }
    let mut nodes = Vec::new();
    let root = enter(tree, &mut nodes);
    Document::new(nodes, root, Default::default()).unwrap()
}
fn environment(with_study: bool) -> Environment {
    let mut fields = vec![(
        "sdtm",
        Tree::Map(vec![
            (
                "standard",
                Tree::Map(vec![
                    ("name", text("SDTMIG")),
                    ("version", text("3.4")),
                    ("status", text("Final")),
                ]),
            ),
            ("specs", Tree::Sequence(vec![])),
            ("define", text("define.xml")),
        ]),
    )];
    if with_study {
        fields.push((
            "study",
            Tree::Map(vec![
                ("id", text("STUDY01")),
                ("name", text("Study")),
                ("description", text("Independent service fixture")),
                ("protocol_name", text("Protocol")),
            ]),
        ));
    }
    Environment {
        document: document(Tree::Map(fields)),
        catalogue: Catalogue::admit(vec![]).unwrap(),
    }
}
struct Spy {
    environment: Option<Environment>,
    events: Vec<&'static str>,
    seconds: Option<u64>,
    environment_error: Option<Opaque>,
    verification_error: Option<Opaque>,
    publication_error: Option<Opaque>,
    publisher_error: Option<(usize, Opaque)>,
    publishers: usize,
    published: Vec<(String, Vec<u8>)>,
    clock_calls: Rc<Cell<usize>>,
}
impl Spy {
    fn new() -> Self {
        Self {
            environment: Some(environment(true)),
            events: Vec::new(),
            seconds: Some(0),
            environment_error: None,
            verification_error: None,
            publication_error: None,
            publisher_error: None,
            publishers: 0,
            published: Vec::new(),
            clock_calls: Rc::new(Cell::new(0)),
        }
    }
}
impl Ports for Spy {
    type Environment = Environment;
    type Specification = Document;
    type Error = Opaque;
    type PublicationError = Opaque;
    type Bytes = Vec<u8>;
    type Parquet = EmptyTable;
    type ParquetError = ();
    type Target = String;
    type Publisher = String;
    fn utc_seconds(&mut self) -> Option<u64> {
        self.clock_calls.set(self.clock_calls.get() + 1);
        self.seconds
    }
    fn capture_environment(
        &mut self,
        written: &str,
        limits: define::MetadataLimits,
    ) -> Result<Environment, Opaque> {
        assert_eq!(written, "environment.yaml");
        assert_eq!(limits, define::METADATA_LIMITS);
        self.events.push("capture_environment");
        if let Some(error) = self.environment_error.take() {
            return Err(error);
        }
        Ok(self.environment.take().unwrap())
    }
    fn environment(environment: &Environment) -> define::EnvironmentView<'_> {
        define::EnvironmentView {
            document: &environment.document,
            catalogue: &environment.catalogue,
            source: "/project/environment.yaml",
        }
    }
    fn capture_specification(&mut self, _: &str, _: &str) -> Result<Document, Opaque> {
        panic!("no spec selected")
    }
    fn specification(_: &Document) -> define::SpecificationView<'_> {
        panic!("no spec selected")
    }
    fn location(&self, path: &str, declaring: &str, written: &str) -> Result<String, Opaque> {
        assert_eq!(path, "sdtm.define");
        assert_eq!(declaring, "/project/environment.yaml");
        assert_eq!(written, "define.xml");
        Ok("/project/define.xml".into())
    }
    fn json_written(&self, _: &str) -> String {
        panic!("no dataset selected")
    }
    fn same_location(&self, left: &str, right: &str) -> bool {
        left == right
    }
    fn same_parent(&self, _: &str, _: &str) -> bool {
        panic!("no dataset selected")
    }
    fn package_reference(&self, _: &str, _: &str) -> Option<String> {
        panic!("no dataset selected")
    }
    fn define_name(&self, _: &str) -> String {
        panic!("no dataset selected")
    }
    fn verify_metadata(&mut self, path: &str, source: &str) -> Result<(), Opaque> {
        assert_eq!(path, "sdtm");
        assert_eq!(source, "/project/environment.yaml");
        self.events.push("verify_metadata");
        self.verification_error.take().map_or(Ok(()), Err)
    }
    fn finish_metadata(&mut self) {
        self.events.push("finish_metadata");
    }
    fn capture_published(
        &mut self,
        _: &str,
        _: &str,
        _: &str,
        _: usize,
    ) -> Result<Vec<u8>, Opaque> {
        panic!("no dataset selected")
    }
    fn decode_parquet(&mut self, _: &[u8]) -> Result<EmptyTable, ()> {
        panic!("no codec selected")
    }
    fn target(&self, path: &str, declaring: &str, written: &str) -> Result<String, Opaque> {
        assert_eq!(
            (path, declaring, written),
            ("sdtm.define", "/project/environment.yaml", "define.xml")
        );
        Ok("/project/define.xml".into())
    }
    fn publisher(&mut self, target: &String, written: &str) -> Result<String, Opaque> {
        assert_eq!(written, "define.xml");
        self.events.push("publisher");
        self.publishers += 1;
        if self
            .publisher_error
            .as_ref()
            .is_some_and(|(call, _)| *call == self.publishers)
        {
            return Err(self.publisher_error.take().unwrap().1);
        }
        Ok(target.clone())
    }
    fn same_target(&mut self, left: &String, right: &String) -> Result<bool, Opaque> {
        Ok(left == right)
    }
    fn publish(
        &mut self,
        publisher: &mut String,
        written: &str,
        bytes: &[u8],
    ) -> Result<(), Opaque> {
        assert_eq!(publisher, "/project/define.xml");
        self.events.push("publish");
        if let Some(error) = self.publication_error.take() {
            return Err(error);
        }
        self.published.push((written.into(), bytes.into()));
        Ok(())
    }
}
fn prepare(spy: &mut Spy, standard: &str, created: Option<&str>) -> define::Attempt<Spy> {
    Application::prepare(
        spy,
        define::Request {
            environment: "environment.yaml",
            standard,
            created,
            installed: "0.3.0",
        },
    )
}
#[test]
fn standard_and_explicit_creation_admit_before_any_capture_or_clock() {
    for (standard, created, code) in [
        ("other", None, ConditionCode::DefineStandardName),
        (
            "sdtm",
            Some("2026-10-11T00:00:00.5"),
            ConditionCode::DefineCreatedInvalid,
        ),
        (
            "sdtm",
            Some("2026-02-30T00:00:00"),
            ConditionCode::DefineCreatedInvalid,
        ),
    ] {
        let mut spy = Spy::new();
        let mut attempt = prepare(&mut spy, standard, created);
        let Err(Failure::Metadata(findings)) = attempt.outcome else {
            panic!("missing admission cause")
        };
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, code);
        assert!(spy.events.is_empty());
        assert_eq!(spy.clock_calls.get(), 0);
        assert!(matches!(
            attempt.application.save(&mut spy),
            Err(SaveFailure::Refused)
        ));
        assert!(spy.events.is_empty());
    }
}
#[test]
fn actual_capture_failure_moves_opaque_payload_once_and_refuses_save_without_retry() {
    let mut spy = Spy::new();
    let error = Opaque(Box::new(42));
    let identity = error.identity();
    spy.environment_error = Some(error);
    let mut attempt = prepare(&mut spy, "sdtm", Some("2026-10-11T00:00:00"));
    let Err(Failure::Port(error)) = attempt.outcome else {
        panic!("original port error lost")
    };
    assert_eq!(error.identity(), identity);
    assert_eq!(spy.events, ["capture_environment"]);
    assert!(matches!(
        attempt.application.save(&mut spy),
        Err(SaveFailure::Refused)
    ));
    assert_eq!(spy.events, ["capture_environment"]);
    assert_eq!(attempt.application.artifacts().len(), 0);
}
#[test]
fn complete_metadata_and_rendering_precede_publication_and_save_is_one_shot() {
    let mut spy = Spy::new();
    let mut attempt = prepare(&mut spy, "sdtm", Some("2026-10-11T00:00:00"));
    assert!(attempt.outcome.is_ok());
    assert_eq!(
        spy.events,
        [
            "capture_environment",
            "verify_metadata",
            "finish_metadata",
            "publisher"
        ]
    );
    assert_eq!(spy.clock_calls.get(), 0);
    assert!(spy.published.is_empty());
    let held = attempt.application.artifacts().next().unwrap();
    assert_eq!(held.0, "define.xml");
    assert_eq!(held.2, PublicationState::Ready);
    assert!(std::str::from_utf8(held.1)
        .unwrap()
        .contains("CreationDateTime=\"2026-10-11T00:00:00\""));
    let bytes = held.1.to_vec();
    assert!(attempt.application.save(&mut spy).ok().unwrap());
    assert_eq!(spy.published, [("define.xml".into(), bytes)]);
    assert_eq!(
        attempt.application.artifacts().next().unwrap().2,
        PublicationState::Published
    );
    let events = spy.events.clone();
    assert!(!attempt.application.save(&mut spy).ok().unwrap());
    assert_eq!(spy.events, events);
    assert_eq!(spy.publishers, 2);
}
#[test]
fn verification_failure_retains_original_payload_and_never_acquires_a_publisher() {
    let mut spy = Spy::new();
    let error = Opaque(Box::new(17));
    let identity = error.identity();
    spy.verification_error = Some(error);
    let attempt = prepare(&mut spy, "sdtm", Some("2026-10-11T00:00:00"));
    let Err(Failure::Port(error)) = attempt.outcome else {
        panic!("original verification error lost")
    };
    assert_eq!(error.identity(), identity);
    assert_eq!(spy.events, ["capture_environment", "verify_metadata"]);
    assert_eq!(attempt.application.artifacts().len(), 0);
}
#[test]
fn target_admission_during_prepare_and_save_retains_original_errors_and_never_writes() {
    for call in [1, 2] {
        let mut spy = Spy::new();
        let error = Opaque(Box::new(23));
        let identity = error.identity();
        spy.publisher_error = Some((call, error));
        let mut attempt = prepare(&mut spy, "sdtm", Some("2026-10-11T00:00:00"));
        let failure = if call == 1 {
            attempt.outcome.err().unwrap()
        } else {
            assert!(attempt.outcome.is_ok());
            let Err(SaveFailure::Failure(failure)) = attempt.application.save(&mut spy) else {
                panic!("missing save admission failure")
            };
            failure
        };
        let Failure::TargetAdmission { index, error } = failure else {
            panic!("original target cause lost")
        };
        assert_eq!(index, 0);
        assert_eq!(error.identity(), identity);
        assert_eq!(
            attempt.application.artifacts().next().unwrap().2,
            PublicationState::Failed
        );
        assert!(spy.published.is_empty());
        let events = spy.events.clone();
        assert!(matches!(
            attempt.application.save(&mut spy),
            Err(SaveFailure::Refused)
        ));
        assert_eq!(spy.events, events);
    }
}
#[test]
fn actual_publication_failure_keeps_admitted_bytes_and_refuses_further_calls() {
    let mut spy = Spy::new();
    let error = Opaque(Box::new(81));
    let identity = error.identity();
    spy.publication_error = Some(error);
    let mut attempt = prepare(&mut spy, "sdtm", Some("2026-10-11T00:00:00"));
    assert!(attempt.outcome.is_ok());
    let held = attempt.application.artifacts().next().unwrap().1.to_vec();
    let Err(SaveFailure::Failure(Failure::Publication { index, error })) =
        attempt.application.save(&mut spy)
    else {
        panic!("original publish error lost")
    };
    assert_eq!(index, 0);
    assert_eq!(error.identity(), identity);
    assert_eq!(
        attempt.application.artifacts().next().unwrap(),
        ("define.xml", held.as_slice(), PublicationState::Failed)
    );
    let events = spy.events.clone();
    assert!(matches!(
        attempt.application.save(&mut spy),
        Err(SaveFailure::Refused)
    ));
    assert_eq!(spy.events, events);
}
#[test]
fn missing_study_is_the_real_typed_owner_before_any_metadata_verification() {
    let mut spy = Spy::new();
    spy.environment = Some(environment(false));
    let attempt = prepare(&mut spy, "sdtm", Some("2026-10-11T00:00:00"));
    let Err(Failure::Metadata(findings)) = attempt.outcome else {
        panic!("missing actual cause")
    };
    assert_eq!(
        findings,
        [yamaa_core::submission_composition::Finding::cause(
            ConditionCode::DefineRequiredDeclaration,
            "study",
            "study"
        )]
    );
    assert_eq!(findings[0].definition().requirement, Some("REQ-1015"));
    assert_eq!(spy.events, ["capture_environment"]);
}
#[test]
fn clock_is_observed_once_and_calendar_admission_is_owned_by_the_engine() {
    for (seconds, expected) in [
        (0, "1970-01-01T00:00:00"),
        (951782400, "2000-02-29T00:00:00"),
    ] {
        let mut spy = Spy::new();
        spy.seconds = Some(seconds);
        let attempt = prepare(&mut spy, "sdtm", None);
        assert!(attempt.outcome.is_ok());
        let held = attempt.application.artifacts().next().unwrap();
        assert!(std::str::from_utf8(held.1)
            .unwrap()
            .contains(&format!("CreationDateTime=\"{expected}\"")));
        assert_eq!(spy.clock_calls.get(), 1);
    }
    for (seconds, detail) in [
        (None, "UTC clock before epoch"),
        (Some(u64::MAX), "UTC clock outside supported calendar"),
    ] {
        let mut spy = Spy::new();
        spy.seconds = seconds;
        let attempt = prepare(&mut spy, "sdtm", None);
        let Err(Failure::Metadata(findings)) = attempt.outcome else {
            panic!("missing clock refusal")
        };
        assert_eq!(
            findings,
            [yamaa_core::define_diagnostics::creation_refusal(detail)]
        );
        assert_eq!(spy.clock_calls.get(), 1);
        assert!(spy.events.is_empty());
    }
}

#[test]
fn resource_observations_keep_original_normative_and_implementation_wire_owners() {
    use define::ResourceObservation as R;
    use yamaa_core::{diagnostic::ContextValue as C, value::Value};
    for (path, observation, code, requirement) in [
        (
            "datasets.DM.artifact",
            R::Missing,
            ConditionCode::ProducerContractMismatch,
            "REQ-0535",
        ),
        (
            "sdtm.define",
            R::Missing,
            ConditionCode::ResourceMissing,
            "REQ-0785",
        ),
        (
            "datasets.DM.artifact",
            R::NotRegularFile,
            ConditionCode::ResourceNotRegularFile,
            "REQ-0785",
        ),
    ] {
        let finding = define::resource_diagnostic(path, "declared.csv", observation);
        assert_eq!(finding.code, code);
        assert_eq!(finding.definition().phase, "validation");
        assert_eq!(finding.definition().requirement, Some(requirement));
        assert_eq!(finding.spec_paths, [path]);
        assert_eq!(
            finding.context,
            [("path".into(), C::Scalar(Value::Str("declared.csv".into())))].into()
        );
        assert_eq!(finding.source_span, None);
        assert_eq!(finding.operand_route, None);
    }
    for (observation, expected) in [
        (R::Unreadable, "resource_unreadable"),
        (R::InvalidRoot, "resource_root"),
        (R::InvalidBase, "resource_base"),
        (R::InvalidPath, "resource_path"),
        (R::InvalidLock, "invalid_packaging_lock"),
        (R::OutsideRoots, "resource_outside_roots"),
        (R::Symlink, "resource_link"),
        (R::Changed, "resource_changed"),
        (R::Limit, "resource_limit"),
    ] {
        let finding = define::resource_diagnostic("sdtm.specs[0]", "declared.yaml", observation);
        assert_eq!(finding.code, ConditionCode::ApplicationRejected);
        assert_eq!(finding.definition().phase, "engine");
        assert_eq!(finding.definition().requirement, None);
        assert_eq!(finding.spec_paths, ["sdtm.specs[0]"]);
        assert_eq!(
            finding.context,
            [
                ("path".into(), C::Scalar(Value::Str("declared.yaml".into()))),
                ("stage".into(), C::Scalar(Value::Str("define".into()))),
                ("code".into(), C::Scalar(Value::Str(expected.into()))),
            ]
            .into()
        );
    }
}
#[test]
fn publication_stage_projection_preserves_full_width_artifact_index_and_cause_text() {
    use define::PublicationFailure as P;
    use yamaa_core::{diagnostic::ContextValue as C, value::Value};
    for (stage, code, phase, requirement) in [
        (
            P::TargetAdmission,
            ConditionCode::ApplicationRejected,
            "engine",
            None,
        ),
        (
            P::Publication,
            ConditionCode::OutputPublicationFailed,
            "output",
            Some("REQ-0764"),
        ),
    ] {
        let finding =
            define::publication_diagnostic(usize::MAX, stage, "opaque original cause".into());
        assert_eq!(finding.code, code);
        assert_eq!(finding.definition().phase, phase);
        assert_eq!(finding.definition().requirement, requirement);
        assert_eq!(finding.spec_paths, ["define"]);
        let mut expected: yamaa_core::diagnostic::Context = [
            ("artifact".into(), C::Integer(usize::MAX.to_string())),
            (
                "cause".into(),
                C::Scalar(Value::Str("opaque original cause".into())),
            ),
        ]
        .into();
        if stage == P::TargetAdmission {
            expected.insert(
                "stage".into(),
                C::Scalar(Value::Str("publication_target_admission".into())),
            );
        }
        assert_eq!(finding.context, expected);
        assert_eq!(finding.source_span, None);
        assert_eq!(finding.operand_route, None);
    }
}
