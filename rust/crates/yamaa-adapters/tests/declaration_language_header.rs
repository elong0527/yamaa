#![cfg(feature = "yaml")]
//! The 1.x header (SCHEMA-2, LAYER-4, P-VER-1).
//!
//! A root that declares `schema_version` keeps the 0.5 path; a root that
//! declares `yamaa` is a 1.x document. Until the engine reads 1.0 files
//! (#3030 D76) every public read refuses one as `unsupported_language_version`,
//! at the place it is read. The 1.0 path, which only tests reach, reads the
//! header before choosing a closure, admits each document at the version it
//! declares and each specification layer at its own, and checks the file
//! every referencing field names.
mod declaration {
    mod language_header {
        use std::fs;
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use yamaa_adapters::document::{
            self, check_references, Closures, Decoded, ReferenceError, SpecificationError,
        };
        use yamaa_adapters::json;
        use yamaa_adapters::specification_report::Identity;
        use yamaa_adapters::specification_source::{
            CapturedSchema, Error, InheritancePort, Source,
        };
        use yamaa_core::diagnostic::{ConditionCode, Diagnostic as Issue};
        use yamaa_core::domain_entry::Request;
        use yamaa_core::inheritance::{self as graph, Source as GraphSource, SourceError};
        use yamaa_core::json::Value;
        use yamaa_core::language::{FileType, LanguageVersion, ReferenceFault, LANGUAGE_VERSION};
        use yamaa_core::project_function::Language;
        use yamaa_core::schema::{NormalizationError, SchemaContext as C};

        static NEXT: AtomicUsize = AtomicUsize::new(0);

        /// A complete 0.5 specification reading one CSV input.
        const LEGACY: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
        /// A complete 1.0 specification of the same dataset.
        const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
        /// A complete 1.0 function file.
        const FUNCTION: &str = "yamaa: \"1.0\"\ntype: function\nname: bmi\ndescription: Body mass index.\nparams: [{name: weight, type: float}]\nreturns: float\nimplementations: {python: project.bmi}\ntests: [{id: normal, covers: [normal], args: {weight: 1.0}, result: 1.0}]\n";

