#![cfg(feature = "yaml")]
//! The 1.x header (SCHEMA-2, LAYER-4, P-VER-1).
//!
//! A root that declares `yamaa` is a 1.x document, and one that declares
//! `schema_version` is written in language 0.5. The engine reads language
//! version 1.0 (#3030 D76): a public call reads each file it is given by the
//! header it declares, and refuses a newer version, and 0.5, as
//! `unsupported_language_version`. The 1.0 path reads the header before
//! choosing a closure, admits each document at the version it declares and
//! each specification layer at its own, and checks the file every
//! referencing field names.
mod declaration {
    mod language_header {
        use crate::support::Edit as _;
        use std::fs;
        use std::path::Path;

        use std::sync::Arc;
        use yamaa_adapters::document::{
            self, check_references, Closures, ReferenceError, SpecificationError,
        };
        use yamaa_adapters::json;

        use yamaa_adapters::specification_source::{CapturedSchema, Error, Source};
        use yamaa_core::diagnostic::{ConditionCode, Diagnostic as Issue};
        use yamaa_core::domain_entry::Request;
        use yamaa_core::inheritance::{self as graph, Source as GraphSource};
        use yamaa_core::json::Value;
        use yamaa_core::language::{FileType, LanguageVersion, ReferenceFault, LANGUAGE_VERSION};
        use yamaa_core::project_function::Language;
        use yamaa_core::resolver::{Captured, Failure, Name, Resolver};
        use yamaa_core::resource::ResourceFailure;
        use yamaa_core::schema::{NormalizationError, SchemaContext as C};

