#![cfg(feature = "yaml")]
//! The 1.x header's internal seams (SCHEMA-1, SCHEMA-2, LAYER-4, P-VER-1,
//! FILE-2).
//!
//! A root that declares `yamaa` is a 1.x document, and one that declares
//! `schema_version` is written in language 0.5. The engine reads language
//! version 1.0 (#3030 D76). What a public call reports for each file's header
//! is in the application suite's `fixtures/diagnostics/language_header.json`,
//! which its replay reads.
//! These tests prove what no public call returns: the 1.0 path reads the
//! header before choosing a closure, admits each document at the version it
//! declares and each specification layer at its own, under closures built
//! here for an invented version 1.1, prepares the layers it composes, and
//! returns each file a referencing field names.
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
        use yamaa_core::diagnostic::ConditionCode;
        use yamaa_core::inheritance::{self as graph, Source as GraphSource};
        use yamaa_core::json::Value;
        use yamaa_core::language::{FileType, LanguageVersion, ReferenceFault, LANGUAGE_VERSION};
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

        struct Study(crate::support::scratch::Scratch);
        impl Study {
            fn new() -> Self {
                let path = crate::support::scratch::Scratch::canonical("language-header");
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
            // Seam: `document::decode`'s choice of closure, the very shipped
            // closure of each declared type, and a decoder that reads no 1.x
            // version, which no public call reaches.
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
            // Seam: the type `document::decode` is asked for, including a
            // function or codelists file where a public call fixes the type
            // it reads.
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
            // Seam: closures built here for an invented version 1.1, which
            // the shipped engine does not read.
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
            // Seam: closures built here for an invented version 1.1, and the
            // layers, versions and resolved document a prepared specification
            // holds, which no public call returns.
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
        fn a_single_layer_is_complete_at_its_version() {
            // Seam: the layers and the resolved document a prepared
            // specification holds, and the refusal of its entry
            // (`SpecificationError::Entry`), which no public call returns. A
            // layer of several, a fragment whose written fields are checked,
            // is `language_header.json::a_layer_is_a_fragment_whose_written_fields_are_checked`.
            let study = Study::new();
            let closures = closures();
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
            // Seam: what `check_references` returns, each field's checked
            // reference with its file's header, an unavailable file and a
            // finding in the named file as its own faults, which no public
            // call returns as such.
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
            // Seam: `check_references` called for a layer whose declaring file
            // is not the entry, and the identity of the file it resolves,
            // which no public call returns.
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
    }
}
