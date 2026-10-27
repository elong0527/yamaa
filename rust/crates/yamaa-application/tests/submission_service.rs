#![cfg(feature = "yaml")]
//! The Define service: what one section's composition reads, refuses,
//! renders and admits before any save, observed through a caller's resolver
//! that records every request (DEFINE-3, DEFINE-28, PROJECT-14).
//!
//! The seam is that resolver's log of operations, written paths and held
//! references, and the `PublicationState` before any save; a `define` case
//! states only the issues and the files saved.
mod service {
    use std::sync::{Arc, Mutex};
    use yamaa_adapters::{
        file_publication, file_resources,
        host_resources::{Failure, MemoryResolver, Operation, Reply, Request, Resolver},
    };
    use yamaa_application::{
        file_define::{self, Define, PublicationState},
        publication::Cause,
    };
    use yamaa_core::{
        diagnostic::ConditionCode, json::Value, project_environment::Submission,
        project_function::Language,
    };

    const CREATED: &str = "2026-10-11T00:00:00";
    const STUDY: &str = "study:\n  id: STUDY01\n  name: Study\n  description: Independent service fixture\n  protocol_name: Protocol\n";
    const SDTM: &str = "sdtm:\n  standards:\n    - name: SDTMIG\n      version: \"3.4\"\n      status: Final\n  specs: []\n  define: define.xml\n  context: Other\n";