        struct Study(PathBuf);
        impl Study {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-language-header-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir_all(&path).unwrap();
                Self(yamaa_adapters::canonical_path::canonicalize(path).unwrap())
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
                    Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: yamaa_core::VERSION,
                        example: "test",
                        specification: &specification,
                        base_directory: ".",
                    },
                )
            }
        }
        impl Drop for Study {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
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

        #[test]
        fn a_public_check_refuses_a_1_x_specification_it_reads() {
            let study = Study::new();
            // The engine reads no 1.x version until the switch (D76), so even
            // the version this engine implements is refused, at `yamaa`.
            study.write("spec.yaml", SPEC);
            refused(
                only(&study.check("spec.yaml", None)),
                "SCHEMA-2",
                "yamaa",
                json!({"version": "1.0", "language_versions": []}),
            );
            // An entry that names a parent is refused before the parent is read.
            study.write("spec.yaml", &format!("{SPEC}parent: missing.yaml\n"));
            refused(
                only(&study.check("spec.yaml", None)),
                "SCHEMA-2",
                "yamaa",
                json!({"version": "1.0", "language_versions": [],
                    "source": study.path("spec.yaml"), "entry": study.path("spec.yaml")}),
            );
            // Any declared version, whatever the rest of the file holds.
            study.write("spec.yaml", "yamaa: \"1.4\"\nanything: [1, 2]\n");
            refused(
                only(&study.check("spec.yaml", None)),
                "SCHEMA-2",
                "yamaa",
                json!({"version": "1.4", "language_versions": []}),
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
        fn schema_version_keeps_the_0_5_path_unchanged() {
            let study = Study::new();
            study.write("input.csv", "ID\n1\n");
            study.write("spec.yaml", LEGACY);
            assert!(study.check("spec.yaml", None).is_empty());
            // Beside `schema_version`, `yamaa` is an unknown 0.5 field.
            study.write("spec.yaml", &format!("{LEGACY}yamaa: \"1.0\"\n"));
            let issue = only(&study.check("spec.yaml", None)).clone();
            assert_eq!(issue.condition(), "unknown_field");
            assert_eq!(issue.spec_paths, ["yamaa"]);
            // A file declaring neither keeps today's version finding.
            study.write("spec.yaml", "domain: TEST\n");
            let issue = only(&study.check("spec.yaml", None)).clone();
            assert_eq!(issue.condition(), "schema_version_mismatch");
            assert_eq!(issue.spec_paths, ["schema_version"]);
        }

        #[test]
        fn a_0_5_chain_refuses_a_1_x_parent_at_the_parent_naming_it() {
            let study = Study::new();
            study.write("base.yaml", SPEC);
            study.write(
                "spec.yaml",
                "schema_version: '1.0'\nparent: base.yaml\ndomain: TEST\n",
            );
            refused(
                only(&study.check("spec.yaml", None)),
                "LAYER-4",
                "parent",
                json!({"version": "1.0", "language_versions": [],
                    "source": study.path("base.yaml"), "entry": study.path("spec.yaml")}),
            );
            // A parent whose version is unquoted fails in that layer.
            study.write("base.yaml", "yamaa: 1.0\n");
            let issue = only(&study.check("spec.yaml", None)).clone();
            assert_eq!(issue.condition(), "invalid_field_type");
            assert_eq!(issue.spec_paths, ["yamaa"]);
            assert_eq!(issue.context["source"], study.path("base.yaml"));
        }

        #[test]
        fn a_0_5_schema_link_refuses_a_1_x_producer() {
            let study = Study::new();
            study.write("producer.yaml", SPEC);
            study.write(
                "spec.yaml",
                &LEGACY.replace(
                    "input: {SRC: input.csv}",
                    "input: {SRC: {path: output.csv, schema: producer.yaml}}",
                ),
            );
            // A producer's findings keep their own paths, as every 0.5
            // producer finding does.
            refused(
                only(&study.check("spec.yaml", None)),
                "SCHEMA-2",
                "yamaa",
                json!({"version": "1.0", "language_versions": []}),
            );
        }

        #[test]
        fn a_0_5_project_refuses_each_1_x_file_it_reads() {
            let study = Study::new();
            study.write("input.csv", "ID\n1\n");
            study.write("spec.yaml", LEGACY);
            // The project file a call names.
            study.write("project.yaml", "yamaa: \"1.0\"\ntype: project\n");
            refused(
                only(&study.check("spec.yaml", Some("project.yaml"))),
                "SCHEMA-2",
                "yamaa",
                json!({"version": "1.0", "language_versions": [],
                    "source": study.path("project.yaml"), "entry": study.path("project.yaml"),
                    "environment_path": "$"}),
            );
            // The environment file the project links, a function file it
            // lists and a codelist file the project lists: each at its own
            // `yamaa`, naming the field that read it.
            study.write(
                "project.yaml",
                "schema_version: '1.0'\nenvironments: {python: python/environment.yaml}\n",
            );
            study.write("python/environment.yaml", "yamaa: \"1.0\"\n");
            refused(
                only(&study.check("spec.yaml", Some("project.yaml"))),
                "SCHEMA-2",
                "yamaa",
                json!({"version": "1.0", "language_versions": [],
                    "source": study.path("python/environment.yaml"),
                    "entry": study.path("project.yaml"),
                    "environment_path": "environments.python"}),
            );
            study.write(
                "python/environment.yaml",
                "schema_version: '1.0'\nlock: uv.lock\nfunctions: {bmi: bmi.yaml}\n",
            );
            study.write("python/uv.lock", "version = 1\npackage = []\n");
            study.write(
                "python/bmi.yaml",
                "yamaa: \"1.0\"\ntype: function\nname: bmi\n",
            );
            refused(
                only(&study.check("spec.yaml", Some("project.yaml"))),
                "SCHEMA-2",
                "yamaa",
                json!({"version": "1.0", "language_versions": [],
                    "source": study.path("python/bmi.yaml"),
                    "entry": study.path("project.yaml"),
                    "environment_path": "functions.bmi"}),
            );
            study.write(
                "project.yaml",
                "schema_version: '1.0'\ncodelists: [ct.yaml]\n",
            );
            study.write(
                "ct.yaml",
                "yamaa: \"1.0\"\ntype: codelists\ncodelists: []\n",
            );
            refused(
                only(&study.check("spec.yaml", Some("project.yaml"))),
                "SCHEMA-2",
                "yamaa",
                json!({"version": "1.0", "language_versions": [],
                    "source": study.path("ct.yaml"),
                    "entry": study.path("project.yaml"),
                    "environment_path": "codelists[0]"}),
            );
        }

        // -- The 1.0 path, reached only by tests until the switch --------------

        /// Files resolved beside their declaring file.
        struct Files;
        impl InheritancePort for Files {
            type Error = std::io::Error;
            fn canonicalize(
                &mut self,
                declaring: &str,
                written: &str,
            ) -> Result<GraphSource, SourceError<Self::Error>> {
                let candidate = Path::new(declaring).parent().unwrap().join(written);
                let identity = yamaa_adapters::canonical_path::canonicalize(&candidate)
                    .map_err(|_| SourceError::Unavailable)?;
                let identity = identity.to_str().unwrap().replace('\\', "/");
                Ok(GraphSource {
                    display_path: identity.clone(),
                    identity,
                })
            }
            fn capture(
                &mut self,
                source: &GraphSource,
            ) -> Result<Vec<u8>, SourceError<Self::Error>> {
                fs::read(&source.identity).map_err(SourceError::Raised)
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
                Ok(Decoded::Versioned(document)) => document,
                other => panic!("{identity}: {other:?}"),
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
            // `schema_version`, or no `yamaa`, keeps the 0.5 path.
            for text in [LEGACY, "domain: TEST\n", "name: bmi\n"] {
                assert!(matches!(
                    document::decode(
                        source("legacy.yaml", text),
                        FileType::Spec,
                        Some(LANGUAGE_VERSION),
                        &closures
                    ),
                    Ok(Decoded::Legacy { .. })
                ));
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
            // Before the switch a public call reads no 1.x version.
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
                source("spec.yaml", &SPEC.replace("\"1.0\"", "\"1.1\"")),
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
                            text.replace("values: [\"1.0\"]", "values: [\"1.0\", \"1.1\"]")
                        }
                        "schema_spec.yaml" => text.replace(
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
            let with_added = SPEC.replace("  - {name: ID, type: int, derivation: SRC.ID}\n", ADDED);
            // 1.1 admits the field 1.1 introduced.
            let newer = with_added.replace("\"1.0\"", "\"1.1\"");
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
                &SPEC.replace("derivation: SRC.ID}", "derivation: SRC.ID, other: x}"),
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
                    .replace("\"1.0\"", "\"1.1\"")
                    .replace("  - {name: ID, type: int, derivation: SRC.ID}\n", ADDED),
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
            study.write("base.yaml", &BASE.replace("\"1.0\"", "\"1.2\""));
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
                    "type": "project", "expected": "spec"})
            );
            // A parent without the 1.x header, a 0.5 layer among them, and a
            // malformed header fail in that layer.
            for (text, code, path) in [
                (LEGACY, ConditionCode::SchemaMissingRequiredField, "yamaa"),
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
        /// its layer, as a `path` shorter than one, before any parent is
        /// resolved.
        #[test]
        fn an_empty_parent_fails_schema_admission_in_its_layer() {
            let study = Study::new();
            let closures = closures();
            study.write(
                "spec.yaml",
                &CHILD.replace("parent: base.yaml", "parent: ''"),
            );
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
                &SPEC.replace(
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
                study.write(wrong, &original.replace("\"1.0\"", "\"1.3\""));
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
                CHILD.replace("base.yaml", "../shared/base.yaml").as_str(),
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
        fn a_yamaa_beside_schema_version_routes_to_the_0_5_path_wherever_it_is_read() {
            let study = Study::new();
            let closures = closures();
            let both = format!("{LEGACY}yamaa: \"1.0\"\ntype: spec\n");
            // The decoder hands the file to the 0.5 path, where `yamaa` is
            // an unknown field (`schema_version_keeps_the_0_5_path_unchanged`).
            assert!(matches!(
                document::decode(
                    source("both.yaml", &both),
                    FileType::Spec,
                    Some(LANGUAGE_VERSION),
                    &closures
                ),
                Ok(Decoded::Legacy { .. })
            ));
            let unknown = |found: &[(ConditionCode, String)]| {
                assert_eq!(
                    found,
                    [(ConditionCode::SchemaUnknownField, "yamaa".to_string())]
                );
            };
            // As a parent of a 1.x chain it fails in that layer the same way.
            study.write("spec.yaml", CHILD);
            study.write("base.yaml", &both);
            let (layer, found) = layer_findings(
                prepare(&study, "spec.yaml", LANGUAGE_VERSION, &closures).unwrap_err(),
            );
            assert_eq!(layer, study.path("base.yaml"));
            unknown(&found);
            // And as the file a referencing field names.
            study.write(
                "consumer.yaml",
                &SPEC.replace(
                    "input: {SRC: input.csv}",
                    "input: {SRC: {path: dm.csv, schema: base.yaml}}",
                ),
            );
            let consumer = study.path("consumer.yaml");
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
            let ReferenceFault::Finding(finding) = refused.fault else {
                panic!("a finding in the producer: {refused:?}");
            };
            unknown(&[(finding.code, finding.path)]);
        }
    }
}
