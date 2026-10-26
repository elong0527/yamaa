//! Preparation contracts: schema admission and source profile inspection.
//!
//! The seam is `SchemaStructure::admit`'s descriptors, origins and ordered
//! `BundleIssue`s, and `source_profile_inspection::refine` with a scripted
//! inspection callback, neither of which a replay case or benchmark reaches.
mod preparation {
    mod schema_admission {
        //! A decoded closure's semantic admission (`SchemaStructure::admit`):
        //! its descriptors, defaults and origins, and its ordered bundle
        //! issues. The adapters decode each module before it, in the order
        //! supplied.
        use yamaa_core::schema::{
            BundleError, BundleIssue, BundleIssueKind, Document, DocumentNode as N,
            NormalizationError, SchemaModule, SchemaOrigin, SchemaSource, SchemaStructure,
        };

        use crate::support::document::Tree;

        fn text(s: &str) -> Tree {
            Tree::text(s)
        }
        fn map(items: Vec<(&str, Tree)>) -> Tree {
            Tree::Map(items.into_iter().map(|(k, v)| (k.into(), v)).collect())
        }
        fn document(tree: Tree) -> Document {
            tree.document()
        }
        fn module(declarations: Vec<(&str, Tree)>, includes: &[&str]) -> Document {
            let mut fields = vec![
                ("version", text("1.0")),
                (
                    "includes",
                    Tree::List(includes.iter().map(|s| text(s)).collect()),
                ),
            ];
            fields.extend(declarations);
            document(map(fields))
        }
        fn named(name: &str, document: Document) -> SchemaModule {
            SchemaModule {
                name: name.into(),
                document,
            }
        }
        fn descriptor(kind: &str) -> Tree {
            map(vec![("type", text(kind))])
        }
        /// A shared module and the root that includes it, in that order.
        fn fixtures() -> Vec<SchemaModule> {
            let shared = module(
                vec![(
                    "shared",
                    Tree::List(vec![
                        map(vec![(
                            "id",
                            map(vec![
                                ("type", text("str")),
                                ("required", Tree::boolean(true)),
                            ]),
                        )]),
                        map(vec![(
                            "label",
                            map(vec![("type", text("str")), ("default", text("untitled"))]),
                        )]),
                    ]),
                )],
                &[],
            );
            let root = module(
                vec![(
                    "root_class",
                    Tree::List(vec![
                        map(vec![("fields_from", text("shared"))]),
                        map(vec![("yamaa", descriptor("str"))]),
                    ]),
                )],
                &["schema_s.yaml"],
            );
            vec![named("schema_s.yaml", shared), named("r", root)]
        }
        #[test]
        fn module_order_descriptor_defaults_and_origins_survive_admission() {
            let schema = SchemaStructure::admit(fixtures(), 1, "root_class").unwrap();
            assert_eq!(
                schema
                    .modules()
                    .iter()
                    .map(|module| module.name.as_str())
                    .collect::<Vec<_>>(),
                ["schema_s.yaml", "r"]
            );
            assert_eq!(schema.version(), "1.0");
            assert_eq!(
                schema
                    .root_class()
                    .fields
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>(),
                ["id", "label", "yamaa"]
            );
            let label = schema.root_class().fields[1].descriptor;
            let origin = &schema.descriptors()[label];
            assert_eq!(
                (origin.module, origin.path.as_str()),
                (0, "root_class.label")
            );
            let default = origin.descriptor.default_node().unwrap();
            assert_eq!(
                schema.modules()[0].document.nodes()[default],
                N::Text("untitled".into())
            );
            // A complete 1.x document, normalized once its header is read.
            let input = document(map(vec![("yamaa", text("1.0")), ("id", text("ID01"))]));
            let normalized = schema.normalize_root(&input).unwrap();
            let value = normalized
                .document
                .field(normalized.document.root(), "label")
                .unwrap();
            assert_eq!(
                normalized.document.nodes()[value],
                N::Text("untitled".into())
            );
            assert_eq!(
                normalized.origins[value],
                SchemaOrigin {
                    source: SchemaSource::Default {
                        module: 0,
                        descriptor: label
                    },
                    node: default,
                    generated: false
                }
            );
        }
        #[test]
        fn bundle_reports_every_issue_in_order_before_entry_and_name_errors() {
            let root = module(
                vec![(
                    "root_class",
                    Tree::List(vec![
                        map(vec![("first", descriptor("unknown_one"))]),
                        map(vec![("second", descriptor("unknown_two"))]),
                    ]),
                )],
                &[],
            );
            assert_eq!(
                SchemaStructure::admit(vec![named("r", root)], 0, "root_class").unwrap_err(),
                BundleError::Invalid(vec![
                    BundleIssue {
                        module: 0,
                        path: "root_class.first".into(),
                        kind: BundleIssueKind::UnknownType("unknown_one".into())
                    },
                    BundleIssue {
                        module: 0,
                        path: "root_class.second".into(),
                        kind: BundleIssueKind::UnknownType("unknown_two".into())
                    },
                ]),
                "two unknown types"
            );
            for (entry, issue, label) in [
                (
                    0,
                    BundleIssue {
                        module: 1,
                        path: "$".into(),
                        kind: BundleIssueKind::DuplicateModule("schema_s.yaml".into()),
                    },
                    "a duplicate module name",
                ),
                (
                    usize::MAX,
                    BundleIssue {
                        module: usize::MAX,
                        path: "$".into(),
                        kind: BundleIssueKind::ModuleName,
                    },
                    "an entry no module holds",
                ),
            ] {
                let mut modules = fixtures();
                modules[1].name = "schema_s.yaml".into();
                assert_eq!(
                    SchemaStructure::admit(modules, entry, "root_class").unwrap_err(),
                    BundleError::Invalid(vec![issue]),
                    "{label}"
                );
            }
        }
        #[test]
        fn invalid_defaults_remain_a_later_normalization_gate() {
            let schema = module(
                vec![(
                    "root_class",
                    Tree::List(vec![map(vec![(
                        "value",
                        map(vec![
                            ("type", text("int")),
                            ("default", text("invalid integer")),
                        ]),
                    )])]),
                )],
                &[],
            );
            let admitted =
                SchemaStructure::admit(vec![named("r", schema)], 0, "root_class").unwrap();
            let input = document(map(vec![]));
            assert!(matches!(
                admitted.normalize_root(&input),
                Err(NormalizationError::Defaults(_))
            ));
        }
        #[test]
        fn large_closed_schema_admits_every_module() {
            let names = (0..200)
                .map(|i| format!("schema_m{i}.yaml"))
                .collect::<Vec<_>>();
            let mut modules = names
                .iter()
                .map(|name| named(name, module(vec![], &[])))
                .collect::<Vec<_>>();
            modules.push(named(
                "root",
                module(
                    vec![(
                        "root_class",
                        Tree::List(vec![map(vec![("value", descriptor("str"))])]),
                    )],
                    &names.iter().map(String::as_str).collect::<Vec<_>>(),
                ),
            ));
            let schema = SchemaStructure::admit(modules, 200, "root_class").unwrap();
            assert_eq!(schema.modules().len(), 201);
        }
    }