        /// A complete specification as language 0.5 wrote it, under
        /// `schema_version`, which no route reads (SCHEMA-2).
        const LEGACY: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
        /// A complete 1.0 specification of the same dataset.
        const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
        /// A complete 1.0 function file.
        const FUNCTION: &str = "yamaa: \"1.0\"\ntype: function\nname: bmi\ndescription: Body mass index.\nparams: [{name: weight, type: float}]\nreturns: float\nimplementations: {python: project.bmi}\ntests: [{id: normal, covers: [normal], args: {weight: 1.0}, result: 1.0}]\n";

        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let path = crate::support::Scratch::canonical("language-header");
                Self(path)
            }
            fn write(&self, name: &str, text: &str) {
                let path = self.0.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, text).unwrap();
            }
            /// A file's identity as the engine reports it.
            fn path(&self, name: &str) -> String {
                let path = self.0.join(name).to_str().unwrap().to_owned();
                #[cfg(windows)]
                let path = path.replace('\\', "/");
                path
            }
            /// The issues a public check of `spec`, with `project` if any, reports.
            fn check(&self, spec: &str, project: Option<&str>) -> Vec<Issue> {
                let specification = self.path(spec);
                let environment = project.map(|project| self.path(project));
                yamaa_adapters::file_application::check(
                    Language::Python,
                    Request {
                        specification: &specification,
                        environment: environment.as_deref(),
                    },
                    crate::support::study::identity("test", &specification),
                )
            }
            /// The issues a public check of `spec` reports in `host`, then
            /// those a public run refuses it with before it reads any input.
            fn routes(&self, host: Language, spec: &str) -> [Vec<Issue>; 2] {
                let specification = self.path(spec);
                let request = || Request {
                    specification: &specification,
                    environment: None,
                };
                let identity = || crate::support::study::identity("test", &specification);
                let checked = yamaa_adapters::file_application::check(host, request(), identity());
                let built = match crate::support::study::domain(host, request(), identity()) {
                    Ok(domain) => domain.issues(),
                    Err(issues) => issues,
                };
                [checked, built]
            }
            /// The issues a submission of the `sdtm` section of the project
            /// file `project` reports in `host`.
            fn submission(&self, host: Language, project: &str) -> Vec<Issue> {
                yamaa_adapters::file_define::Define::prepare(
                    &self.path(project),
                    yamaa_core::project_environment::Submission::Sdtm,
                    Some("2026-09-11T00:00:00"),
                    "0",
                    Some(host),
                    None,
                )
                .issues()
                .to_vec()
            }
            /// Each listing of the project file `project` in `host`, in
            /// listing order: the issues a project check reports for it,
            /// each naming its specification as the entry, as a project's
            /// member does (`project_scope::identify`), and those a project
            /// run refuses its specification with.
            fn listings(&self, host: Language, project: &str) -> Vec<[Vec<Issue>; 2]> {
                let project = yamaa_adapters::project_scope::Project::read(
                    &self.path(project),
                    Some(host),
                    None,
                )
                .unwrap();
                let plan = project.plan(None);
                project
                    .listed()
                    .iter()
                    .enumerate()
                    .map(|(index, listed)| {
                        let mut checked = listed.failure().to_vec();
                        yamaa_adapters::project_scope::identify(&mut checked, listed.identity());
                        let refused = plan
                            .steps()
                            .iter()
                            .find(|step| step.listed() == index)
                            .map(|step| step.refused().to_vec())
                            .unwrap_or_default();
                        [checked, refused]
                    })
                    .collect()
            }
        }

        /// The one issue a refused read reports.
        fn only(issues: &[Issue]) -> &Issue {
            assert_eq!(issues.len(), 1, "{issues:?}");
            &issues[0]
        }

        fn refused(issue: &Issue, rule: &str, path: &str, context: Value) {
            assert_eq!(issue.phase(), "validation");
            assert_eq!(issue.condition(), "unsupported_language_version");
            assert_eq!(issue.rule(), Some(rule));
            assert_eq!(issue.spec_paths, [path]);
            assert_eq!(Value::Object(issue.context.clone()), context);
        }

        /// An issue's phase, condition, rule, spec paths and context.
        type Row = (
            &'static str,
            &'static str,
            Option<&'static str>,
            Vec<String>,
            Value,
        );

        fn rows(issues: &[Issue]) -> Vec<Row> {
            issues
                .iter()
                .map(|issue| {
                    (
                        issue.phase(),
                        issue.condition(),
                        issue.rule(),
                        issue.spec_paths.clone(),
                        Value::Object(issue.context.clone()),
                    )
                })
                .collect()
        }

        /// A validation issue of SCHEMA-2 at `path`.
        fn header_row(condition: &'static str, path: &str, context: Value) -> Row {
            (
                "validation",
                condition,
                Some("SCHEMA-2"),
                vec![path.into()],
                context,
            )
        }

        #[test]
        fn a_public_check_reads_a_1_0_specification_and_refuses_a_newer_one() {
            let study = Study::new();
            // The engine reads 1.0 (D76).
            study.write("spec.yaml", SPEC);
            assert_eq!(study.check("spec.yaml", None), []);
            // A missing parent fails where the 1.0 entry names it (LAYER-2).
            study.write("spec.yaml", &format!("{SPEC}parent: missing.yaml\n"));
            let issue = only(&study.check("spec.yaml", None)).clone();
            assert_eq!(issue.condition(), "parent_not_found");
            assert_eq!(issue.spec_paths, ["parent"]);
            assert_eq!(issue.context["source"], study.path("spec.yaml"));
            // A newer version, whatever the rest of the file holds, at `yamaa`.
            study.write("spec.yaml", "yamaa: \"1.4\"\nanything: [1, 2]\n");
            refused(
                only(&study.check("spec.yaml", None)),
                "SCHEMA-2",
                "yamaa",
                json!({"version": "1.4", "language_versions": ["1.0"]}),
            );
            // An unquoted version is a YAML float and no version (SCHEMA-4).
            study.write("spec.yaml", "yamaa: 1.0\ntype: spec\n");
            let issue = only(&study.check("spec.yaml", None)).clone();
            assert_eq!(issue.condition(), "invalid_field_type");
            assert_eq!(issue.spec_paths, ["yamaa"]);
            assert_eq!(
                Value::Object(issue.context),
                json!({"expected": "language_version", "actual": "float"})
            );
        }

        #[test]
        fn a_public_check_refuses_a_specification_that_declares_schema_version() {
            let study = Study::new();
            study.write("input.csv", "ID\n1\n");
            // `schema_version`, with `yamaa` beside it or not, declares
            // language 0.5, which the engine does not read (SCHEMA-2).
            for text in [LEGACY.to_owned(), format!("{LEGACY}yamaa: \"1.0\"\n")] {
                study.write("spec.yaml", &text);
                refused(
                    only(&study.check("spec.yaml", None)),
                    "SCHEMA-2",
                    "schema_version",
                    json!({"version": "0.5", "language_versions": ["1.0"]}),
                );
            }
            // A file that declares neither has no header.
            study.write("spec.yaml", "domain: TEST\n");
            let issue = only(&study.check("spec.yaml", None)).clone();
            assert_eq!(issue.condition(), "missing_required_field");
            assert_eq!(issue.spec_paths, ["yamaa"]);
        }

        /// The project file a call names is read by its own header (SCHEMA-2):
        /// one that declares `schema_version` is written in language 0.5,
        /// which the engine does not read, and one that declares neither
        /// header declares no version. Each is a finding of the project file,
        /// which names it as `source` and `entry`, at its root.
        #[test]
        fn a_project_file_that_declares_schema_version_is_refused_as_language_0_5() {
            let study = Study::new();
            study.write("input.csv", "ID\n1\n");
            study.write("spec.yaml", SPEC);
            study.write("project.yaml", "yamaa: \"1.0\"\ntype: project\n");
            assert_eq!(study.check("spec.yaml", Some("project.yaml")), []);
            let project = study.path("project.yaml");
            for (text, row) in [
                (
                    "schema_version: '1.0'\nenvironments: {python: python/environment.yaml}\n",
                    header_row(
                        "unsupported_language_version",
                        "schema_version",
                        json!({"version": "0.5", "language_versions": ["1.0"],
                            "source": project, "entry": project, "environment_path": "$"}),
                    ),
                ),
                (
                    "environments: {python: {lock: uv.lock}}\n",
                    (
                        "validation",
                        "missing_required_field",
                        Some("SCHEMA-13"),
                        vec!["yamaa".into()],
                        json!({"field": "yamaa", "class": "project_class",
                            "source": project, "entry": project, "environment_path": "$"}),
                    ),
                ),
            ] {
                study.write("project.yaml", text);
                assert_eq!(
                    rows(&study.check("spec.yaml", Some("project.yaml"))),
                    [row],
                    "{text}"
                );
            }
        }

        /// A 1.0 specification reads the header of the producer a `schema`
        /// link names at the link, before the producer is prepared
        /// (SCHEMA-2): a version the engine does not read, or another type,
        /// is refused at `input.<name>.schema`, naming the producer as
        /// `source`, by the check and the run of each host. A malformed
        /// header stays a finding in the producer, which names the producer
        /// as `source` and `entry`, as a finding further into it does
        /// (#3581). A producer that declares `schema_version` is written in
        /// language 0.5, which the engine does not read.
        #[test]
        fn a_1_0_schema_link_refuses_a_producer_it_does_not_read_at_the_link() {
            let study = Study::new();
            let link = "input: {SRC: {path: produced.csv, schema: producer.yaml}}";
            study.write("spec.yaml", &SPEC.replace("input: {SRC: input.csv}", link));
            let producer = study.path("producer.yaml");
            for (text, expected) in [
                (
                    "yamaa: \"1.4\"\ntype: spec\nanything: [1, 2]\n",
                    header_row(
                        "unsupported_language_version",
                        "input.SRC.schema",
                        json!({"source": producer, "version": "1.4", "language_versions": ["1.0"]}),
                    ),
                ),
                (
                    "yamaa: \"1.0\"\ntype: project\n",
                    header_row(
                        "unexpected_file_type",
                        "input.SRC.schema",
                        json!({"source": producer, "type": "project", "expected": "spec"}),
                    ),
                ),
                (
                    LEGACY,
                    header_row(
                        "unsupported_language_version",
                        "input.SRC.schema",
                        json!({"source": producer, "version": "0.5", "language_versions": ["1.0"]}),
                    ),
                ),
            ] {
                study.write("producer.yaml", text);
                for host in [Language::Python, Language::R] {
                    assert_eq!(
                        study.routes(host, "spec.yaml").map(|issues| rows(&issues)),
                        [vec![expected.clone()], vec![expected.clone()]],
                        "{host:?}: {text}"
                    );
                }
            }
            // A malformed header is a finding in the producer, at its own
            // `yamaa` or at the field it lacks, naming the producer.
            let named = |mut context: Value| {
                context["source"] = Value::String(producer.clone());
                context["entry"] = Value::String(producer.clone());
                context
            };
            for (consumer, text, expected) in [
                (
                    "spec.yaml",
                    "yamaa: 1.0\ntype: spec\n",
                    (
                        "validation",
                        "invalid_field_type",
                        Some("SCHEMA-12"),
                        vec!["yamaa".to_string()],
                        named(json!({"actual": "float", "expected": "language_version"})),
                    ),
                ),
                (
                    "spec.yaml",
                    "yamaa: \"1.0\"\n",
                    (
                        "validation",
                        "missing_required_field",
                        Some("SCHEMA-13"),
                        vec!["type".to_string()],
                        named(json!({"class": "root_class", "field": "type"})),
                    ),
                ),
            ] {
                study.write("producer.yaml", text);
                for host in [Language::Python, Language::R] {
                    assert_eq!(
                        study.routes(host, consumer).map(|issues| rows(&issues)),
                        [vec![expected.clone()], vec![expected.clone()]],
                        "{host:?}, {consumer}: {text}"
                    );
                }
            }
        }

        /// A 1.0 project reads the header of each specification it lists at
        /// the listing, before the specification is prepared (SCHEMA-2): a
        /// version the engine does not read, or another type, is the
        /// listing's own finding at `<section>.specs[i]`, naming the file as
        /// `source`, in each host. A project check reports it for that
        /// listing and a project run refuses the specification with it, each
        /// naming the specification as the entry; a submission composing the
        /// section reports it too. A listed specification that declares
        /// `schema_version` is written in language 0.5, which the engine does
        /// not read. A malformed header stays the specification's own
        /// finding.
        #[test]
        fn a_1_0_project_refuses_a_listed_specification_it_does_not_read_at_its_listing() {
            let study = Study::new();
            study.write("input.csv", "ID\n1\n");
            study.write("dm.yaml", &SPEC.replace("domain: TEST", "domain: DM"));
            study.write(
                "project.yaml",
                "yamaa: \"1.0\"\ntype: project\nsdtm:\n  specs: [dm.yaml, listed.yaml]\n",
            );
            study.write(
                "submission.yaml",
                "yamaa: \"1.0\"\ntype: project\n\
                 study: {id: S, name: S, description: A study, protocol_name: S-01}\n\
                 sdtm:\n  specs: [listed.yaml]\n\
                 \x20 standards: [{name: SDTMIG, version: \"3.4\", status: Final}]\n\
                 \x20 define: define.xml\n",
            );
            let listed = study.path("listed.yaml");
            for (text, condition, context) in [
                (
                    "yamaa: \"1.4\"\ntype: spec\nanything: [1, 2]\n",
                    "unsupported_language_version",
                    json!({"source": listed, "version": "1.4", "language_versions": ["1.0"]}),
                ),
                (
                    "yamaa: \"1.0\"\ntype: codelists\ncodelists: []\n",
                    "unexpected_file_type",
                    json!({"source": listed, "type": "codelists", "expected": "spec"}),
                ),
                (
                    LEGACY,
                    "unsupported_language_version",
                    json!({"source": listed, "version": "0.5", "language_versions": ["1.0"]}),
                ),
            ] {
                study.write("listed.yaml", text);
                let mut entry = context.clone();
                entry["entry"] = Value::String(listed.clone());
                let refused = vec![header_row(condition, "sdtm.specs[1]", entry)];
                for host in [Language::Python, Language::R] {
                    assert_eq!(
                        study
                            .listings(host, "project.yaml")
                            .into_iter()
                            .map(|routes| routes.map(|issues| rows(&issues)))
                            .collect::<Vec<_>>(),
                        [[Vec::new(), Vec::new()], [refused.clone(), refused.clone()]],
                        "{host:?}: {text}"
                    );
                    assert_eq!(
                        rows(&study.submission(host, "submission.yaml")),
                        [header_row(condition, "sdtm.specs[0]", context.clone())],
                        "{host:?} submission: {text}"
                    );
                }
            }
            // A malformed header does not fail at its listing.
            study.write("listed.yaml", "yamaa: 1.0\ntype: spec\n");
            assert!(
                study
                    .listings(Language::Python, "project.yaml")
                    .iter()
                    .flatten()
                    .all(Vec::is_empty),
                "an unquoted version"
            );
        }

        // -- The 1.0 path -------------------------------------------------------

        /// Files resolved beside their declaring file.
        struct Files;
        impl Resolver for Files {
            type Error = std::io::Error;
            fn inspect(&mut self, name: Name<'_>) -> Result<GraphSource, Self::Error> {
                let Name::Written {
                    declaring, written, ..
                } = name
                else {
                    panic!("unexpected request {name:?}")
                };
                let candidate = Path::new(declaring).parent().unwrap().join(written);
                let identity = yamaa_adapters::canonical_path::canonicalize(&candidate)?;
                let identity = identity.to_str().unwrap().replace('\\', "/");
                Ok(GraphSource {
                    display_path: identity.clone(),
                    identity,
                })
            }
            fn capture(&mut self, name: Name<'_>) -> Result<Captured, Self::Error> {
                let Name::Located { source, .. } = name else {
                    panic!("unexpected request {name:?}")
                };
                Ok(Captured::resource(fs::read(&source.identity)?.into(), true))
            }
            /// A file that cannot be located is missing; a read error stays
            /// the host's own.
            fn failure(&self, name: Name<'_>, _: &Self::Error) -> Option<Failure> {
                matches!(name, Name::Written { .. })
                    .then_some(Failure::Resource(ResourceFailure::Missing))
            }
            fn rebase(
                &mut self,
                _: &GraphSource,
                _: &GraphSource,
                written: &str,
            ) -> Result<String, Self::Error> {
                Ok(written.into())
            }
        }

        /// The text a resolved specification's root field holds.
        fn text<'a>(document: &'a yamaa_core::schema::Document, field: &str) -> Option<&'a str> {
            match &document.nodes()[document.field(document.root(), field)?] {
                yamaa_core::schema::DocumentNode::Text(value) => Some(value),
                _ => None,
            }
        }

        fn source(identity: &str, text: &str) -> Source {
            Source {
                identity: identity.into(),
                bytes: text.as_bytes().to_vec(),
            }
        }

        fn closures() -> Closures {
            Closures::shipped().unwrap()
        }

        fn versioned(
            identity: &str,
            text: &str,
            expected: FileType,
            reads: LanguageVersion,
            closures: &Closures,
        ) -> document::VersionedDocument {
            match document::decode(source(identity, text), expected, Some(reads), closures) {
                Ok(document) => document,
                Err(error) => panic!("{identity}: {error:?}"),
            }
        }

        /// The findings a refused decode or admission reports.
        /// One finding: its cause, path and context.
        type Finding = (ConditionCode, String, Vec<(&'static str, C)>);

        fn findings(error: &Error) -> Vec<Finding> {
            let Error::Findings(captured) = error else {
                panic!("findings: {error:?}");
            };
            captured
                .findings()
                .iter()
                .map(|finding| (finding.code, finding.path.clone(), finding.context.clone()))
                .collect()
        }

        #[test]
        fn the_root_is_read_before_a_closure_is_chosen() {
            let closures = closures();
            let decode = |text: &str, file_type| {
                document::decode(
                    source("file.yaml", text),
                    file_type,
                    Some(LANGUAGE_VERSION),
                    &closures,
                )
            };
            // A file that declares `schema_version` is written in language
            // 0.5, and one without `yamaa` declares no header: each is
            // refused before a closure is chosen, whatever its type
            // (SCHEMA-2).
            for file_type in [FileType::Spec, FileType::Project] {
                assert_eq!(
                    findings(&decode(LEGACY, file_type).unwrap_err()),
                    [(
                        ConditionCode::HeaderLanguageVersion,
                        "schema_version".to_string(),
                        vec![
                            ("version", C::Text("0.5".into())),
                            ("language_versions", C::TextList(vec!["1.0".into()])),
                        ],
                    )],
                    "{file_type:?}"
                );
            }
            for (text, file_type) in [
                ("domain: TEST\n", FileType::Spec),
                ("name: bmi\n", FileType::Spec),
                ("sdtm: {specs: [dm.yaml]}\n", FileType::Project),
            ] {
                let error = decode(text, file_type).unwrap_err();
                assert_eq!(
                    findings(&error)
                        .iter()
                        .map(|(code, path, _)| (*code, path.as_str()))
                        .collect::<Vec<_>>(),
                    [(ConditionCode::SchemaMissingRequiredField, "yamaa")],
                    "{text}"
                );
            }
            // `yamaa` and `type` select the 1.0 closure of the declared type.
            for (text, file_type) in [
                (SPEC, FileType::Spec),
                ("yamaa: \"1.0\"\ntype: project\n", FileType::Project),
                (
                    "yamaa: \"1.0\"\ntype: codelists\ncodelists: []\n",
                    FileType::Codelists,
                ),
                (FUNCTION, FileType::Function),
            ] {
                let document = versioned("file.yaml", text, file_type, LANGUAGE_VERSION, &closures);
                assert_eq!(document.header().file_type, file_type);
                assert_eq!(document.header().version, LANGUAGE_VERSION);
                assert!(Arc::ptr_eq(document.schema(), closures.get(file_type)));
                assert_eq!(
                    document.schema().structure().root_class().name,
                    file_type.root_class()
                );
                document.admit().unwrap();
            }
            // A decoder that reads no 1.x version refuses every one.
            let error =
                document::decode(source("spec.yaml", SPEC), FileType::Spec, None, &closures)
                    .unwrap_err();
            let Error::Findings(captured) = &error else {
                panic!("findings");
            };
            let held = captured.document().unwrap();
            let version = held.field(held.root(), "yamaa").unwrap();
            assert_eq!(
                findings(&error),
                [(
                    ConditionCode::HeaderLanguageVersion,
                    "yamaa".to_string(),
                    vec![
                        ("version", C::InputValue(version)),
                        ("language_versions", C::TextList(vec![])),
                    ]
                )]
            );
        }

        #[test]
        fn the_calling_function_refuses_an_unread_version_or_another_type() {
            let closures = closures();
            let error = document::decode(
                source("spec.yaml", &SPEC.edit("\"1.0\"", "\"1.1\"")),
                FileType::Spec,
                Some(LANGUAGE_VERSION),
                &closures,
            )
            .unwrap_err();
            let [(code, path, context)] = findings(&error).try_into().unwrap();
            assert_eq!(
                (code, path.as_str()),
                (ConditionCode::HeaderLanguageVersion, "yamaa")
            );
            assert_eq!(
                context[1],
                ("language_versions", C::TextList(vec!["1.0".into()]))
            );
            for (text, expected) in [
                ("yamaa: \"1.0\"\ntype: project\n", FileType::Spec),
                (SPEC, FileType::Project),
                ("yamaa: \"1.0\"\ntype: spec\n", FileType::Function),
                ("yamaa: \"1.0\"\ntype: codelist\n", FileType::Codelists),
            ] {
                let error = document::decode(
                    source("file.yaml", text),
                    expected,
                    Some(LANGUAGE_VERSION),
                    &closures,
                )
                .unwrap_err();
                let [(code, path, context)] = findings(&error).try_into().unwrap();
                assert_eq!(
                    (code, path.as_str()),
                    (ConditionCode::HeaderFileType, "type")
                );
                assert_eq!(context[1], ("expected", C::Text(expected.name().into())));
            }
        }

        /// The 1.0 specification closure with an invented field `added` that
        /// language version 1.1 introduces, on every column (P-VER-1).
        fn later_closures() -> Closures {
            let shipped = closures();
            let sources = shipped
                .get(FileType::Spec)
                .sources()
                .iter()
                .map(|module| {
                    let text = std::str::from_utf8(&module.bytes).unwrap();
                    let text = match module.identity.as_str() {
                        "schema_shared.yaml" => {
                            text.edit("values: [\"1.0\"]", "values: [\"1.0\", \"1.1\"]")
                        }
                        "schema_spec.yaml" => text.edit(
                            "column_class:\n",
                            "column_class:\n    - added: {type: str, since: \"1.1\"}\n",
                        ),
                        _ => text.to_string(),
                    };
                    source(&module.identity, &text)
                })
                .collect();
            let spec = CapturedSchema::admit_root(sources, 0, "root_class").unwrap();
            shipped.with(FileType::Spec, spec)
        }

        const ADDED: &str = "  - {name: ID, type: int, derivation: SRC.ID, added: new}\n";

        #[test]
        fn a_document_is_admitted_at_the_version_it_declares() {
            let closures = later_closures();
            let reads = LanguageVersion::new(1);
            let with_added = SPEC.edit("  - {name: ID, type: int, derivation: SRC.ID}\n", ADDED);
            // 1.1 admits the field 1.1 introduced.
            let newer = with_added.edit("\"1.0\"", "\"1.1\"");
            versioned("spec.yaml", &newer, FileType::Spec, reads, &closures)
                .admit()
                .unwrap();
            // A file declaring 1.0 that writes it fails, naming the version.
            let error = versioned("spec.yaml", &with_added, FileType::Spec, reads, &closures)
                .admit()
                .unwrap_err();
            assert_eq!(
                findings(&error),
                [(
                    ConditionCode::SchemaUnknownFieldSince,
                    "columns.ID.added".to_string(),
                    vec![
                        ("field", C::Text("added".into())),
                        ("class", C::Text("column_class".into())),
                        ("identifier", C::Text("added".into())),
                        ("since", C::Text("1.1".into())),
                    ]
                )]
            );
            assert_eq!(
                ConditionCode::SchemaUnknownFieldSince
                    .definition()
                    .condition,
                "unknown_field"
            );
            // An engine that reads only 1.0 refuses the 1.1 file outright.
            let error = document::decode(
                source("spec.yaml", &newer),
                FileType::Spec,
                Some(LANGUAGE_VERSION),
                &closures,
            )
            .unwrap_err();
            assert_eq!(findings(&error)[0].0, ConditionCode::HeaderLanguageVersion);
            // A field no version declares keeps the plain cause.
            let error = versioned(
                "spec.yaml",
                &SPEC.edit("derivation: SRC.ID}", "derivation: SRC.ID, other: x}"),
                FileType::Spec,
                reads,
                &closures,
            )
            .admit()
            .unwrap_err();
            assert_eq!(findings(&error)[0].0, ConditionCode::SchemaUnknownField);
        }

        /// The specification at `entry`, prepared by an engine reading `reads`.
        fn prepare(
            study: &Study,
            entry: &str,
            reads: LanguageVersion,
            closures: &Closures,
        ) -> Result<document::PreparedSpecification, SpecificationError<std::io::Error>> {
            let identity = study.path(entry);
            let text = fs::read_to_string(study.0.join(entry)).unwrap();
            versioned(&identity, &text, FileType::Spec, reads, closures)
                .prepare_specification(identity.clone(), &mut Files)
        }

        /// The classified traversal diagnostic of a refused chain.
        fn traversal(
            error: SpecificationError<std::io::Error>,
        ) -> yamaa_core::diagnostic::Diagnostic {
            let SpecificationError::Traversal(error) = error else {
                panic!("traversal: {error:?}");
            };
            error.diagnostic().unwrap().unwrap()
        }

        /// The layer findings of a refused chain, with the layer they are in.
        fn layer_findings(
            error: SpecificationError<std::io::Error>,
        ) -> (String, Vec<(ConditionCode, String)>) {
            let SpecificationError::Traversal(graph::Error::Layer {
                source,
                error: NormalizationError::Invalid(findings),
                ..
            }) = error
            else {
                panic!("layer findings: {error:?}");
            };
            let findings = findings
                .into_iter()
                .map(|finding| (finding.code, finding.path))
                .collect();
            (source, findings)
        }

        const BASE: &str = "yamaa: \"1.0\"\ntype: spec\ninput: {SRC: input.csv}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
        const CHILD: &str = "yamaa: \"1.0\"\ntype: spec\nparent: base.yaml\ndomain: TEST\nkeys: [ID]\noutput: {path: output.csv, columns: [ID]}\n";

        #[test]
        fn each_layer_is_admitted_at_its_own_version_and_the_chain_needs_the_newest() {
            let study = Study::new();
            let closures = later_closures();
            let reads = LanguageVersion::new(1);
            study.write("base.yaml", BASE);
            study.write("spec.yaml", CHILD);
            let prepared = prepare(&study, "spec.yaml", reads, &closures).unwrap();
            assert_eq!(prepared.layers().len(), 2);
            assert_eq!(prepared.version(), LanguageVersion::new(0));
            // The layers compose into one specification without `parent`.
            assert_eq!(text(prepared.resolved(), "domain"), Some("TEST"));
            assert_eq!(text(prepared.resolved(), "yamaa"), Some("1.0"));
            let resolved = prepared.resolved();
            assert!(resolved.field(resolved.root(), "columns").is_some());
            assert!(resolved.field(resolved.root(), "parent").is_none());
            // A parent written for 1.1 that uses the 1.1 field, under a 1.0
            // child: each layer at its own version, the chain at the newest.
            study.write(
                "base.yaml",
                &BASE
                    .edit("\"1.0\"", "\"1.1\"")
                    .edit("  - {name: ID, type: int, derivation: SRC.ID}\n", ADDED),
            );
            let prepared = prepare(&study, "spec.yaml", reads, &closures).unwrap();
            let versions = prepared
                .layers()
                .iter()
                .map(|layer| (layer.source.identity.clone(), layer.version))
                .collect::<Vec<_>>();
            assert_eq!(
                versions,
                [
                    (study.path("base.yaml"), LanguageVersion::new(1)),
                    (study.path("spec.yaml"), LanguageVersion::new(0)),
                ]
            );
            assert_eq!(prepared.version(), LanguageVersion::new(1));
            // The resolved specification records the newest version.
            assert_eq!(text(prepared.resolved(), "yamaa"), Some("1.1"));
            // The same field written in the 1.0 child fails in that layer.
            study.write(
                "spec.yaml",
                &format!("{CHILD}columns:\n  - {{name: ID, added: late}}\n"),
            );
            let (layer, found) =
                layer_findings(prepare(&study, "spec.yaml", reads, &closures).unwrap_err());
            assert_eq!(layer, study.path("spec.yaml"));
            assert_eq!(
                found,
                [(
                    ConditionCode::SchemaUnknownFieldSince,
                    "columns.ID.added".into()
                )]
            );
        }

        #[test]
        fn a_parent_naming_an_unread_version_or_another_type_fails_at_parent() {
            let study = Study::new();
            let closures = closures();
            study.write("spec.yaml", CHILD);
            study.write("base.yaml", &BASE.edit("\"1.0\"", "\"1.2\""));
            let finding =
                traversal(prepare(&study, "spec.yaml", LANGUAGE_VERSION, &closures).unwrap_err());
            assert_eq!(
                finding.code,
                ConditionCode::InheritanceParentLanguageVersion
            );
            assert_eq!(finding.definition().rule, Some("LAYER-4"));
            assert_eq!(finding.spec_paths, ["parent"]);
            assert_eq!(
                Value::Object(finding.context.into_iter().collect()),
                json!({"source": study.path("base.yaml"), "entry": study.path("spec.yaml"),
                    "declaring_source": study.path("spec.yaml"),
                    "version": "1.2", "language_versions": ["1.0"]})
            );
            // Every layer of a chain declares the same type.
            study.write("base.yaml", "yamaa: \"1.0\"\ntype: project\n");
            let finding =
                traversal(prepare(&study, "spec.yaml", LANGUAGE_VERSION, &closures).unwrap_err());
            assert_eq!(finding.code, ConditionCode::InheritanceParentFileType);
            assert_eq!(finding.spec_paths, ["parent"]);
            assert_eq!(
                Value::Object(finding.context.into_iter().collect()),
                json!({"source": study.path("base.yaml"), "entry": study.path("spec.yaml"),
                    "declaring_source": study.path("spec.yaml"),
                    "type": "project", "expected": "spec"})
            );
            // A parent that declares `schema_version` is written in language
            // 0.5, which the engine does not read (SCHEMA-2).
            study.write("base.yaml", LEGACY);
            let finding =
                traversal(prepare(&study, "spec.yaml", LANGUAGE_VERSION, &closures).unwrap_err());
            assert_eq!(
                finding.code,
                ConditionCode::InheritanceParentLanguageVersion
            );
            assert_eq!(finding.spec_paths, ["parent"]);
            assert_eq!(
                Value::Object(finding.context.into_iter().collect()),
                json!({"source": study.path("base.yaml"), "entry": study.path("spec.yaml"),
                    "declaring_source": study.path("spec.yaml"),
                    "version": "0.5", "language_versions": ["1.0"]})
            );
            // A parent without the 1.x header and a malformed header fail in
            // that layer.
            for (text, code, path) in [
                (
                    "domain: TEST\n",
                    ConditionCode::SchemaMissingRequiredField,
                    "yamaa",
                ),
                (
                    "yamaa: 1.0\ntype: spec\n",
                    ConditionCode::SchemaInvalidFieldType,
                    "yamaa",
                ),
                (
                    "yamaa: \"1.0\"\n",
                    ConditionCode::SchemaMissingRequiredField,
                    "type",
                ),
            ] {
                study.write("base.yaml", text);
                let (layer, found) = layer_findings(
                    prepare(&study, "spec.yaml", LANGUAGE_VERSION, &closures).unwrap_err(),
                );
                assert_eq!(layer, study.path("base.yaml"));
                assert_eq!(found, [(code, path.to_string())]);
            }
        }

        #[test]
        fn a_layer_is_a_fragment_and_a_single_layer_is_complete() {
            let study = Study::new();
            let closures = closures();
            // A layer may leave out required root fields and member fields,
            // but a field it writes is checked, and keyed members are unique.
            study.write("spec.yaml", CHILD);
            study.write(
                "base.yaml",
                &format!("{BASE}  - {{name: ID, label: Again}}\nunknown: 1\n"),
            );
            let (_, found) = layer_findings(
                prepare(&study, "spec.yaml", LANGUAGE_VERSION, &closures).unwrap_err(),
            );
            assert_eq!(
                found,
                [
                    (ConditionCode::SchemaUnknownField, "unknown".into()),
                    (
                        ConditionCode::SchemaLayerIdentifierRepeated,
                        "columns.ID.name".into()
                    ),
                ]
            );
            // A specification of one layer is complete at its version.
            study.write("alone.yaml", SPEC);
            let prepared = prepare(&study, "alone.yaml", LANGUAGE_VERSION, &closures).unwrap();
            assert_eq!(prepared.layers().len(), 1);
            assert_eq!(text(prepared.resolved(), "domain"), Some("TEST"));
            study.write("alone.yaml", BASE);
            let SpecificationError::Entry(error) =
                prepare(&study, "alone.yaml", LANGUAGE_VERSION, &closures).unwrap_err()
            else {
                panic!("an incomplete specification");
            };
            let missing = findings(&error)
                .into_iter()
                .map(|(code, path, _)| (code, path))
                .collect::<Vec<_>>();
            assert!(missing.contains(&(ConditionCode::SchemaMissingRequiredField, "domain".into())));
        }

        /// LAYER-2 and SCHEMA-12: an empty `parent` fails schema admission in
        /// its layer, being shorter than a `path`'s minimum length of one,
        /// before any parent is resolved.
        #[test]
        fn an_empty_parent_fails_schema_admission_in_its_layer() {
            let study = Study::new();
            let closures = closures();
            study.write("spec.yaml", &CHILD.edit("parent: base.yaml", "parent: ''"));
            let (layer, found) = layer_findings(
                prepare(&study, "spec.yaml", LANGUAGE_VERSION, &closures).unwrap_err(),
            );
            assert_eq!(layer, study.path("spec.yaml"));
            assert_eq!(
                found,
                [(
                    ConditionCode::SchemaDescriptorMinimumLength,
                    "parent".to_string()
                )]
            );
            // A public check reports it at `parent`, in the entry.
            let issue = only(&study.check("spec.yaml", None)).clone();
            assert_eq!(
                (issue.phase(), issue.condition(), issue.rule()),
                ("validation", "minimum_length", Some("SCHEMA-12"))
            );
            assert_eq!(issue.spec_paths, ["parent"]);
            let entry = study.path("spec.yaml");
            assert_eq!(
                Value::Object(issue.context.clone()),
                json!({"minimum": 1, "source": entry, "entry": entry})
            );
        }

        /// LAYER-2 and SCHEMA-12: a layer names at most one parent, so a list
        /// written at `parent` fails as `invalid_field_type` in its layer,
        /// with the hint an undeclared `parents` key carries (SCHEMA-13).
        #[test]
        fn a_list_of_parents_carries_the_one_parent_hint() {
            let study = Study::new();
            study.write("base.yaml", BASE);
            let entry = study.path("spec.yaml");
            let hint = "write at most one parent per file; replace parents: [organization.yaml, compound.yaml] with parent: compound.yaml in the entry and parent: organization.yaml in compound.yaml; organization.yaml has no parent; a fragment that several files share goes under include and names no parent";
            for (written, condition, rule, path, context) in [
                (
                    "parent: [base.yaml, fragment.yaml]",
                    "invalid_field_type",
                    "SCHEMA-12",
                    "parent",
                    json!({"expected": "path", "actual": "sequence", "hint": hint, "source": entry, "entry": entry}),
                ),
                (
                    "parents: [base.yaml, fragment.yaml]",
                    "unknown_field",
                    "SCHEMA-13",
                    "parents",
                    json!({"field": "parents", "class": "root_class", "identifier": "parents", "hint": hint, "source": entry, "entry": entry}),
                ),
            ] {
                study.write("spec.yaml", &CHILD.edit("parent: base.yaml", written));
                let issues: Vec<_> = study
                    .check("spec.yaml", None)
                    .into_iter()
                    .map(|issue| {
                        (
                            issue.phase(),
                            issue.condition(),
                            issue.rule(),
                            issue.spec_paths,
                            Value::Object(issue.context),
                            issue.source_span,
                            issue.operand_route,
                        )
                    })
                    .collect();
                assert_eq!(
                    issues,
                    [(
                        "validation",
                        condition,
                        Some(rule),
                        vec![path.to_string()],
                        context,
                        None,
                        None
                    )],
                    "{written}"
                );
            }
        }

        /// The diagnostic at a referencing field whose file is refused.
        fn reference_refusal(
            error: ReferenceError<std::io::Error>,
        ) -> yamaa_core::diagnostic::Diagnostic {
            let ReferenceError::Refused(refused) = error else {
                panic!("a refused reference: {error:?}");
            };
            let ReferenceFault::Field(finding) = refused.fault else {
                panic!("a diagnostic at the field: {refused:?}");
            };
            finding
        }

        #[test]
        fn each_referencing_field_checks_the_file_it_names() {
            let study = Study::new();
            let check = |name: &str, file_type| {
                let path = study.path(name);
                let text = fs::read(study.0.join(name)).unwrap();
                let document = yamaa_adapters::yaml_decode::decode_yaml(&text)
                    .unwrap()
                    .document;
                check_references(
                    &document,
                    file_type,
                    &path,
                    &path,
                    LANGUAGE_VERSION,
                    &mut Files,
                )
            };
            study.write("dm.yaml", SPEC);
            study.write("bmi.yaml", "yamaa: \"1.0\"\ntype: function\n");
            study.write("ct.yaml", "yamaa: \"1.0\"\ntype: codelists\n");
            study.write(
                "spec.yaml",
                &SPEC.edit(
                    "input: {SRC: input.csv}",
                    "input: {SRC: {path: dm.csv, schema: dm.yaml}}",
                ),
            );
            study.write(
                "project.yaml",
                "yamaa: \"1.0\"\ntype: project\nfunctions: [bmi.yaml]\ncodelists: [ct.yaml]\nsdtm: {specs: [dm.yaml]}\n",
            );
            let fields = |checked: Vec<document::CheckedReference>| {
                checked
                    .into_iter()
                    .map(|checked| (checked.reference.field, checked.header.file_type))
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                fields(check("spec.yaml", FileType::Spec).unwrap()),
                [("input.SRC.schema".to_string(), FileType::Spec)]
            );
            assert_eq!(
                fields(check("project.yaml", FileType::Project).unwrap()),
                [
                    ("functions[0]".to_string(), FileType::Function),
                    ("codelists[0]".to_string(), FileType::Codelists),
                    ("sdtm.specs[0]".to_string(), FileType::Spec),
                ]
            );
            // Each field names the type it reads: a wrong type fails there.
            for (name, file_type, field, wrong, expected) in [
                (
                    "spec.yaml",
                    FileType::Spec,
                    "input.SRC.schema",
                    "dm.yaml",
                    "spec",
                ),
                (
                    "project.yaml",
                    FileType::Project,
                    "functions[0]",
                    "bmi.yaml",
                    "function",
                ),
                (
                    "project.yaml",
                    FileType::Project,
                    "codelists[0]",
                    "ct.yaml",
                    "codelists",
                ),
                (
                    "project.yaml",
                    FileType::Project,
                    "sdtm.specs[0]",
                    "dm.yaml",
                    "spec",
                ),
            ] {
                let original = fs::read_to_string(study.0.join(wrong)).unwrap();
                // A file of another type, then one newer than the engine reads.
                let other = if expected == "function" {
                    "project"
                } else {
                    "function"
                };
                study.write(wrong, &format!("yamaa: \"1.0\"\ntype: {other}\n"));
                let finding = reference_refusal(check(name, file_type).unwrap_err());
                assert_eq!(finding.code, ConditionCode::ReferenceFileType, "{field}");
                assert_eq!(finding.spec_paths, [field]);
                assert_eq!(
                    Value::Object(finding.context.into_iter().collect()),
                    json!({"source": study.path(wrong), "type": other, "expected": expected})
                );
                study.write(wrong, &original.edit("\"1.0\"", "\"1.3\""));
                let finding = reference_refusal(check(name, file_type).unwrap_err());
                assert_eq!(
                    finding.code,
                    ConditionCode::ReferenceLanguageVersion,
                    "{field}"
                );
                assert_eq!(finding.definition().rule, Some("SCHEMA-2"));
                assert_eq!(finding.spec_paths, [field]);
                assert_eq!(
                    Value::Object(finding.context.into_iter().collect()),
                    json!({"source": study.path(wrong), "version": "1.3",
                        "language_versions": ["1.0"]})
                );
                // A malformed header is a finding in the named file.
                study.write(wrong, "yamaa: 1.0\n");
                let ReferenceError::Refused(refused) = check(name, file_type).unwrap_err() else {
                    panic!("a refused {wrong}");
                };
                let ReferenceFault::Finding(finding) = refused.fault else {
                    panic!("a finding in {wrong}");
                };
                assert_eq!(finding.code, ConditionCode::SchemaInvalidFieldType);
                assert_eq!(finding.path, "yamaa");
                study.write(wrong, &original);
            }
            // A named file that does not exist is unavailable.
            fs::remove_file(study.0.join("bmi.yaml")).unwrap();
            assert!(matches!(
                check("project.yaml", FileType::Project).unwrap_err(),
                ReferenceError::Unavailable { reference } if reference.field == "functions[0]"
            ));
            // Fields are checked in the order they are written, so the first
            // refused file written is the one reported.
            study.write("bmi.yaml", "yamaa: \"1.0\"\ntype: spec\n");
            study.write("dm.yaml", "yamaa: \"1.0\"\ntype: function\n");
            for (text, field) in [
                (
                    "yamaa: \"1.0\"\ntype: project\nsdtm: {specs: [dm.yaml]}\nfunctions: [bmi.yaml]\n",
                    "sdtm.specs[0]",
                ),
                (
                    "yamaa: \"1.0\"\ntype: project\nfunctions: [bmi.yaml]\nsdtm: {specs: [dm.yaml]}\n",
                    "functions[0]",
                ),
            ] {
                study.write("project.yaml", text);
                let finding = reference_refusal(check("project.yaml", FileType::Project).unwrap_err());
                assert_eq!(finding.spec_paths, [field]);
            }
        }

        #[test]
        fn an_entry_relative_schema_resolves_from_the_entry_in_every_layer() {
            let study = Study::new();
            // A shared layer in another directory reads each study's own
            // producer through `relative_to: entry` (FILE-2).
            study.write(
                "shared/base.yaml",
                "yamaa: \"1.0\"\ntype: spec\ninput: {DM: {path: dm.csv, relative_to: entry, schema: dm.yaml}}\n",
            );
            study.write(
                "study/spec.yaml",
                CHILD.edit("base.yaml", "../shared/base.yaml").as_str(),
            );
            study.write("study/dm.yaml", SPEC);
            let base = study.path("shared/base.yaml");
            let entry = study.path("study/spec.yaml");
            let check = |declaring: &str| {
                let text = fs::read(declaring).unwrap();
                let document = yamaa_adapters::yaml_decode::decode_yaml(&text)
                    .unwrap()
                    .document;
                check_references(
                    &document,
                    FileType::Spec,
                    declaring,
                    &entry,
                    LANGUAGE_VERSION,
                    &mut Files,
                )
            };
            let [checked] = check(&base).unwrap().try_into().unwrap();
            assert_eq!(checked.reference.field, "input.DM.schema");
            assert_eq!(checked.source.identity, study.path("study/dm.yaml"));
            // Nothing is read beside the layer that writes the link: with no
            // producer beside the entry, the link is unavailable there.
            study.write("shared/dm.yaml", SPEC);
            fs::remove_file(study.0.join("study/dm.yaml")).unwrap();
            assert!(matches!(
                check(&base).unwrap_err(),
                ReferenceError::Unavailable { reference } if reference.field == "input.DM.schema"
            ));
            // With the default base the link resolves beside its layer.
            study.write(
                "shared/base.yaml",
                "yamaa: \"1.0\"\ntype: spec\ninput: {DM: {path: dm.csv, schema: dm.yaml}}\n",
            );
            let [checked] = check(&base).unwrap().try_into().unwrap();
            assert_eq!(checked.source.identity, study.path("shared/dm.yaml"));
        }

        #[test]
        fn a_schema_version_root_is_refused_as_language_0_5_wherever_it_is_read() {
            let study = Study::new();
            let closures = closures();
            study.write(
                "consumer.yaml",
                &SPEC.edit(
                    "input: {SRC: input.csv}",
                    "input: {SRC: {path: dm.csv, schema: base.yaml}}",
                ),
            );
            let consumer = study.path("consumer.yaml");
            let base = study.path("base.yaml");
            // `schema_version`, with `yamaa` beside it or not, declares
            // language 0.5, which no route reads (SCHEMA-2).
            let both = format!("{LEGACY}yamaa: \"1.0\"\ntype: spec\n");
            for text in [LEGACY, both.as_str()] {
                // As the file a call reads, at its own `schema_version`.
                let error = document::decode(
                    source("base.yaml", text),
                    FileType::Spec,
                    Some(LANGUAGE_VERSION),
                    &closures,
                )
                .unwrap_err();
                assert_eq!(
                    findings(&error),
                    [(
                        ConditionCode::HeaderLanguageVersion,
                        "schema_version".to_string(),
                        vec![
                            ("version", C::Text("0.5".into())),
                            ("language_versions", C::TextList(vec!["1.0".into()])),
                        ],
                    )]
                );
                // As the parent of a 1.x chain, at `parent` (LAYER-4).
                study.write("spec.yaml", CHILD);
                study.write("base.yaml", text);
                let finding = traversal(
                    prepare(&study, "spec.yaml", LANGUAGE_VERSION, &closures).unwrap_err(),
                );
                assert_eq!(
                    (finding.code, finding.spec_paths.clone()),
                    (
                        ConditionCode::InheritanceParentLanguageVersion,
                        vec!["parent".to_string()]
                    )
                );
                assert_eq!(
                    Value::Object(finding.context.into_iter().collect()),
                    json!({"source": base, "entry": study.path("spec.yaml"),
                        "declaring_source": study.path("spec.yaml"),
                        "version": "0.5", "language_versions": ["1.0"]})
                );
                // As the file a referencing field names, at that field.
                let text = fs::read(&consumer).unwrap();
                let document = yamaa_adapters::yaml_decode::decode_yaml(&text)
                    .unwrap()
                    .document;
                let ReferenceError::Refused(refused) = check_references(
                    &document,
                    FileType::Spec,
                    &consumer,
                    &consumer,
                    LANGUAGE_VERSION,
                    &mut Files,
                )
                .unwrap_err() else {
                    panic!("a refused producer");
                };
                let ReferenceFault::Field(diagnostic) = refused.fault else {
                    panic!("refused at the field: {refused:?}");
                };
                assert_eq!(
                    (diagnostic.code, diagnostic.spec_paths.clone()),
                    (
                        ConditionCode::ReferenceLanguageVersion,
                        vec!["input.SRC.schema".to_string()]
                    )
                );
                assert_eq!(
                    Value::Object(diagnostic.context.into_iter().collect()),
                    json!({"source": base, "version": "0.5", "language_versions": ["1.0"]})
                );
            }
        }
    }
}