    /// One request a composition made: its operation, the path as written,
    /// and whether it held a reference (a freshness check or a held target).
    type Seen = (Operation, &'static str, bool);

    /// The requests a resolver refuses, and the failure it answers them with.
    type Refusal = (fn(&Request<'_>) -> bool, Failure);

    /// A caller's resolver over a memory store holding one project file. It
    /// records every request, and refuses the requests `refused` selects.
    struct Recorder {
        store: MemoryResolver,
        seen: Mutex<Vec<(Operation, String, bool)>>,
        refused: Option<Refusal>,
    }
    impl Recorder {
        fn new(fields: &str) -> Self {
            let store = MemoryResolver::default();
            let project = format!("yamaa: \"1.0\"\ntype: project\n{fields}");
            store.insert(
                "/",
                "project.yaml",
                "project:1",
                Arc::from(project.as_bytes()),
            );
            Self {
                store,
                seen: Mutex::default(),
                refused: None,
            }
        }
        fn refusing(mut self, selects: fn(&Request<'_>) -> bool, failure: Failure) -> Self {
            self.refused = Some((selects, failure));
            self
        }
        fn seen(&self) -> Vec<(Operation, String, bool)> {
            self.seen.lock().unwrap().clone()
        }
    }
    impl Resolver for Recorder {
        fn request(&self, request: Request<'_>) -> Result<Reply, Failure> {
            self.seen.lock().unwrap().push((
                request.operation,
                request.written.to_owned(),
                request.reference.is_some(),
            ));
            match self.refused {
                Some((selects, failure)) if selects(&request) => Err(failure),
                _ => self.store.request(request),
            }
        }
    }
    fn requests(seen: &[Seen]) -> Vec<(Operation, String, bool)> {
        seen.iter()
            .map(|&(operation, written, held)| (operation, written.to_owned(), held))
            .collect()
    }
    /// Compose `section` of the recorder's project, created at `created`.
    fn compose(resolver: &Arc<Recorder>, section: Submission, created: Option<&str>) -> Define {
        Define::prepare(
            "project.yaml",
            section,
            created,
            "0.3.0",
            Some(Language::Python),
            Some(Arc::clone(resolver) as Arc<dyn Resolver>),
        )
    }
    /// The project file is located, then read once, and nothing else is
    /// requested.
    const PROJECT_READ: &[Seen] = &[
        (Operation::Inspect, "project.yaml", false),
        (Operation::Capture, "project.yaml", false),
    ];

    /// An explicit creation time is admitted before anything is read
    /// (DEFINE-3); the clock is never read for it.
    #[test]
    fn explicit_creation_admits_before_any_capture_or_clock() {
        for created in ["2026-10-11T00:00:00.5", "2026-02-30T00:00:00"] {
            let resolver = Arc::new(Recorder::new(&format!("{STUDY}{SDTM}")));
            let define = compose(&resolver, Submission::Sdtm, Some(created));
            let [issue] = define.issues() else {
                panic!("one refusal: {:?}", define.issues())
            };
            assert_eq!(issue.code, ConditionCode::DefineCreatedInvalid, "{created}");
            assert!(resolver.seen().is_empty(), "{created}");
            assert_eq!(define.artifacts().len(), 0);
            assert!(define.plan().is_none());
        }
    }

    /// A project file that cannot be read refuses the composition, which
    /// then holds and saves nothing.
    #[test]
    fn a_project_that_cannot_be_read_refuses_the_composition_and_saves_nothing() {
        let resolver = Arc::new(Recorder::new(&format!("{STUDY}{SDTM}")).refusing(
            |request| request.written == "project.yaml",
            Failure::Unreadable,
        ));
        let define = compose(&resolver, Submission::Sdtm, Some(CREATED));
        assert!(define.failure().is_some());
        // Locating the project file fails, so nothing is read.
        assert_eq!(resolver.seen(), requests(&PROJECT_READ[..1]));
        assert_eq!(define.artifacts().len(), 0);
        assert!(define.plan().is_none());
    }

    /// The document is rendered and the captured metadata verified before
    /// any target is admitted; a save writes the held bytes, and a repeated
    /// record keeps them.
    #[test]
    fn complete_metadata_and_rendering_precede_publication_and_record_marks_held_bytes() {
        let resolver = Arc::new(Recorder::new(&format!("{STUDY}{SDTM}")));
        let mut define = compose(&resolver, Submission::Sdtm, Some(CREATED));
        assert!(define.issues().is_empty(), "{:?}", define.issues());
        assert_eq!(
            resolver.seen(),
            requests(&[
                (Operation::Inspect, "project.yaml", false),
                (Operation::Capture, "project.yaml", false),
                // The freshness check of the captured project file.
                (Operation::Capture, "project.yaml", true),
                // The define.xml target, resolved and then admitted.
                (Operation::Target, "define.xml", false),
                (Operation::Target, "define.xml", true),
            ])
        );
        let held = define
            .artifacts()
            .map(|(written, bytes, state)| (written.to_owned(), bytes.to_vec(), state))
            .collect::<Vec<_>>();
        let [(written, bytes, state)] = held.as_slice() else {
            panic!("one held file: {held:?}")
        };
        assert_eq!(
            (written.as_str(), *state),
            ("define.xml", PublicationState::Ready)
        );
        let xml = std::str::from_utf8(bytes).unwrap();
        assert!(
            xml.contains(&format!("CreationDateTime=\"{CREATED}\"")),
            "{xml}"
        );
        // A save writes the held file to the target the resolver named.
        let paths = define
            .plan()
            .unwrap()
            .iter()
            .map(|file| file.path().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(paths, ["memory:/define.xml"]);
        for _ in 0..2 {
            define.record(1, None);
            assert_eq!(
                define.artifacts().next().unwrap(),
                ("define.xml", bytes.as_slice(), PublicationState::Published)
            );
            assert!(define.issues().is_empty());
        }
    }

    /// A captured file that changed before verification refuses the
    /// composition with FILE-10's condition at the field that declared it, the
    /// path as written, before any target is requested. A caller's resolver
    /// names the file by a reference that is not its location, and the
    /// verification still finds its declaration.
    #[test]
    fn a_file_changed_before_verification_fails_at_its_field_before_any_target() {
        let resolver = Arc::new(Recorder::new(&format!("{STUDY}{SDTM}")).refusing(
            |request| request.operation == Operation::Capture && request.reference.is_some(),
            Failure::Changed,
        ));
        let define = compose(&resolver, Submission::Sdtm, Some(CREATED));
        // The composition keeps the resolver's original change.
        assert!(
            matches!(
                define.failure(),
                Some(file_define::Failure::ClassifiedResource {
                    error: file_resources::Error::Changed,
                    ..
                })
            ),
            "{:?}",
            define.failure()
        );
        assert_eq!(
            crate::support::rows(define.issues()),
            [yamaa_adapters::json!({
                "phase": "ingest",
                "condition": "resource_path_content_changed",
                "rule": "FILE-10",
                "spec_paths": ["environment"],
                "context": {"path": "project.yaml"},
            })],
            "a changed project file"
        );
        assert_eq!(
            resolver.seen(),
            requests(&[
                (Operation::Inspect, "project.yaml", false),
                (Operation::Capture, "project.yaml", false),
                (Operation::Capture, "project.yaml", true),
            ])
        );
        assert_eq!(define.artifacts().len(), 0);
    }

    /// A target the publisher port refuses while the composition is admitted
    /// fails it at that file, which writes nothing.
    #[test]
    fn target_admission_during_prepare_retains_the_original_error_and_never_writes() {
        let resolver = Arc::new(Recorder::new(&format!("{STUDY}{SDTM}")).refusing(
            |request| request.operation == Operation::Target && request.reference.is_some(),
            Failure::Rejected,
        ));
        let define = compose(&resolver, Submission::Sdtm, Some(CREATED));
        let Some(file_define::Failure::TargetAdmission { index, error }) = define.failure() else {
            panic!("original target cause lost: {:?}", define.failure())
        };
        assert_eq!(*index, 0);
        assert!(matches!(
            error,
            file_publication::Error::Host(Failure::Rejected)
        ));
        assert_eq!(
            define.artifacts().next().unwrap().2,
            PublicationState::Failed
        );
        assert!(define.plan().is_none());
    }

    /// A save that stopped at a file keeps the held bytes and fails the
    /// composition, which then publishes nothing again.
    #[test]
    fn a_recorded_save_failure_keeps_admitted_bytes_and_fails_the_composition() {
        let causes: [fn() -> Cause; 2] = [
            || Cause::Target(file_publication::Error::InvalidTarget),
            || Cause::Commit(file_publication::Error::Changed),
        ];
        for cause in causes {
            let resolver = Arc::new(Recorder::new(&format!("{STUDY}{SDTM}")));
            let mut define = compose(&resolver, Submission::Sdtm, Some(CREATED));
            assert!(define.issues().is_empty(), "{:?}", define.issues());
            let held = define.artifacts().next().unwrap().1.to_vec();
            define.record(0, Some((0, cause())));
            assert_eq!(
                define.artifacts().next().unwrap(),
                ("define.xml", held.as_slice(), PublicationState::Failed)
            );
            assert!(define.plan().is_none());
        }
    }

    /// A project without a study refuses composition with DEFINE-28's
    /// finding before the metadata is verified.
    #[test]
    fn missing_study_is_the_real_typed_owner_before_any_metadata_verification() {
        let resolver = Arc::new(Recorder::new(SDTM));
        let define = compose(&resolver, Submission::Sdtm, Some(CREATED));
        let [issue] = define.issues() else {
            panic!("one refusal: {:?}", define.issues())
        };
        assert_eq!(
            (
                issue.code,
                issue.rule(),
                issue.spec_paths.as_slice(),
                Value::Object(issue.context.clone())
            ),
            (
                ConditionCode::DefineRequiredDeclaration,
                Some("DEFINE-28"),
                ["study".to_owned()].as_slice(),
                yamaa_adapters::json!({"detail": "study", "field": "study"})
            )
        );
        assert_eq!(resolver.seen(), requests(PROJECT_READ));
    }

    /// A run admits a section that lists only its specifications; Define
    /// composition reports every missing declaration together (DEFINE-28).
    #[test]
    fn a_section_listing_only_specs_reports_every_missing_define_declaration_together() {
        let resolver = Arc::new(Recorder::new("sdtm:\n  specs: []\n"));
        let define = compose(&resolver, Submission::Sdtm, Some(CREATED));
        let found = define
            .issues()
            .iter()
            .map(|issue| {
                (
                    issue.code,
                    issue.rule(),
                    issue.spec_paths.clone(),
                    Value::Object(issue.context.clone()),
                )
            })
            .collect::<Vec<_>>();
        let missing = |path: &str, field: &str| {
            (
                ConditionCode::DefineRequiredDeclaration,
                Some("DEFINE-28"),
                vec![path.to_owned()],
                yamaa_adapters::json!({ "detail": field, "field": field }),
            )
        };
        assert_eq!(
            found,
            [
                missing("study", "study"),
                missing("sdtm.standards", "standards"),
                missing("sdtm.define", "define"),
            ]
        );
        assert_eq!(resolver.seen(), requests(PROJECT_READ));
    }

    /// A project file that no longer declares the selected section when
    /// composition reads it refuses the section as any undeclared section is
    /// refused (PROJECT-14), before its declarations are checked or any
    /// specification is read.
    #[test]
    fn a_project_that_no_longer_declares_the_selected_section_refuses_it() {
        let resolver = Arc::new(Recorder::new(&format!("{STUDY}{SDTM}")));
        let define = compose(&resolver, Submission::Adam, Some(CREATED));
        let [issue] = define.issues() else {
            panic!("one refusal: {:?}", define.issues())
        };
        assert_eq!(
            (
                issue.phase(),
                issue.condition(),
                issue.rule(),
                issue.spec_paths.as_slice(),
                Value::Object(issue.context.clone())
            ),
            (
                "validation",
                "unknown_section",
                Some("PROJECT-14"),
                [].as_slice(),
                yamaa_adapters::json!({"section": "adam", "sections": ["sdtm"]})
            )
        );
        assert_eq!(resolver.seen(), requests(PROJECT_READ));
        assert!(define.plan().is_none());
    }
}