    mod source_profile_inspection {
        use yamaa_core::source_profile_inspection;
        use yamaa_core::{
            resource::ResourceFailure,
            specification::{PreflightFinding, PrepareError, UnsupportedFeature},
        };

        fn unknown() -> PrepareError {
            PrepareError::Invalid(vec![PreflightFinding::UnknownSourceProfile {
                dataset: "SRC".into(),
                path: "input.unknown".into(),
            }])
        }

        #[test]
        fn physical_kind_refines_unknown_profile_without_byte_authority() {
            for (failure, condition) in [
                (
                    ResourceFailure::NotRegularFile,
                    "resource_path_not_regular_file",
                ),
                (ResourceFailure::Symlink, "resource_path_symlink"),
                (ResourceFailure::Missing, "source_profile_unknown"),
            ] {
                let mut inspected = Vec::new();
                let error = source_profile_inspection::refine(unknown(), |dataset, path| {
                    inspected.push((dataset.to_owned(), path.to_owned()));
                    Some((failure, "../input.unknown".into()))
                });
                assert_eq!(inspected, [("SRC".into(), "input.unknown".into())]);
                let PrepareError::Invalid(findings) = error else {
                    panic!("invalid findings")
                };
                let issue = findings[0].diagnostic();
                assert_eq!(issue.definition().condition, condition);
                assert_eq!(issue.spec_paths, ["input.SRC.path"]);
            }
            let PrepareError::Invalid(findings) =
                source_profile_inspection::refine(unknown(), |_, _| None)
            else {
                panic!("invalid findings")
            };
            assert_eq!(
                findings[0].diagnostic().definition().condition,
                "source_profile_unknown"
            );
        }

        #[test]
        fn unsupported_and_other_static_findings_do_not_enter_inspection() {
            let unsupported = PrepareError::Unsupported(vec![UnsupportedFeature {
                operation: "future_operation".into(),
                path: "columns.X".into(),
            }]);
            assert!(matches!(
                source_profile_inspection::refine(unsupported, |_, _| panic!(
                    "unsupported entered study metadata"
                )),
                PrepareError::Unsupported(_)
            ));
            let original = PreflightFinding::UndeclaredKey {
                position: 0,
                column: "ID".into(),
            };
            let expected = original.diagnostic();
            let PrepareError::Invalid(findings) =
                source_profile_inspection::refine(PrepareError::Invalid(vec![original]), |_, _| {
                    panic!("unrelated finding entered study metadata")
                })
            else {
                panic!("invalid findings")
            };
            assert_eq!(findings[0].diagnostic(), expected);
        }
    }
}
