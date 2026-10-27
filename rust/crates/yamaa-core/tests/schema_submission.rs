//! Schema contracts: specification submission document, submission composition,
//! submission document and submission values. The cases that compose a
//! study's value-level entries and compare its findings are
//! `fixtures/diagnostics/submission.json`; each test here proves a seam the
//! replay cannot reach, which its first comment names (#3798).
mod schema {
    mod specification_submission_document {
        use yamaa_core::{
            cdisc_vocabulary::{Core, DataType, MethodType, OriginSource, OriginType, PageType},
            schema::{Document, DocumentNode as N},
            specification_submission_document::{self as submission, *},
        };

        // These arenas and expected declarations are authored independently of the
        // decoder, compiler, schema normalization and submission generators.
        #[derive(Default)]
        struct Arena(Vec<N>);
        impl Arena {
            fn node(&mut self, value: N) -> usize {
                let id = self.0.len();
                self.0.push(value);
                id
            }
            fn text(&mut self, text: &str) -> usize {
                self.node(N::Text(text.into()))
            }
            fn map(&mut self, fields: Vec<(&str, usize)>) -> usize {
                let pairs = fields
                    .into_iter()
                    .map(|(key, value)| (self.text(key), value))
                    .collect();
                self.node(N::Mapping(pairs))
            }
            fn texts(&mut self, fields: &[(&str, &str)]) -> usize {
                let fields = fields
                    .iter()
                    .map(|&(key, value)| (key, self.text(value)))
                    .collect();
                self.map(fields)
            }
            fn column(&mut self, name: &str, kind: &str, submission: Option<usize>) -> usize {
                let name = self.text(name);
                let kind = self.text(kind);
                let mut fields = vec![("name", name), ("type", kind)];
                if let Some(value) = submission {
                    fields.push(("submission", value));
                }
                self.map(fields)
            }
            fn root(&mut self, dataset: Option<usize>, columns: Vec<usize>) -> usize {
                let columns = self.node(N::Sequence(columns));
                let mut fields = vec![("columns", columns)];
                if let Some(node) = dataset {
                    fields.push(("submission", node));
                }
                self.map(fields)
            }
            fn dataset(&mut self, class: &str, subclass: Option<&str>) -> usize {
                let label = self.text("Independent dataset");
                let class = self.text(class);
                let structure = self.text("One record per subject");
                let repeating = self.node(N::Boolean(false));
                let mut fields = vec![
                    ("label", label),
                    ("class", class),
                    ("structure", structure),
                    ("repeating", repeating),
                ];
                if let Some(subclass) = subclass {
                    fields.push(("subclass", self.text(subclass)));
                }
                self.map(fields)
            }
            fn finish(self, root: usize) -> Document {
                Document::new(self.0, root).unwrap()
            }
        }
        fn decode(input: &Document) -> Declarations {
            submission::decode(input, input.root()).unwrap()
        }
        fn findings(input: &Document) -> Vec<Finding> {
            let error = submission::decode(input, input.root()).unwrap_err();
            assert!(std::ptr::eq(input, error.input));
            match error.cause {
                Cause::Findings(values) => values,
                cause => panic!("unexpected {cause:?}"),
            }
        }
        fn columns(owner: &Declarations) -> &[Column] {
            &owner.columns().unwrap().value
        }

        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn locations_borrow_original_ordered_occurrences_without_accepting_nulls() {
            let mut a = Arena::default();
            let null_one = a.node(N::Null);
            let null_two = a.node(N::Null);
            let first = a.column("B", "str", Some(null_one));
            let second = a.column("A", "int", None);
            let root = a.root(Some(null_two), vec![first, second]);
            let input = a.finish(root);
            let locations = submission::locate(&input, root).unwrap();
            assert!(std::ptr::eq(locations.input(), &input));
            assert_eq!(locations.dataset().unwrap().node, Some(null_two));
            assert_eq!(
                locations
                    .columns()
                    .iter()
                    .map(|v| v.node)
                    .collect::<Vec<_>>(),
                [first, second]
            );
            assert_eq!(locations.columns()[0].submission.node, Some(null_one));
            assert_eq!(locations.columns()[1].submission.node, None);
            assert_eq!(
                findings(&input)
                    .iter()
                    .filter(|v| v.kind == Kind::ExpectedMapping)
                    .count(),
                2
            );
        }
        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn locations_keep_every_column_and_original_submission_occurrence() {
            let mut a = Arena::default();
            let one = a.node(N::Null);
            let two = a.node(N::Null);
            let first = a.column("A", "str", Some(one));
            let second = a.column("B", "str", Some(two));
            let root = a.root(None, vec![first, second]);
            let input = a.finish(root);
            let locations = submission::locate(&input, root).unwrap();
            assert!(std::ptr::eq(locations.input(), &input));
            assert_eq!(
                locations
                    .columns()
                    .iter()
                    .map(|v| v.node)
                    .collect::<Vec<_>>(),
                [first, second]
            );
            assert_eq!(locations.columns()[0].submission.node, Some(one));
            assert_eq!(locations.columns()[1].submission.node, Some(two));
        }

        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn locations_absence_and_original_shapes_remain_distinct() {
            let empty = Document::new(vec![N::Mapping(vec![])], 0).unwrap();
            let locations = submission::locate(&empty, 0).unwrap();
            assert!(locations.dataset().is_none());
            assert!(locations.columns().is_empty());
            let input = Document::new(
                vec![N::Integer("7".into()), N::Null, N::Mapping(vec![(0, 1)])],
                2,
            )
            .unwrap();
            for (root, node) in [(0, 0), (2, 0), (99, 99)] {
                let error = submission::locate(&input, root).unwrap_err();
                assert_eq!(error.cause, Cause::Shape { node });
                assert!(std::ptr::eq(error.input, &input));
            }
            let mut a = Arena::default();
            let scalar = a.text("columns are not a scalar");
            let root = a.map(vec![("columns", scalar)]);
            let input = a.finish(root);
            assert_eq!(
                submission::locate(&input, root).unwrap_err().cause,
                Cause::Shape { node: scalar }
            );
        }
        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn absent_submission_remains_absent_and_preserves_ordinary_column_declarations() {
            let mut a = Arena::default();
            let first = a.column("B", "int", None);
            let label = a.text("");
            let name = a.text("A");
            let kind = a.text("str");
            let ignored = a.node(N::Null);
            let second = a.map(vec![
                ("label", label),
                ("type", kind),
                ("name", name),
                ("derivation", ignored),
            ]);
            let root = a.root(None, vec![first, second]);
            let input = a.finish(root);
            let owner = decode(&input);
            assert!(owner.dataset().is_none());
            assert_eq!(columns(&owner).len(), 2);
            assert_eq!(columns(&owner)[0].name.value, "B");
            assert_eq!(columns(&owner)[0].column_type.value, ColumnType::Integer);
            assert_eq!(columns(&owner)[1].label.as_ref().unwrap().value, "");
            assert_eq!(
                columns(&owner)[1]
                    .fields
                    .iter()
                    .map(|v| v.field)
                    .collect::<Vec<_>>(),
                [FieldName::Label, FieldName::Type, FieldName::Name]
            );
            assert!(columns(&owner).iter().all(|v| v.submission.is_none()));
        }
        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn dataset_defaults_and_comment_survive_original_document_drop() {
            let mut a = Arena::default();
            let label = a.text("DM");
            let class = a.text("SPECIAL PURPOSE");
            let structure = a.text("One record per subject");
            let repeating = a.node(N::Boolean(false));
            let no_data = a.node(N::Boolean(true));
            let comment = a.text("No data were collected");
            let dataset = a.map(vec![
                ("comment", comment),
                ("has_no_data", no_data),
                ("label", label),
                ("class", class),
                ("structure", structure),
                ("repeating", repeating),
            ]);
            let root = a.root(Some(dataset), vec![]);
            let input = a.finish(root);
            let owner = decode(&input);
            let original_key = owner.dataset().unwrap().has_no_data.origin.key.unwrap();
            assert_eq!(input.nodes()[original_key], N::Text("has_no_data".into()));
            drop(input);
            let dataset = owner.dataset().unwrap();
            assert!(dataset.has_no_data.value);
            assert!(!dataset.reference_data.value);
            assert_eq!(dataset.reference_data.origin.key, None);
            assert_eq!(dataset.reference_data.origin.node, None);
            assert_eq!(
                dataset.fields.iter().map(|v| v.field).collect::<Vec<_>>(),
                [
                    FieldName::Comment,
                    FieldName::HasNoData,
                    FieldName::Label,
                    FieldName::Class,
                    FieldName::Structure,
                    FieldName::Repeating
                ]
            );
            assert_eq!(
                dataset.comment,
                Some(Comment::Plain(Field {
                    value: "No data were collected".into(),
                    origin: dataset.fields[0]
                }))
            );
            assert_eq!(
                owner.fields().iter().map(|v| v.field).collect::<Vec<_>>(),
                [FieldName::Columns, FieldName::Submission]
            );
        }
        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn omitted_defaults_and_explicit_boolean_overrides_are_distinct() {
            for explicit in [false, true] {
                let mut a = Arena::default();
                let label = a.text("DM");
                let class = a.text("SPECIAL PURPOSE");
                let structure = a.text("One record");
                let repeating = a.node(N::Boolean(true));
                let mut fields = vec![
                    ("label", label),
                    ("class", class),
                    ("structure", structure),
                    ("repeating", repeating),
                ];
                if explicit {
                    let no = a.node(N::Boolean(false));
                    let reference = a.node(N::Boolean(true));
                    fields.push(("has_no_data", no));
                    fields.push(("reference_data", reference));
                }
                let dataset = a.map(fields);
                let root = a.root(Some(dataset), vec![]);
                let input = a.finish(root);
                let owner = decode(&input);
                let dataset = owner.dataset().unwrap();
                assert!(!dataset.has_no_data.value);
                assert_eq!(dataset.has_no_data.origin.node.is_some(), explicit);
                assert_eq!(dataset.reference_data.value, explicit);
            }
        }
        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn complete_nested_metadata_retains_integer_text_documents_pages_and_order_after_drop() {
            let mut a = Arena::default();
            let page1 = a.texts(&[
                ("type", "PhysicalRef"),
                ("refs", "2-4"),
                ("title", "Collection pages"),
            ]);
            let doc1 = a.text("acrf");
            let doc1 = a.map(vec![("pages", page1), ("document", doc1)]);
            let page2 = a.texts(&[("type", "NamedDestination"), ("refs", "analysis-start")]);
            let doc2 = a.text("review");
            let doc2 = a.map(vec![("document", doc2), ("pages", page2)]);
            let plain_doc = a.text("protocol");
            let docs = a.node(N::Sequence(vec![doc1, plain_doc, doc2]));
            let kind = a.text("Collected");
            let source = a.text("Subject");
            let description = a.text("Questionnaire");
            let origin = a.map(vec![
                ("documents", docs),
                ("type", kind),
                ("source", source),
                ("description", description),
            ]);
            let expression = a.texts(&[("code", "x + 1"), ("context", "Sponsor language")]);
            let method_description = a.text("Independent calculation");
            let method_name = a.text("Derive value");
            let method = a.map(vec![
                ("expression", expression),
                ("description", method_description),
                ("name", method_name),
            ]);
            let comment_text = a.text("A & B < C");
            let comment = a.map(vec![("text", comment_text)]);
            let length = a.node(N::Integer("18446744073709551616000000".into()));
            let digits = a.node(N::Integer("-18446744073709551616000000".into()));
            let core = a.text("Cond");
            let mandatory = a.node(N::Boolean(true));
            let role = a.text("Record Qualifier");
            let data_type = a.text("text");
            let display = a.text("$20.");
            let codelist = a.text("SEX");
            let inventory = a.node(N::Boolean(false));
            let sub = a.map(vec![
                ("comment", comment),
                ("origin", origin),
                ("method", method),
                ("length", length),
                ("significant_digits", digits),
                ("core", core),
                ("mandatory", mandatory),
                ("role", role),
                ("data_type", data_type),
                ("display_format", display),
                ("codelist", codelist),
                ("inventory_vocabulary", inventory),
            ]);
            let column = a.column("SEX", "str", Some(sub));
            let root = a.root(None, vec![column]);
            let input = a.finish(root);
            let owner = decode(&input);
            drop(input);
            let sub = columns(&owner)[0].submission.as_ref().unwrap();
            assert_eq!(
                sub.length.as_ref().unwrap().value,
                "18446744073709551616000000"
            );
            assert_eq!(
                sub.significant_digits.as_ref().unwrap().value,
                "-18446744073709551616000000"
            );
            assert_eq!(sub.core.as_ref().unwrap().value, Core::Cond);
            assert!(sub.mandatory.as_ref().unwrap().value);
            assert_eq!(sub.role.as_ref().unwrap().value, "Record Qualifier");
            assert_eq!(sub.data_type.as_ref().unwrap().value, DataType::Text);
            assert_eq!(sub.display_format.as_ref().unwrap().value, "$20.");
            assert_eq!(sub.codelist.as_ref().unwrap().value, "SEX");
            assert!(!sub.inventory_vocabulary.as_ref().unwrap().value);
            assert_eq!(sub.fields[0].field, FieldName::Comment);
            assert_eq!(sub.fields[1].field, FieldName::Origin);
            let origin = sub.origins.first().unwrap();
            assert_eq!(origin.kind.value, OriginType::Collected);
            assert_eq!(origin.source.as_ref().unwrap().value, OriginSource::Subject);
            assert_eq!(origin.documents.value.len(), 3);
            let DocumentReference::Class(first) = &origin.documents.value[0] else {
                panic!("first class")
            };
            assert_eq!(first.fields[0].field, FieldName::Pages);
            let pages = first.pages.as_ref().unwrap();
            assert_eq!(pages.kind.value, PageType::PhysicalRef);
            assert_eq!(pages.refs.value, "2-4");
            assert_eq!(pages.title.as_ref().unwrap().value, "Collection pages");
            let DocumentReference::Plain(second) = &origin.documents.value[1] else {
                panic!("second plain")
            };
            assert_eq!(second.value, "protocol");
            let DocumentReference::Class(third) = &origin.documents.value[2] else {
                panic!("third class")
            };
            assert_eq!(
                third.pages.as_ref().unwrap().kind.value,
                PageType::NamedDestination
            );
            let Method::Class(method) = sub.method.as_ref().unwrap() else {
                panic!("method class")
            };
            assert_eq!(method.kind.value, MethodType::Computation);
            assert_eq!(method.kind.origin.node, None);
            assert!(method.documents.value.is_empty());
            assert_eq!(
                method.expression.as_ref().unwrap().context.value,
                "Sponsor language"
            );
            assert_eq!(method.expression.as_ref().unwrap().code.value, "x + 1");
            let Comment::Class(comment) = sub.comment.as_ref().unwrap() else {
                panic!("comment class")
            };
            assert_eq!(comment.text.value, "A & B < C");
        }
        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn plain_empty_method_and_comment_are_schema_strings_not_class_descriptions() {
            let mut a = Arena::default();
            let method = a.text("");
            let comment = a.text("");
            let sub = a.map(vec![("method", method), ("comment", comment)]);
            let column = a.column("A", "str", Some(sub));
            let root = a.root(None, vec![column]);
            let input = a.finish(root);
            let owner = decode(&input);
            let sub = columns(&owner)[0].submission.as_ref().unwrap();
            assert!(matches!(&sub.method, Some(Method::Plain(v)) if v.value.is_empty()));
            assert!(matches!(&sub.comment, Some(Comment::Plain(v)) if v.value.is_empty()));
            let mut a = Arena::default();
            let method = a.texts(&[("description", "")]);
            let comment = a.texts(&[("text", "")]);
            let sub = a.map(vec![("method", method), ("comment", comment)]);
            let column = a.column("A", "str", Some(sub));
            let root = a.root(None, vec![column]);
            let input = a.finish(root);
            assert_eq!(
                findings(&input).iter().map(|v| v.kind).collect::<Vec<_>>(),
                [Kind::EmptyText, Kind::EmptyText]
            );
        }
        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn all_dataset_class_and_subclass_vocabularies_remain_closed_and_family_independent() {
            let classes = [
                "ADAM OTHER",
                "BASIC DATA STRUCTURE",
                "DEVICE LEVEL ANALYSIS DATASET",
                "EVENTS",
                "FINDINGS",
                "FINDINGS ABOUT",
                "INTERVENTIONS",
                "MEDICAL DEVICE BASIC DATA STRUCTURE",
                "MEDICAL DEVICE OCCURRENCE DATA STRUCTURE",
                "OCCURRENCE DATA STRUCTURE",
                "REFERENCE DATA STRUCTURE",
                "RELATIONSHIP",
                "SPECIAL PURPOSE",
                "STUDY REFERENCE",
                "SUBJECT LEVEL ANALYSIS DATASET",
                "TRIAL DESIGN",
            ];
            for class in classes {
                let mut a = Arena::default();
                let dataset = a.dataset(class, None);
                let root = a.root(Some(dataset), vec![]);
                let owner = decode(&a.finish(root));
                assert_eq!(owner.dataset().unwrap().class.value.name(), class);
            }
            for subclass in [
                "ADVERSE EVENT",
                "MEDICAL DEVICE TIME-TO-EVENT",
                "NON-COMPARTMENTAL ANALYSIS",
                "POPULATION PHARMACOKINETIC ANALYSIS",
                "TIME-TO-EVENT",
            ] {
                let mut a = Arena::default();
                let dataset = a.dataset("SPECIAL PURPOSE", Some(subclass));
                let root = a.root(Some(dataset), vec![]);
                let owner = decode(&a.finish(root));
                assert_eq!(
                    owner
                        .dataset()
                        .unwrap()
                        .subclass
                        .as_ref()
                        .unwrap()
                        .value
                        .name(),
                    subclass
                );
            }
        }
        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn all_column_and_define_types_are_declarations_without_semantic_type_resolution() {
            for kind in ["str", "int", "float", "date", "datetime"] {
                let mut a = Arena::default();
                let column = a.column("A", kind, None);
                let root = a.root(None, vec![column]);
                let owner = decode(&a.finish(root));
                assert_eq!(columns(&owner)[0].column_type.value.name(), kind);
            }
            for data_type in [
                "text",
                "integer",
                "float",
                "date",
                "datetime",
                "time",
                "partialDate",
                "partialTime",
                "partialDatetime",
                "incompleteDate",
                "incompleteTime",
                "incompleteDatetime",
                "durationDatetime",
                "intervalDatetime",
                "URI",
            ] {
                let mut a = Arena::default();
                let sub = a.texts(&[("data_type", data_type)]);
                let column = a.column("A", "int", Some(sub));
                let root = a.root(None, vec![column]);
                let owner = decode(&a.finish(root));
                assert_eq!(
                    columns(&owner)[0]
                        .submission
                        .as_ref()
                        .unwrap()
                        .data_type
                        .as_ref()
                        .unwrap()
                        .value
                        .name(),
                    data_type
                );
            }
        }
        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn all_core_origin_source_and_method_choices_are_retained() {
            for core in ["Req", "Exp", "Perm", "Cond"] {
                let mut a = Arena::default();
                let sub = a.texts(&[("core", core)]);
                let column = a.column("A", "str", Some(sub));
                let root = a.root(None, vec![column]);
                let owner = decode(&a.finish(root));
                assert_eq!(
                    columns(&owner)[0]
                        .submission
                        .as_ref()
                        .unwrap()
                        .core
                        .as_ref()
                        .unwrap()
                        .value
                        .name(),
                    core
                );
            }
            for kind in [
                "Assigned",
                "Collected",
                "Derived",
                "Not Available",
                "Other",
                "Predecessor",
                "Protocol",
            ] {
                let mut a = Arena::default();
                let origin = a.texts(&[("type", kind)]);
                let sub = a.map(vec![("origin", origin)]);
                let column = a.column("A", "str", Some(sub));
                let root = a.root(None, vec![column]);
                let owner = decode(&a.finish(root));
                assert_eq!(
                    columns(&owner)[0]
                        .submission
                        .as_ref()
                        .unwrap()
                        .origins
                        .first()
                        .unwrap()
                        .kind
                        .value
                        .name(),
                    kind
                );
            }
            for source in ["Investigator", "Sponsor", "Subject", "Vendor"] {
                let mut a = Arena::default();
                let origin = a.texts(&[("type", "Collected"), ("source", source)]);
                let sub = a.map(vec![("origin", origin)]);
                let column = a.column("A", "str", Some(sub));
                let root = a.root(None, vec![column]);
                let owner = decode(&a.finish(root));
                assert_eq!(
                    columns(&owner)[0]
                        .submission
                        .as_ref()
                        .unwrap()
                        .origins
                        .first()
                        .unwrap()
                        .source
                        .as_ref()
                        .unwrap()
                        .value
                        .name(),
                    source
                );
            }
            for kind in ["Computation", "Imputation"] {
                let mut a = Arena::default();
                let method = a.texts(&[("type", kind), ("description", "Derive")]);
                let sub = a.map(vec![("method", method)]);
                let column = a.column("A", "str", Some(sub));
                let root = a.root(None, vec![column]);
                let owner = decode(&a.finish(root));
                let Method::Class(method) = columns(&owner)[0]
                    .submission
                    .as_ref()
                    .unwrap()
                    .method
                    .as_ref()
                    .unwrap()
                else {
                    panic!("class")
                };
                assert_eq!(method.kind.value.name(), kind);
                assert!(method.kind.origin.node.is_some());
            }
        }
        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn missing_fields_and_explicit_null_defaults_are_complete_distinct_findings() {
            let mut a = Arena::default();
            let no = a.node(N::Null);
            let reference = a.node(N::Null);
            let dataset = a.map(vec![("has_no_data", no), ("reference_data", reference)]);
            let type_null = a.node(N::Null);
            let method = a.map(vec![("type", type_null)]);
            let sub = a.map(vec![("method", method)]);
            let column = a.column("A", "str", Some(sub));
            let root = a.root(Some(dataset), vec![column]);
            let input = a.finish(root);
            let values = findings(&input);
            assert_eq!(values.len(), 8);
            assert_eq!(values.iter().filter(|v| v.kind == Kind::Missing).count(), 5);
            assert_eq!(
                values
                    .iter()
                    .filter(|v| v.kind == Kind::ExpectedBoolean)
                    .count(),
                2
            );
            assert_eq!(
                values
                    .iter()
                    .filter(|v| v.kind == Kind::ExpectedText)
                    .count(),
                1
            );
            assert!(values.iter().all(|v| v.rule().is_none()));
        }
        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn portable_choice_and_identifier_failures_use_only_applicable_current_causes() {
            let mut a = Arena::default();
            let origin = a.texts(&[("type", "invalid"), ("source", "invalid")]);
            let method = a.texts(&[("type", "invalid"), ("description", "Derive")]);
            let core = a.text("invalid");
            let data = a.text("invalid");
            let list = a.text("bad-list");
            let sub = a.map(vec![
                ("core", core),
                ("data_type", data),
                ("codelist", list),
                ("origin", origin),
                ("method", method),
            ]);
            let column = a.column("bad-name", "str", Some(sub));
            let root = a.root(None, vec![column]);
            let input = a.finish(root);
            assert_eq!(
                findings(&input)
                    .iter()
                    .map(|v| v.rule())
                    .collect::<Vec<_>>(),
                [
                    Some("SPEC-2"),
                    Some("SUB-10"),
                    Some("SUB-6"),
                    Some("SPEC-2"),
                    Some("SUB-14"),
                    Some("SUB-14"),
                    Some("SUB-21")
                ]
            );
            let mut a = Arena::default();
            let dataset = a.dataset("wrong", Some("wrong"));
            let root = a.root(Some(dataset), vec![]);
            let input = a.finish(root);
            assert_eq!(
                findings(&input)
                    .iter()
                    .map(|v| v.rule())
                    .collect::<Vec<_>>(),
                [Some("SUB-4"), Some("SUB-4")]
            );
        }
        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn unknown_and_non_text_fields_preserve_exact_occurrences_without_fabricated_causes() {
            let mut a = Arena::default();
            let value = a.text("unknown");
            let unknown = a.text("extension");
            let bool_key = a.node(N::Boolean(true));
            let null = a.node(N::Null);
            let sub = a.node(N::Mapping(vec![(unknown, value), (bool_key, null)]));
            let column = a.column("A", "str", Some(sub));
            let root = a.root(None, vec![column]);
            let input = a.finish(root);
            let values = findings(&input);
            assert_eq!(values.len(), 2);
            assert_eq!(values[0].kind, Kind::UnknownField);
            assert_eq!(values[0].key, Some(unknown));
            assert_eq!(values[0].container, Some(sub));
            assert_eq!(values[0].node, value);
            assert_eq!(values[1].kind, Kind::NonTextKey);
            assert_eq!(values[1].node, bool_key);
            assert_eq!(values[1].key, Some(bool_key));
            assert!(values
                .iter()
                .all(|v| v.rule().is_none() && v.origin.is_none()));
        }
        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn malformed_lists_nested_objects_and_integer_kinds_refuse_without_partial_acceptance() {
            let mut a = Arena::default();
            let scalar = a.text("not a list");
            let origin = a.map(vec![("documents", scalar)]);
            let length = a.node(N::Float(3.0));
            let digits = a.node(N::Boolean(true));
            let expression = a.text("not a mapping");
            let description = a.text("Derive");
            let method = a.map(vec![
                ("description", description),
                ("expression", expression),
            ]);
            let sub = a.map(vec![
                ("origin", origin),
                ("length", length),
                ("significant_digits", digits),
                ("method", method),
            ]);
            let column = a.column("A", "str", Some(sub));
            let root = a.root(None, vec![column]);
            let input = a.finish(root);
            let values = findings(&input);
            assert_eq!(
                values.iter().map(|v| v.kind).collect::<Vec<_>>(),
                [
                    Kind::ExpectedInteger,
                    Kind::ExpectedInteger,
                    Kind::Missing,
                    Kind::ExpectedSequence,
                    Kind::ExpectedMapping
                ]
            );
            assert!(values.iter().all(|v| v.rule().is_none()));
        }
        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn valid_metadata_is_owned_and_invalid_metadata_keeps_complete_original_findings() {
            let mut a = Arena::default();
            let dataset = a.dataset("SPECIAL PURPOSE", None);
            let sub = a.texts(&[("core", "Req")]);
            let column = a.column("A", "str", Some(sub));
            let root = a.root(Some(dataset), vec![column]);
            let input = a.finish(root);
            let owner = decode(&input);
            assert_eq!(owner.dataset().unwrap().origin.node, Some(dataset));
            assert_eq!(columns(&owner)[0].node, column);
            assert_eq!(
                columns(&owner)[0]
                    .submission
                    .as_ref()
                    .unwrap()
                    .core
                    .as_ref()
                    .unwrap()
                    .value,
                Core::Req
            );
            let mut a = Arena::default();
            let core = a.text("wrong");
            let list = a.text("wrong-list");
            let sub = a.map(vec![("core", core), ("codelist", list)]);
            let column = a.column("A", "str", Some(sub));
            let root = a.root(None, vec![column]);
            let input = a.finish(root);
            let error = submission::decode(&input, root).unwrap_err();
            assert!(std::ptr::eq(error.input, &input));
            let Cause::Findings(findings) = error.cause else {
                panic!("complete findings")
            };
            assert_eq!(
                findings
                    .iter()
                    .map(|v| (v.node, v.kind))
                    .collect::<Vec<_>>(),
                [(core, Kind::InvalidChoice), (list, Kind::InvalidIdentifier)]
            );
        }

        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn malformed_deep_unknown_values_keep_the_authored_unknown_field() {
            let mut a = Arena::default();
            let mut value = a.text("deep");
            for _ in 0..10 {
                value = a.node(N::Sequence(vec![value]));
            }
            let sub = a.map(vec![("extension", value)]);
            let column = a.column("A", "str", Some(sub));
            let root = a.root(None, vec![column]);
            let input = a.finish(root);
            let error = submission::decode(&input, root).unwrap_err();
            assert!(std::ptr::eq(error.input, &input));
            let Cause::Findings(findings) = error.cause else {
                panic!("complete findings")
            };
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].kind, Kind::UnknownField);
            assert_eq!(findings[0].node, value);
        }

        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn zero_and_negative_arbitrary_integer_declarations_await_semantic_range_checks() {
            for value in ["0", "-1", "9223372036854775808"] {
                let mut a = Arena::default();
                let integer = a.node(N::Integer(value.into()));
                let sub = a.map(vec![("length", integer)]);
                let column = a.column("A", "str", Some(sub));
                let root = a.root(None, vec![column]);
                let owner = decode(&a.finish(root));
                assert_eq!(
                    columns(&owner)[0]
                        .submission
                        .as_ref()
                        .unwrap()
                        .length
                        .as_ref()
                        .unwrap()
                        .value,
                    value
                );
            }
        }

        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn deepest_page_failure_retains_complete_closed_path_and_original_field_facts() {
            let mut a = Arena::default();
            let refs = a.text("");
            let kind = a.text("wrong");
            let pages = a.map(vec![("type", kind), ("refs", refs)]);
            let document = a.text("bad-document");
            let reference = a.map(vec![("document", document), ("pages", pages)]);
            let docs = a.node(N::Sequence(vec![reference]));
            let origin_type = a.text("Collected");
            let origin = a.map(vec![("type", origin_type), ("documents", docs)]);
            let sub = a.map(vec![("origin", origin)]);
            let column = a.column("A", "str", Some(sub));
            let root = a.root(None, vec![column]);
            let input = a.finish(root);
            let values = findings(&input);
            assert_eq!(values.len(), 3);
            assert_eq!(values[0].rule(), Some("SPEC-2"));
            assert_eq!(values[1].rule(), None);
            assert_eq!(values[2].rule(), None);
            assert_eq!(
                values[2].path.steps().collect::<Vec<_>>(),
                [
                    Step::Field(FieldName::Columns),
                    Step::Index(0),
                    Step::Field(FieldName::Submission),
                    Step::Field(FieldName::Origin),
                    Step::Field(FieldName::Documents),
                    Step::Index(0),
                    Step::Field(FieldName::Pages),
                    Step::Field(FieldName::Refs)
                ]
            );
            assert_eq!(values[2].origin.unwrap().container, pages);
            assert_eq!(values[2].origin.unwrap().node, Some(refs));
            assert_eq!(values[2].node, refs);
        }
        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn later_column_text_and_findings_are_preserved_completely() {
            let mut a = Arena::default();
            let first_sub = a.texts(&[("comment", "first bounded declaration")]);
            let second_sub = a.texts(&[("comment", "second bounded declaration")]);
            let first = a.column("A", "str", Some(first_sub));
            let second = a.column("B", "str", Some(second_sub));
            let root = a.root(None, vec![first, second]);
            let input = a.finish(root);
            let owner = decode(&input);
            for (column, expected) in columns(&owner)
                .iter()
                .zip(["first bounded declaration", "second bounded declaration"])
            {
                let Comment::Plain(comment) = column
                    .submission
                    .as_ref()
                    .unwrap()
                    .comment
                    .as_ref()
                    .unwrap()
                else {
                    panic!("plain comment")
                };
                assert_eq!(comment.value, expected);
            }
            let mut a = Arena::default();
            let one = a.texts(&[("core", "wrong")]);
            let two = a.texts(&[("core", "wrong")]);
            let first = a.column("A", "str", Some(one));
            let second = a.column("B", "str", Some(two));
            let root = a.root(None, vec![first, second]);
            let input = a.finish(root);
            let error = submission::decode(&input, root).unwrap_err();
            assert!(std::ptr::eq(error.input, &input));
            let Cause::Findings(findings) = error.cause else {
                panic!("complete findings")
            };
            assert_eq!(findings.len(), 2);
            assert!(findings
                .iter()
                .all(|finding| finding.kind == Kind::InvalidChoice));
            assert_eq!(findings[0].node, input.field(one, "core").unwrap());
            assert_eq!(findings[1].node, input.field(two, "core").unwrap());
        }

        /// Seam: the values, kinds and paths the raw
        /// `specification_submission_document` decoder returns, which no reader
        /// states.
        #[test]
        fn root_scalar_column_scalar_and_non_text_root_key_keep_transport_facts() {
            let mut a = Arena::default();
            let scalar = a.text("not a column");
            let root = a.root(None, vec![scalar]);
            let input = a.finish(root);
            let values = findings(&input);
            assert_eq!(values.len(), 1);
            assert_eq!(values[0].kind, Kind::ExpectedMapping);
            assert_eq!(values[0].node, scalar);
            let columns_node = input.field(root, "columns").unwrap();
            assert_eq!(values[0].origin.unwrap().container, columns_node);
            assert_eq!(values[0].origin.unwrap().key, None);
            assert_eq!(values[0].origin.unwrap().node, Some(scalar));
            for root in [scalar, usize::MAX] {
                let error = submission::decode(&input, root).unwrap_err();
                assert_eq!(error.cause, Cause::Shape { node: root });
                assert!(std::ptr::eq(error.input, &input));
            }
            let input = Document::new(
                vec![
                    N::Integer("999999999999999999999999999".into()),
                    N::Null,
                    N::Mapping(vec![(0, 1)]),
                ],
                2,
            )
            .unwrap();
            let values = findings(&input);
            assert_eq!(values.len(), 1);
            assert_eq!(values[0].node, 0);
            assert_eq!(values[0].key, Some(0));
            assert_eq!(values[0].kind, Kind::NonTextKey);
            assert_eq!(values[0].rule(), None);
            let empty = Document::new(vec![N::Mapping(vec![])], 0).unwrap();
            let owner = decode(&empty);
            assert!(owner.dataset().is_none());
            assert!(owner.columns().is_none());
        }
    }
    mod submission_composition {
        //! Independent metadata arenas exercise complete multi-domain composition
        //! of 1.0 specifications (D4) of the `SDTM` model (D25) under a 1.0
        //! project's `sdtm` section.
        use yamaa_core::{
            front_end::v1,
            project_terminology::Catalogue,
            schema::{Document, DocumentNode as N},
            submission_composition::{self, Input},
        };

        #[derive(Default)]
        struct Arena(Vec<N>);
        impl Arena {
            fn node(&mut self, node: N) -> usize {
                let index = self.0.len();
                self.0.push(node);
                index
            }
            fn text(&mut self, value: &str) -> usize {
                self.node(N::Text(value.into()))
            }
            fn map(&mut self, fields: Vec<(&str, usize)>) -> usize {
                let entries = fields
                    .into_iter()
                    .map(|(name, value)| (self.text(name), value))
                    .collect();
                self.node(N::Mapping(entries))
            }
            fn texts(&mut self, fields: &[(&str, &str)]) -> usize {
                let entries = fields
                    .iter()
                    .map(|&(name, value)| (name, self.text(value)))
                    .collect();
                self.map(entries)
            }
            fn finish(self, root: usize) -> Document {
                Document::new(self.0, root).unwrap()
            }
        }
        /// The 1.0 header of a file of `kind`.
        fn header(a: &mut Arena, kind: &str) -> Vec<(&'static str, usize)> {
            vec![("yamaa", a.text("1.0")), ("type", a.text(kind))]
        }
        fn project(count: usize) -> Document {
            let mut a = Arena::default();
            let mut fields = header(&mut a, "project");
            let study = a.texts(&[
                ("id", "STUDY01"),
                ("name", "Independent admission study"),
                ("description", "Thirty domains with column and row metadata"),
                ("protocol_name", "PROTOCOL01"),
            ]);
            let standard = a.texts(&[("name", "SDTMIG"), ("version", "3.4"), ("status", "Final")]);
            let standards = a.node(N::Sequence(vec![standard]));
            let entries = (0..count)
                .map(|index| a.text(&format!("d{index}.yaml")))
                .collect();
            let specs = a.node(N::Sequence(entries));
            let define = a.text("define.xml");
            let context = a.text("Other");
            let section = a.map(vec![
                ("standards", standards),
                ("specs", specs),
                ("define", define),
                ("context", context),
            ]);
            fields.extend([("study", study), ("sdtm", section)]);
            let root = a.map(fields);
            a.finish(root)
        }
        fn specification(index: usize, count: usize, row_count: usize) -> Document {
            let mut a = Arena::default();
            let mut fields = header(&mut a, "spec");
            fields.push(("model", a.text("SDTM")));
            let domain = a.text(&format!("D{index}"));
            let label = a.text("Independent dataset");
            let class = a.text("SPECIAL PURPOSE");
            let structure = a.text("One record per subject");
            let repeating = a.node(N::Boolean(false));
            let dataset = a.map(vec![
                ("label", label),
                ("class", class),
                ("structure", structure),
                ("repeating", repeating),
            ]);
            let mut columns = Vec::new();
            let mut names = Vec::new();
            for column in 0..count {
                let name = format!("V{column:04}");
                names.push(a.text(&name));
                let name = a.text(&name);
                let kind = a.text("str");
                let label = a.text("Independent column");
                let derivation = a.text("'X'");
                let origin = a.texts(&[("type", "Assigned"), ("source", "Sponsor")]);
                let origins = a.node(N::Sequence(vec![origin]));
                let core = a.text("Exp");
                let length = a.node(N::Integer("1".into()));
                let submission = a.map(vec![
                    ("core", core),
                    ("length", length),
                    ("origins", origins),
                ]);
                columns.push(a.map(vec![
                    ("name", name),
                    ("type", kind),
                    ("label", label),
                    ("derivation", derivation),
                    ("submission", submission),
                ]));
            }
            let columns = a.node(N::Sequence(columns));
            let names = a.node(N::Sequence(names));
            let path = a.text(&format!("d{index}.csv"));
            let output = a.map(vec![("path", path), ("columns", names)]);
            let mut rows = Vec::new();
            for row in 0..row_count {
                let id = a.text(&format!("r{row}"));
                let entries = (0..count)
                    .map(|column| {
                        let name = a.text(&format!("V{column:04}"));
                        let value = a.text("'X'");
                        (name, value)
                    })
                    .collect();
                let derivations = a.node(N::Mapping(entries));
                rows.push(a.map(vec![("id", id), ("derivations", derivations)]));
            }
            let rows = a.node(N::Sequence(rows));
            fields.extend([
                ("domain", domain),
                ("submission", dataset),
                ("output", output),
                ("columns", columns),
                ("rows", rows),
            ]);
            let root = a.map(fields);
            a.finish(root)
        }
        fn compose(
            documents: &[Document],
        ) -> Result<submission_composition::Study, Vec<submission_composition::Finding>> {
            let project = v1::project::project(&project(documents.len()), None).unwrap();
            let declared = project.submission.unwrap();
            let sources: Vec<_> = (0..documents.len())
                .map(|index| format!("d{index}.yaml"))
                .collect();
            let artifacts: Vec<_> = (0..documents.len())
                .map(|index| format!("d{index}.csv"))
                .collect();
            let trees: Vec<_> = documents
                .iter()
                .map(|document| v1::spec::lower(document, None).into_spec())
                .collect();
            let inputs: Vec<_> = documents
                .iter()
                .enumerate()
                .map(|(index, document)| Input {
                    document,
                    tree: &trees[index],
                    source: &sources[index],
                    artifact: &artifacts[index],
                    href: Some(&artifacts[index]),
                })
                .collect();
            let section = &declared.sections[0];
            let required =
                submission_composition::Required::admit(declared.study.as_ref(), section)?;
            submission_composition::compose(
                required,
                &Catalogue::admit(vec![]).unwrap(),
                &inputs,
                &[],
                "1.0",
                "2026-09-11T00:00:00",
            )
        }
        /// Seam: composition's capacity over generated domains, columns and nodes.
        #[test]
        fn thirty_domains_with_fifteen_hundred_nodes_admit_complete_column_and_row_scans() {
            let documents: Vec<_> = (0..30).map(|index| specification(index, 30, 12)).collect();
            assert!(documents
                .iter()
                .all(|document| (1400..1800).contains(&document.nodes().len())));
            let study = compose(&documents).unwrap();
            assert_eq!(study.datasets().len(), 30);
            for (index, dataset) in study.datasets().iter().enumerate() {
                assert_eq!(dataset.id(), format!("D{index}"));
                assert_eq!(dataset.columns().len(), 30);
                assert_eq!(dataset.columns()[29].declaration().name.value, "V0029");
            }
        }
        /// Seam: composition's capacity over generated domains, columns and nodes.
        #[test]
        fn thirty_domains_with_two_hundred_fifty_columns_keep_complete_ordered_metadata() {
            use yamaa_core::cdisc_vocabulary::{Core, OriginSource, OriginType};
            use yamaa_core::specification_submission_document::ColumnType;
            let documents: Vec<_> = (0..30).map(|index| specification(index, 250, 0)).collect();
            let study = compose(&documents).unwrap();
            assert_eq!(study.datasets().len(), 30);
            assert_eq!(study.installed_version(), "1.0");
            for (index, dataset) in study.datasets().iter().enumerate() {
                assert_eq!(dataset.id(), format!("D{index}"));
                assert_eq!(dataset.columns().len(), 250);
                for (column, compiled) in dataset.columns().iter().enumerate() {
                    let declared = compiled.declaration();
                    assert_eq!(declared.name.value, format!("V{column:04}"));
                    assert_eq!(declared.column_type.value, ColumnType::String);
                    assert_eq!(declared.label.as_ref().unwrap().value, "Independent column");
                    let metadata = declared.submission.as_ref().unwrap();
                    assert_eq!(metadata.core.as_ref().unwrap().value, Core::Exp);
                    assert_eq!(metadata.length.as_ref().unwrap().value, "1");
                    let origin = metadata.origins.first().unwrap();
                    assert_eq!(origin.kind.value, OriginType::Assigned);
                    assert_eq!(origin.source.as_ref().unwrap().value, OriginSource::Sponsor);
                    assert!(compiled.values().is_empty());
                }
            }
        }
    }
    mod submission_document {
        //! The study and sections a project lowers into its root
        //! (`front_end::v1::project::sections`): complete declarations lower to the
        //! tree, and every closed-shape refusal keeps its exact document nodes.
        use yamaa_core::{
            ast,
            cdisc_vocabulary::{DefineContext, GuideStatus},
            front_end::{
                lowering::sections::{self, Cause},
                v1::project::{project as lower_project, sections},
            },
            project_environment::Submission,
            schema::{Document, DocumentNode as N},
        };

        /// Each value is a distinct occurrence; the sections are null.
        fn declarations() -> Document {
            Document::new(
                vec![
                    N::Text("send".into()),
                    N::Null,
                    N::Text("study".into()),
                    N::Null,
                    N::Text("adam".into()),
                    N::Null,
                    N::Text("sdtm".into()),
                    N::Null,
                    N::Text("environments".into()),
                    N::Mapping(vec![]),
                    N::Text("yamaa".into()),
                    N::Text("1.0".into()),
                    N::Text("type".into()),
                    N::Text("project".into()),
                    N::Mapping(vec![
                        (10, 11),
                        (12, 13),
                        (0, 1),
                        (2, 3),
                        (4, 5),
                        (6, 7),
                        (8, 9),
                    ]),
                ],
                14,
            )
            .unwrap()
        }

        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn a_project_whose_declarations_cannot_be_read_still_lowers_and_keeps_the_refusal() {
            let input = declarations();
            // Preparation and runs read the rest of the project; only
            // Define-XML composition requires the study and sections.
            let project = lower_project(&input, None).unwrap();
            assert_eq!(project.submission, Err(ast::SubmissionRefused));
            assert!(project.environments.is_empty() && project.codelists.is_empty());
            let error = sections(&input).unwrap_err();
            assert!(std::ptr::eq(error.input, &input));
            let Cause::Findings(findings) = error.cause else {
                panic!("findings")
            };
            assert_eq!(
                findings
                    .iter()
                    .map(|f| (f.node, f.kind))
                    .collect::<Vec<_>>(),
                [
                    (1, Kind::ExpectedMapping),
                    (3, Kind::ExpectedMapping),
                    (5, Kind::ExpectedMapping),
                    (7, Kind::ExpectedMapping)
                ]
            );
        }

        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn absent_declarations_and_original_shape_refusals_remain_distinct() {
            let empty = Document::new(vec![N::Mapping(vec![])], 0).unwrap();
            let owned = sections(&empty).unwrap();
            assert_eq!(owned.study, None);
            assert!(owned.sections.is_empty());
            let input = Document::new(vec![N::Integer("1".into())], 0).unwrap();
            let error = sections(&input).unwrap_err();
            assert!(std::ptr::eq(error.input, &input));
            assert_eq!(error.cause, Cause::Shape { node: 0 });
        }

        use sections::{FieldName as F, Kind, Step};

        // Independently authored arena fixtures; no decoder or schema normalization
        // helper builds the expected values. Each child is a distinct occurrence.
        #[derive(Default)]
        struct Arena(Vec<N>);
        impl Arena {
            fn node(&mut self, value: N) -> usize {
                let id = self.0.len();
                self.0.push(value);
                id
            }
            fn text(&mut self, value: &str) -> usize {
                self.node(N::Text(value.into()))
            }
            fn map(&mut self, fields: Vec<(&str, usize)>) -> usize {
                let fields = fields
                    .into_iter()
                    .map(|(name, node)| (self.text(name), node))
                    .collect();
                self.node(N::Mapping(fields))
            }
            fn text_map(&mut self, fields: &[(&str, &str)]) -> usize {
                let fields = fields
                    .iter()
                    .map(|&(key, value)| (key, self.text(value)))
                    .collect();
                self.map(fields)
            }
            fn guide(&mut self, name: &str) -> usize {
                self.text_map(&[("name", name), ("version", "3.4"), ("status", "Final")])
            }
            /// A section's `standards`, listing the one guide `guide`.
            fn standards(&mut self, guide: usize) -> usize {
                self.node(N::Sequence(vec![guide]))
            }
            fn section(&mut self, guide: &str) -> usize {
                let guide = self.guide(guide);
                let standards = self.standards(guide);
                let one = self.text("specs/dm.yaml");
                let two = self.text("specs/ae.yaml");
                let specs = self.node(N::Sequence(vec![one, two]));
                let define = self.text("results/define.xml");
                self.map(vec![
                    ("standards", standards),
                    ("specs", specs),
                    ("define", define),
                ])
            }
            /// A project root holding `fields` under the header of a 1.0
            /// project, which lowering the whole project reads.
            fn project(&mut self, mut fields: Vec<(&str, usize)>) -> usize {
                let version = self.text("1.0");
                let kind = self.text("project");
                fields.splice(0..0, [("yamaa", version), ("type", kind)]);
                self.map(fields)
            }
            fn finish(self, root: usize) -> Document {
                Document::new(self.0, root).unwrap()
            }
        }
        fn full() -> Document {
            let mut a = Arena::default();
            let study = a.text_map(&[
                ("id", "STUDY.alpha-1"),
                ("name", "Independent study"),
                ("description", "A & B < C"),
                ("protocol_name", "P-001"),
            ]);
            let send = a.section("SENDIG-DART");
            let sdtm = a.section("SDTMIG-AP");
            let guide = a.guide("ADaMIG-MD");
            let spec = a.text("analysis/adsl.yaml");
            let specs = a.node(N::Sequence(vec![spec]));
            let define = a.text("analysis/define.xml");
            let docs = [
                (
                    "review",
                    "supplemental",
                    "reviewers/review.pdf",
                    "Reviewer guide",
                ),
                ("acrf", "annotated_crf", "forms/acrf.pdf", "Annotated CRF"),
                ("other", "other", "notes/data.txt", "Notes"),
            ]
            .into_iter()
            .map(|(id, kind, link, title)| {
                a.text_map(&[("id", id), ("kind", kind), ("link", link), ("title", title)])
            })
            .collect();
            let docs = a.node(N::Sequence(docs));
            let transport = a.text("dataset_json");
            let context = a.text("Other");
            let xml_lang = a.text("fr-CA");
            let file = a.text("FILE.analysis-1");
            let metadata = a.text_map(&[
                ("id", "MDV.analysis-1"),
                ("name", "Analysis metadata"),
                ("description", "Sponsor metadata"),
            ]);
            let originator = a.text("Independent sponsor");
            let style = a.text("styles/define.xsl");
            let standards = a.standards(guide);
            let adam = a.map(vec![
                ("documents", docs),
                ("originator", originator),
                ("standards", standards),
                ("specs", specs),
                ("define", define),
                ("transport", transport),
                ("context", context),
                ("xml_lang", xml_lang),
                ("file_oid", file),
                ("metadata_version", metadata),
                ("stylesheet", style),
            ]);
            // Other project domains are left to their own admission.
            let lock = a.text("python/uv.lock");
            let python = a.map(vec![("lock", lock)]);
            let environments = a.map(vec![("python", python)]);
            let root = a.project(vec![
                ("send", send),
                ("environments", environments),
                ("study", study),
                ("adam", adam),
                ("sdtm", sdtm),
            ]);
            a.finish(root)
        }
        /// A node's path in a project root, in today's notation.
        fn path(project: &ast::Project, meta: ast::Meta) -> String {
            project.paths.render(meta.path)
        }
        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn complete_declarations_lower_in_authored_order_with_their_paths() {
            let input = full();
            let project = lower_project(&input, None).unwrap();
            drop(input);
            let owned = project.submission.as_ref().unwrap();
            let study = owned.study.as_ref().unwrap();
            assert_eq!(study.id.name, "STUDY.alpha-1");
            assert_eq!(study.name.text, "Independent study");
            assert_eq!(study.description.text, "A & B < C");
            assert_eq!(study.protocol_name.text, "P-001");
            assert_eq!(path(&project, study.meta), "study");
            assert_eq!(path(&project, study.id.meta), "study.id");
            assert_eq!(
                owned.sections.iter().map(|s| s.family).collect::<Vec<_>>(),
                [Submission::Send, Submission::Adam, Submission::Sdtm]
            );
            let send = &owned.sections[0];
            assert_eq!(path(&project, send.meta), "send");
            assert_eq!(send.standards.len(), 1);
            let standard = &send.standards[0];
            assert_eq!(standard.name.text, "SENDIG-DART");
            assert_eq!(standard.version.text, "3.4");
            assert_eq!(standard.status, GuideStatus::Final);
            assert_eq!(path(&project, standard.name.meta), "send.standards[0].name");
            assert_eq!(
                send.specs
                    .iter()
                    .map(|s| s.text.as_str())
                    .collect::<Vec<_>>(),
                ["specs/dm.yaml", "specs/ae.yaml"]
            );
            assert_eq!(path(&project, send.specs[1].meta), "send.specs[1]");
            assert_eq!(send.define.as_ref().unwrap().text, "results/define.xml");
            assert!(send.documents.is_empty());
            // An absent transport writes only the Define-XML document.
            assert!(send.transport.is_none());
            assert_eq!(send.context, DefineContext::Submission);
            assert_eq!(send.xml_lang.text, "en");
            assert_eq!(path(&project, send.xml_lang.meta), "send.xml_lang");
            assert!(
                send.file_oid.is_none()
                    && send.metadata_version.is_none()
                    && send.originator.is_none()
                    && send.stylesheet.is_none()
            );
            let adam = &owned.sections[1];
            assert_eq!(adam.standards[0].name.text, "ADaMIG-MD");
            assert_eq!(adam.transport, Some(ast::Transport::DatasetJson));
            assert_eq!(adam.context, DefineContext::Other);
            assert_eq!(adam.xml_lang.text, "fr-CA");
            assert_eq!(adam.file_oid.as_ref().unwrap().text, "FILE.analysis-1");
            let metadata = adam.metadata_version.as_ref().unwrap();
            assert_eq!(metadata.id.name, "MDV.analysis-1");
            assert_eq!(metadata.name.text, "Analysis metadata");
            assert_eq!(
                metadata.description.as_ref().unwrap().text,
                "Sponsor metadata"
            );
            assert_eq!(
                adam.originator.as_ref().unwrap().text,
                "Independent sponsor"
            );
            assert_eq!(adam.stylesheet.as_ref().unwrap().text, "styles/define.xsl");
            assert_eq!(
                adam.documents
                    .iter()
                    .map(|d| (
                        d.id.name.as_str(),
                        d.kind,
                        d.link.text.as_str(),
                        d.title.text.as_str()
                    ))
                    .collect::<Vec<_>>(),
                [
                    (
                        "review",
                        ast::DocumentKind::Supplemental,
                        "reviewers/review.pdf",
                        "Reviewer guide"
                    ),
                    (
                        "acrf",
                        ast::DocumentKind::AnnotatedCrf,
                        "forms/acrf.pdf",
                        "Annotated CRF"
                    ),
                    ("other", ast::DocumentKind::Other, "notes/data.txt", "Notes"),
                ]
            );
            assert_eq!(
                path(&project, adam.documents[2].link.meta),
                "adam.documents[2].link"
            );
            assert_eq!(owned.sections[2].standards[0].name.text, "SDTMIG-AP");
            // The rest of the project lowers beside the declarations.
            assert_eq!(
                project.environments[0].lock.as_ref().unwrap().text,
                "python/uv.lock"
            );
        }
        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn every_shipped_guide_spelling_is_closed_metadata_not_family_composition() {
            for text in [
                "ADaMIG",
                "ADaMIG-MD",
                "BIMO",
                "CDISC/NCI",
                "SDTMIG",
                "SDTMIG-AP",
                "SDTMIG-MD",
                "SENDIG",
                "SENDIG-AR",
                "SENDIG-DART",
                "SENDIG-GENETOX",
            ] {
                let mut a = Arena::default();
                let section = a.section(text);
                let root = a.map(vec![("sdtm", section)]);
                let input = a.finish(root);
                let owned = sections(&input).unwrap();
                assert_eq!(owned.sections[0].standards[0].name.text, text);
                assert!(owned.study.is_none()); // missing generator prerequisites remain a separate use case.
            }
        }
        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn absent_declarations_and_explicit_default_values_lower_alike() {
            let mut a = Arena::default();
            let lock = a.text("r/renv.lock");
            let r = a.map(vec![("lock", lock)]);
            let environments = a.map(vec![("r", r)]);
            let root = a.map(vec![("environments", environments)]);
            let input = a.finish(root);
            let owned = sections(&input).unwrap();
            assert!(owned.study.is_none());
            assert!(owned.sections.is_empty());
            let mut a = Arena::default();
            let guide = a.guide("SDTMIG");
            let standards = a.standards(guide);
            let specs = a.node(N::Sequence(vec![]));
            let define = a.text("define.xml");
            let documents = a.node(N::Sequence(vec![]));
            let transport = a.text("dataset_json");
            let context = a.text("Submission");
            let lang = a.text("en");
            let section = a.map(vec![
                ("standards", standards),
                ("specs", specs),
                ("define", define),
                ("documents", documents),
                ("transport", transport),
                ("context", context),
                ("xml_lang", lang),
            ]);
            let root = a.map(vec![("sdtm", section)]);
            let input = a.finish(root);
            let owned = sections(&input).unwrap();
            let s = &owned.sections[0];
            assert!(s.documents.is_empty() && s.specs.is_empty());
            assert_eq!(s.transport, Some(ast::Transport::DatasetJson));
            assert_eq!(s.context, DefineContext::Submission);
            assert_eq!(s.xml_lang.text, "en");
            // Each transport spelling lowers to its own variant (JSON-1).
            for (spelling, transport) in [
                ("dataset_json", ast::Transport::DatasetJson),
                ("dataset_ndjson", ast::Transport::DatasetNdjson),
                ("xpt", ast::Transport::Xpt),
            ] {
                let mut a = Arena::default();
                let specs = a.node(N::Sequence(vec![]));
                let value = a.text(spelling);
                let section = a.map(vec![("specs", specs), ("transport", value)]);
                let root = a.map(vec![("sdtm", section)]);
                let input = a.finish(root);
                let owned = sections(&input).unwrap();
                assert_eq!(owned.sections[0].transport, Some(transport), "{spelling}");
            }
        }
        fn invalid() -> Document {
            let mut a = Arena::default();
            let study = a.text_map(&[
                ("id", "9 bad"),
                ("name", ""),
                ("description", "Description"),
                ("protocol_name", "P"),
            ]);
            let guide = a.text_map(&[("name", "UNKNOWN"), ("version", ""), ("status", "final")]);
            let standards = a.standards(guide);
            let spec = a.node(N::Integer("99".into()));
            let specs = a.node(N::Sequence(vec![spec]));
            let define = a.node(N::Null);
            let doc = a.text_map(&[
                ("id", "bad.id"),
                ("kind", "wrong"),
                ("link", "docs/../escape.pdf"),
                ("title", ""),
            ]);
            let documents = a.node(N::Sequence(vec![doc]));
            let transport = a.text("parquet");
            let context = a.text("submission");
            let language = a.text("e-123456789");
            let file = a.text("");
            let metadata = a.text_map(&[("id", "MDV"), ("name", ""), ("description", "")]);
            let originator = a.node(N::Boolean(true));
            let style = a.text("/absolute.xsl");
            let unknown = a.text("1.0");
            let section = a.map(vec![
                ("standards", standards),
                ("specs", specs),
                ("define", define),
                ("documents", documents),
                ("transport", transport),
                ("context", context),
                ("xml_lang", language),
                ("file_oid", file),
                ("metadata_version", metadata),
                ("originator", originator),
                ("stylesheet", style),
                ("generator_version", unknown),
            ]);
            let root = a.map(vec![("study", study), ("adam", section)]);
            a.finish(root)
        }
        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn structural_findings_are_complete_ordered_and_only_supported_causes_are_portable() {
            let input = invalid();
            let error = sections(&input).unwrap_err();
            assert!(std::ptr::eq(error.input, &input));
            let Cause::Findings(findings) = error.cause else {
                panic!("expected complete structural findings")
            };
            assert_eq!(
                findings.iter().map(|f| f.kind).collect::<Vec<_>>(),
                [
                    Kind::InvalidDefineId,
                    Kind::EmptyText,
                    Kind::UnknownField,
                    Kind::InvalidChoice,
                    Kind::EmptyText,
                    Kind::InvalidChoice,
                    Kind::ExpectedText,
                    Kind::ExpectedText,
                    Kind::InvalidIdentifier,
                    Kind::InvalidChoice,
                    Kind::InvalidHref,
                    Kind::EmptyText,
                    Kind::InvalidChoice,
                    Kind::InvalidChoice,
                    Kind::InvalidLanguage,
                    Kind::EmptyText,
                    Kind::EmptyText,
                    Kind::EmptyText,
                    Kind::ExpectedText,
                    Kind::InvalidHref,
                ]
            );
            assert_eq!(
                findings[0].path.steps().collect::<Vec<_>>(),
                [Step::Field(F::Study), Step::Field(F::Id)]
            );
            assert_eq!(
                findings[10].path.steps().collect::<Vec<_>>(),
                [
                    Step::Field(F::Adam),
                    Step::Field(F::Documents),
                    Step::Index(0),
                    Step::Field(F::Link)
                ]
            );
            // `transport` is a closed choice of `dataset_json`, `dataset_ndjson`
            // and `xpt`.
            assert_eq!(
                findings[12].path.steps().collect::<Vec<_>>(),
                [Step::Field(F::Adam), Step::Field(F::Transport)]
            );
            assert_eq!(findings[12].rule(), None);
            assert_eq!(
                findings.iter().filter_map(|f| f.rule()).collect::<Vec<_>>(),
                [
                    "DEFINE-9",
                    "DEFINE-6",
                    "SPEC-2",
                    "DEFINE-13",
                    "DEFINE-21",
                    "DEFINE-13"
                ]
            );
            assert_eq!(findings[7].rule(), None); // null is an original shape, not a fabricated normative message.
            assert!(matches!(error.input.nodes()[findings[7].node], N::Null));
        }
        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn complete_findings_preserve_original_invalid_input() {
            let input = invalid();
            for _ in 0..2 {
                let error = sections(&input).unwrap_err();
                assert!(std::ptr::eq(error.input, &input));
                assert!(error.input.field(error.input.root(), "adam").is_some());
                assert!(matches!(error.cause, Cause::Findings(v) if v.len() == 20));
            }
        }

        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn independent_whole_model_decodes_keep_all_sections() {
            let input = full();
            for _ in 0..2 {
                let owner = sections(&input).unwrap();
                assert_eq!(owner.sections.len(), 3);
                assert_eq!(owner.study.as_ref().unwrap().id.name, "STUDY.alpha-1");
                assert_eq!(owner.sections[1].documents.len(), 3);
            }
        }

        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn null_optional_fields_are_original_shape_findings_not_absent_defaults() {
            let mut a = Arena::default();
            let specs = a.node(N::Sequence(vec![]));
            let mut fields = vec![("specs", specs)];
            for key in [
                "standards",
                "define",
                "documents",
                "transport",
                "context",
                "xml_lang",
                "file_oid",
                "metadata_version",
                "originator",
                "stylesheet",
            ] {
                fields.push((key, a.node(N::Null)));
            }
            let section = a.map(fields);
            let root = a.map(vec![("send", section)]);
            let input = a.finish(root);
            let error = sections(&input).unwrap_err();
            let Cause::Findings(findings) = error.cause else {
                panic!("expected findings")
            };
            assert_eq!(
                findings.iter().map(|f| f.kind).collect::<Vec<_>>(),
                [
                    Kind::ExpectedSequence,
                    Kind::ExpectedText,
                    Kind::ExpectedSequence,
                    Kind::ExpectedText,
                    Kind::ExpectedText,
                    Kind::ExpectedText,
                    Kind::ExpectedText,
                    Kind::ExpectedMapping,
                    Kind::ExpectedText,
                    Kind::ExpectedText
                ]
            );
            assert!(findings
                .iter()
                .all(|f| f.rule().is_none() && matches!(input.nodes()[f.node], N::Null)));
        }
        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn scalar_container_and_nontext_key_refusals_preserve_original_shapes() {
            for scalar in [
                N::Null,
                N::Boolean(true),
                N::Integer("9999999999999999999999999".into()),
                N::Float(1.5),
                N::Text("study".into()),
                N::Sequence(vec![]),
            ] {
                let mut a = Arena::default();
                let node = a.node(scalar);
                let root = a.map(vec![("study", node)]);
                let input = a.finish(root);
                let error = sections(&input).unwrap_err();
                assert!(std::ptr::eq(error.input, &input));
                assert!(
                    matches!(error.cause, Cause::Findings(v) if v.len() == 1 && v[0].node == node && v[0].kind == Kind::ExpectedMapping && v[0].rule().is_none())
                );
            }
            let mut a = Arena::default();
            let key = a.node(N::Boolean(true));
            let value = a.node(N::Null);
            let study = a.node(N::Mapping(vec![(key, value)]));
            let root = a.map(vec![("study", study)]);
            let input = a.finish(root);
            let Cause::Findings(findings) = sections(&input).unwrap_err().cause else {
                panic!("findings")
            };
            assert_eq!(findings.len(), 5);
            assert_eq!(findings[0].node, key);
            assert_eq!(findings[0].kind, Kind::NonTextKey);
            assert!(findings[1..]
                .iter()
                .all(|f| f.kind == Kind::Missing && f.origin.unwrap().node.is_none()));
        }
        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn fixed_document_depth_and_submission_shape_refusals_remain_distinct() {
            let mut nodes = vec![N::Text("original deep scalar".into())];
            let mut child = 0;
            for _ in 0..70 {
                nodes.push(N::Sequence(vec![child]));
                child = nodes.len() - 1;
            }
            let key = nodes.len();
            nodes.push(N::Text("study".into()));
            let root = nodes.len();
            nodes.push(N::Mapping(vec![(key, child)]));
            assert_eq!(
                Document::new(nodes, root).unwrap_err(),
                yamaa_core::schema::DocumentError::Limit {
                    resource: yamaa_core::schema::DocumentResource::Depth,
                    limit: 64
                }
            );
            let mut a = Arena::default();
            let mut child = a.text("original scalar");
            for _ in 0..10 {
                child = a.node(N::Sequence(vec![child]));
            }
            let root = a.map(vec![("study", child)]);
            let input = a.finish(root);
            let error = sections(&input).unwrap_err();
            assert!(
                matches!(error.cause, Cause::Findings(v) if v.len() == 1 && v[0].kind == Kind::ExpectedMapping)
            );
        }

        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn unknown_fields_and_nontext_keys_retain_exact_container_and_key_occurrences() {
            let input = invalid();
            let root = input.root();
            let section = input.field(root, "adam").unwrap();
            let Cause::Findings(findings) = sections(&input).unwrap_err().cause else {
                panic!("findings")
            };
            let unknown = &findings[2];
            assert_eq!(unknown.kind, Kind::UnknownField);
            assert_eq!(unknown.container, Some(section));
            assert!(
                matches!(&input.nodes()[unknown.key.unwrap()], N::Text(text) if text == "generator_version")
            );
            assert_eq!(
                unknown.node,
                input.field(section, "generator_version").unwrap()
            );
            assert_eq!(unknown.rule(), None);
            let mut a = Arena::default();
            let key = a.node(N::Integer("2".into()));
            let value = a.node(N::Null);
            let root = a.node(N::Mapping(vec![(key, value)]));
            let input = a.finish(root);
            let Cause::Findings(findings) = sections(&input).unwrap_err().cause else {
                panic!("findings")
            };
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].kind, Kind::NonTextKey);
            assert_eq!(findings[0].container, Some(root));
            assert_eq!(findings[0].key, Some(key));
            assert_eq!(findings[0].node, key);
        }
        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn present_metadata_description_is_optional_and_draft_status_is_retained() {
            let mut a = Arena::default();
            let guide = a.text_map(&[("name", "ADaMIG"), ("version", "1.3"), ("status", "Draft")]);
            let standards = a.standards(guide);
            let specs = a.node(N::Sequence(vec![]));
            let define = a.text("define.xml");
            let metadata = a.text_map(&[("id", "MDV"), ("name", "Metadata")]);
            let section = a.map(vec![
                ("standards", standards),
                ("specs", specs),
                ("define", define),
                ("metadata_version", metadata),
            ]);
            let root = a.map(vec![("adam", section)]);
            let input = a.finish(root);
            let owned = sections(&input).unwrap();
            let standard = &owned.sections[0].standards[0];
            assert_eq!(standard.status, GuideStatus::Draft);
            assert_eq!(standard.version.text, "1.3");
            assert!(owned.sections[0]
                .metadata_version
                .as_ref()
                .unwrap()
                .description
                .is_none());
        }
        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn default_text_and_empty_root_remain_owned_without_authored_values() {
            let mut a = Arena::default();
            let guide = a.guide("SDTMIG");
            let standards = a.standards(guide);
            let specs = a.node(N::Sequence(vec![]));
            let define = a.text("d.xml");
            let section = a.map(vec![
                ("standards", standards),
                ("specs", specs),
                ("define", define),
            ]);
            let root = a.map(vec![("sdtm", section)]);
            let input = a.finish(root);
            let owned = sections(&input).unwrap();
            assert_eq!(owned.sections[0].xml_lang.text, "en");
            let empty = Document::new(vec![N::Mapping(vec![])], 0).unwrap();
            let owned = sections(&empty).unwrap();
            assert!(owned.sections.is_empty());
            assert!(owned.study.is_none());
        }
        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn a_section_listing_only_specs_admits_and_leaves_define_declarations_to_composition() {
            use yamaa_core::submission_composition::Required;
            for (name, family) in [
                ("sdtm", Submission::Sdtm),
                ("adam", Submission::Adam),
                ("send", Submission::Send),
            ] {
                let mut a = Arena::default();
                let spec = a.text("specs/dm.yaml");
                let specs = a.node(N::Sequence(vec![spec]));
                let section = a.map(vec![("specs", specs)]);
                let root = a.map(vec![(name, section)]);
                let input = a.finish(root);
                let owned = sections(&input).unwrap();
                drop(input);
                assert!(owned.study.is_none());
                let s = &owned.sections[0];
                assert_eq!(s.family, family);
                assert_eq!(
                    s.specs.iter().map(|v| v.text.as_str()).collect::<Vec<_>>(),
                    ["specs/dm.yaml"]
                );
                assert!(s.standards.is_empty() && s.define.is_none() && s.transport.is_none());
                assert!(s.documents.is_empty());
                assert_eq!(s.context, DefineContext::Submission);
                assert_eq!(s.xml_lang.text, "en");
                // Only Define-XML composition requires the study, guide and target.
                let Err(findings) = Required::admit(owned.study.as_ref(), s) else {
                    panic!("composition requires the define declarations")
                };
                assert_eq!(
                    findings
                        .iter()
                        .map(|finding| finding.path.as_str())
                        .collect::<Vec<_>>(),
                    [
                        "study".to_string(),
                        format!("{name}.standards"),
                        format!("{name}.define")
                    ]
                );
            }
        }

        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn control_and_unicode_text_survive_original_document_ownership() {
            let mut a = Arena::default();
            let study = a.text_map(&[
                ("id", "STUDY"),
                ("name", "A\nB\t'\\"),
                ("description", "é < 漢字 & 😀"),
                ("protocol_name", "P"),
            ]);
            let root = a.map(vec![("study", study)]);
            let input = a.finish(root);
            let owned = sections(&input).unwrap();
            drop(input);
            assert_eq!(owned.study.as_ref().unwrap().name.text, "A\nB\t'\\");
            assert_eq!(
                owned.study.as_ref().unwrap().description.text,
                "é < 漢字 & 😀"
            );
        }

        struct DocumentItemFact {
            family: F,
            sequence: usize,
            node: usize,
            index: usize,
        }

        // Independent normalized occurrences: valid mappings surround two different
        // malformed shapes in every family. Expected facts come from this arena alone.
        fn malformed_document_lists() -> (Document, Vec<DocumentItemFact>) {
            let mut a = Arena::default();
            let mut sections = Vec::new();
            let mut facts = Vec::new();
            for (name, family, guide) in [
                ("send", F::Send, "SENDIG"),
                ("sdtm", F::Sdtm, "SDTMIG"),
                ("adam", F::Adam, "ADaMIG"),
            ] {
                let guide = a.guide(guide);
                let standards = a.standards(guide);
                let spec = a.text("specifications/domain.yaml");
                let specs = a.node(N::Sequence(vec![spec]));
                let define = a.text("results/define.xml");
                let first = a.text_map(&[
                    ("id", "first"),
                    ("kind", "supplemental"),
                    ("link", "docs/first.pdf"),
                    ("title", "First document"),
                ]);
                let null = a.node(N::Null);
                let scalar = a.node(N::Integer("42".into()));
                let last = a.text_map(&[
                    ("id", "last"),
                    ("kind", "other"),
                    ("link", "docs/last.pdf"),
                    ("title", "Last document"),
                ]);
                let sequence = a.node(N::Sequence(vec![first, null, scalar, last]));
                let section = a.map(vec![
                    ("documents", sequence),
                    ("standards", standards),
                    ("specs", specs),
                    ("define", define),
                ]);
                sections.push((name, section));
                for (index, node) in [(1, null), (2, scalar)] {
                    facts.push(DocumentItemFact {
                        family,
                        sequence,
                        node,
                        index,
                    });
                }
            }
            let root = a.map(sections);
            (a.finish(root), facts)
        }

        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn malformed_document_items_retain_actual_occurrences_in_every_family_and_position() {
            let (input, expected) = malformed_document_lists();
            let error = sections(&input).unwrap_err();
            assert!(std::ptr::eq(error.input, &input));
            let Cause::Findings(findings) = error.cause else {
                panic!("complete shape findings")
            };
            assert_eq!(findings.len(), 6);
            for (finding, fact) in findings.iter().zip(&expected) {
                assert_eq!(finding.kind, Kind::ExpectedMapping);
                assert_eq!(finding.node, fact.node);
                assert_eq!(finding.container, Some(fact.sequence));
                assert_eq!(finding.key, None);
                assert_eq!(finding.rule(), None);
                assert_eq!(
                    finding.origin,
                    Some(sections::Origin {
                        container: fact.sequence,
                        key: None,
                        node: Some(fact.node),
                        field: F::Documents,
                    })
                );
                assert_eq!(
                    finding.path.steps().collect::<Vec<_>>(),
                    [
                        Step::Field(fact.family),
                        Step::Field(F::Documents),
                        Step::Index(fact.index)
                    ]
                );
                let N::Sequence(items) = &error.input.nodes()[fact.sequence] else {
                    panic!("original document sequence")
                };
                assert_eq!(items[fact.index], fact.node);
                let section = input.field(input.root(), fact.family.name()).unwrap();
                assert_eq!(input.field(section, "documents"), Some(fact.sequence));
                assert_ne!(section, fact.sequence);
            }
            assert!(matches!(input.nodes()[expected[0].node], N::Null));
            assert_eq!(input.nodes()[expected[1].node], N::Integer("42".into()));
        }

        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn all_malformed_document_items_keep_the_original_input_and_nodes() {
            let (input, expected) = malformed_document_lists();
            let error = sections(&input).unwrap_err();
            assert!(std::ptr::eq(error.input, &input));
            let Cause::Findings(findings) = error.cause else {
                panic!("complete findings")
            };
            assert_eq!(findings.len(), 6);
            assert_eq!(
                findings.iter().map(|v| v.node).collect::<Vec<_>>(),
                expected.iter().map(|v| v.node).collect::<Vec<_>>()
            );
        }

        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn valid_document_fields_lower_in_order_with_their_paths() {
            let input = full();
            let project = lower_project(&input, None).unwrap();
            drop(input);
            let owned = project.submission.as_ref().unwrap();
            let docs = &owned.sections[1].documents;
            assert_eq!(docs.len(), 3);
            for (index, document) in docs.iter().enumerate() {
                for (meta, field) in [
                    (document.id.meta, "id"),
                    (document.link.meta, "link"),
                    (document.title.meta, "title"),
                ] {
                    assert_eq!(
                        project.paths.render(meta.path),
                        format!("adam.documents[{index}].{field}")
                    );
                }
            }
            assert_eq!(docs[0].title.text, "Reviewer guide");
            assert_eq!(docs[1].kind, ast::DocumentKind::AnnotatedCrf);
            assert_eq!(docs[2].link.text, "notes/data.txt");
            assert!(owned.sections[0].documents.is_empty());
        }

        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn an_authored_source_system_version_lowers_for_every_section() {
            for (family, guide) in [("sdtm", "SDTMIG"), ("adam", "ADaMIG"), ("send", "SENDIG")] {
                let mut a = Arena::default();
                a.section(guide);
                let N::Mapping(mut fields) = a.0.pop().unwrap() else {
                    unreachable!()
                };
                let key = a.text("source_system_version");
                let version = a.text("1.0");
                fields.push((key, version));
                let section = a.node(N::Mapping(fields));
                let root = a.project(vec![(family, section)]);
                let input = a.finish(root);
                let project = lower_project(&input, None).unwrap();
                drop(input);
                let section = &project.submission.as_ref().unwrap().sections[0];
                let field = section.source_system_version.as_ref().unwrap();
                assert_eq!(field.text, "1.0");
                assert_eq!(
                    project.paths.render(field.meta.path),
                    format!("{family}.source_system_version")
                );
            }
            // Omitted, the installed generator's version is used at composition.
            let input = full();
            let owned = sections(&input).unwrap();
            assert!(owned
                .sections
                .iter()
                .all(|section| section.source_system_version.is_none()));
        }

        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn submission_version_refusals_preserve_exact_nodes_and_do_not_choose_defaults() {
            for (value, expected) in [
                (N::Null, Kind::ExpectedText),
                (N::Boolean(true), Kind::ExpectedText),
                (N::Integer("1".into()), Kind::ExpectedText),
                (N::Text("".into()), Kind::EmptyText),
            ] {
                let mut a = Arena::default();
                a.section("ADaMIG");
                let N::Mapping(mut fields) = a.0.pop().unwrap() else {
                    unreachable!()
                };
                let key = a.text("source_system_version");
                let version = a.node(value);
                fields.push((key, version));
                let section = a.node(N::Mapping(fields));
                let root = a.map(vec![("adam", section)]);
                let input = a.finish(root);
                let error = sections(&input).unwrap_err();
                assert!(std::ptr::eq(error.input, &input));
                let Cause::Findings(findings) = error.cause else {
                    panic!("findings")
                };
                assert_eq!(findings.len(), 1);
                assert_eq!(findings[0].kind, expected);
                assert_eq!(findings[0].node, version);
                assert_eq!(findings[0].key, Some(key));
                assert_eq!(
                    findings[0].path.steps().collect::<Vec<_>>(),
                    [Step::Field(F::Adam), Step::Field(F::SourceSystemVersion)]
                );
                assert_eq!(findings[0].rule(), None);
            }
        }

        /// Seam: the declarations, kinds and paths the raw `submission_document`
        /// decoder returns, which no reader states.
        #[test]
        fn full_submission_version_text_survives_the_input() {
            let mut a = Arena::default();
            a.section("SENDIG");
            let N::Mapping(mut fields) = a.0.pop().unwrap() else {
                unreachable!()
            };
            let key = a.text("source_system_version");
            let version = a.text(&"v".repeat(4096));
            fields.push((key, version));
            let section = a.node(N::Mapping(fields));
            let root = a.map(vec![("send", section)]);
            let input = a.finish(root);
            assert_eq!(input.field(section, "source_system_version"), Some(version));
            let owned = sections(&input).unwrap();
            drop(input);
            let selected = owned.sections[0].source_system_version.as_ref().unwrap();
            assert_eq!(selected.text, "v".repeat(4096));
        }
    }
    mod submission_values {
        //! Independently authored normalized metadata; no generator supplies expectations.
        //! The Define-XML these entries render is checked in yamaa-adapters'
        //! `tests/submission_encoders.rs`. The specification is a 1.0 file (D4)
        //! of the `SDTM` model (D25) that a 1.0 project's `sdtm` section lists.
        use yamaa_core::{
            cdisc_vocabulary,
            front_end::v1,
            project_terminology::Catalogue,
            schema::{Document, DocumentNode as N},
            specification_submission_document as declarations,
            submission_composition::{self, Input, Study},
        };

        #[derive(Default)]
        struct Arena(Vec<N>);
        impl Arena {
            fn node(&mut self, node: N) -> usize {
                let n = self.0.len();
                self.0.push(node);
                n
            }
            fn text(&mut self, text: &str) -> usize {
                self.node(N::Text(text.into()))
            }
            fn map(&mut self, fields: Vec<(&str, usize)>) -> usize {
                let fields = fields
                    .into_iter()
                    .map(|(key, value)| (self.text(key), value))
                    .collect();
                self.node(N::Mapping(fields))
            }
            fn texts(&mut self, fields: &[(&str, &str)]) -> usize {
                let fields = fields
                    .iter()
                    .map(|&(key, value)| (key, self.text(value)))
                    .collect();
                self.map(fields)
            }
            fn strings(&mut self, values: &[&str]) -> usize {
                let values = values.iter().map(|value| self.text(value)).collect();
                self.node(N::Sequence(values))
            }
            fn finish(self, root: usize) -> Document {
                Document::new(self.0, root).unwrap()
            }
        }
        fn project() -> Document {
            let mut a = Arena::default();
            let yamaa = a.text("1.0");
            let kind = a.text("project");
            let study = a.texts(&[
                ("id", "VLSTUDY"),
                ("name", "Value metadata study"),
                ("description", "Independent structure witness"),
                ("protocol_name", "VLPROTOCOL"),
            ]);
            let standard = a.texts(&[("name", "SDTMIG"), ("version", "3.4"), ("status", "Final")]);
            let standards = a.node(N::Sequence(vec![standard]));
            let filename = a.text("spec.yaml");
            let specs = a.node(N::Sequence(vec![filename]));
            let define = a.text("define.xml");
            let version = a.text("1.0");
            let context = a.text("Other");
            let section = a.map(vec![
                ("standards", standards),
                ("specs", specs),
                ("define", define),
                ("source_system_version", version),
                ("context", context),
            ]);
            let root = a.map(vec![
                ("yamaa", yamaa),
                ("type", kind),
                ("study", study),
                ("sdtm", section),
            ]);
            a.finish(root)
        }
        /// The `LB` specification whose `LBORRESU` declares two value-level
        /// entries, each its id, the test code its where clause selects, and
        /// its origin's source.
        fn specification() -> Document {
            let mut a = Arena::default();
            let yamaa = a.text("1.0");
            let kind = a.text("spec");
            let model = a.text("SDTM");
            let domain = a.text("LB");
            let label = a.text("Laboratory tests");
            let class = a.text("FINDINGS");
            let structure = a.text("One record per test");
            let repeating = a.node(N::Boolean(true));
            let dataset = a.map(vec![
                ("label", label),
                ("class", class),
                ("structure", structure),
                ("repeating", repeating),
            ]);
            let mut columns = Vec::new();
            for (name, kind) in [
                ("LBTESTCD", "str"),
                ("LBSPEC", "str"),
                ("LBNAM", "str"),
                ("LBSEQ", "int"),
                ("LBORRESU", "str"),
            ] {
                let name_node = a.text(name);
                let kind = a.text(kind);
                let label = a.text(match name {
                    "LBORRESU" => "Original units",
                    "LBSPEC" => "Specimen type",
                    "LBNAM" => "Laboratory name",
                    "LBSEQ" => "Sequence number",
                    _ => "Test code",
                });
                let mut fields = vec![("name", name_node), ("type", kind), ("label", label)];
                let literal = match name {
                    "LBORRESU" => Some(a.text("'mg/dL'")),
                    "LBSPEC" => Some(a.text("'SERUM'")),
                    "LBNAM" => Some(a.text("'CENTRAL'")),
                    "LBSEQ" => Some(a.node(N::Integer("1".into()))),
                    _ => None,
                };
                if let Some(literal) = literal {
                    let derivation = a.map(vec![("value", literal)]);
                    fields.push(("derivation", derivation));
                }
                // The internal column carries no submission metadata.
                if name == "LBNAM" {
                    columns.push(a.map(fields));
                    continue;
                }
                let origin = a.texts(&[("type", "Assigned"), ("source", "Sponsor")]);
                let origins = a.node(N::Sequence(vec![origin]));
                let core = a.text("Exp");
                let length = a.node(N::Integer("8".into()));
                let mut metadata = vec![("core", core), ("length", length), ("origins", origins)];
                if name == "LBORRESU" {
                    let mut entries = Vec::new();
                    for (code, source) in [("GLUC", "Vendor"), ("CREAT", "Sponsor")] {
                        let id = a.text(code);
                        let terms = a.strings(&[code]);
                        let clause = a.map(vec![("LBTESTCD", terms)]);
                        let origin = a.texts(&[("type", "Assigned"), ("source", source)]);
                        let origins = a.node(N::Sequence(vec![origin]));
                        let method = a.text("Units assigned from the supplied test metadata");
                        let fields = vec![
                            ("id", id),
                            ("where", clause),
                            ("origins", origins),
                            ("method", method),
                        ];
                        entries.push(a.map(fields));
                    }
                    let entries = a.node(N::Sequence(entries));
                    metadata.push(("value_level", entries));
                }
                let submission = a.map(metadata);
                fields.push(("submission", submission));
                columns.push(a.map(fields));
            }
            let columns = a.node(N::Sequence(columns));
            let names = a.strings(&["LBTESTCD", "LBSPEC", "LBSEQ", "LBORRESU"]);
            let output_path = a.text("lb.csv");
            let output = a.map(vec![("path", output_path), ("columns", names)]);
            let mut rows = Vec::new();
            for (id, code) in [
                ("glucose", "GLUC"),
                ("creatinine", "CREAT"),
                ("albumin", "ALB"),
            ] {
                let id = a.text(id);
                let literal = a.text(&format!("'{code}'"));
                let literal = a.map(vec![("value", literal)]);
                let derivations = a.map(vec![("LBTESTCD", literal)]);
                rows.push(a.map(vec![("id", id), ("derivations", derivations)]));
            }
            let rows = a.node(N::Sequence(rows));
            let root = a.map(vec![
                ("yamaa", yamaa),
                ("type", kind),
                ("model", model),
                ("domain", domain),
                ("submission", dataset),
                ("columns", columns),
                ("output", output),
                ("rows", rows),
            ]);
            a.finish(root)
        }
        fn compose(document: &Document) -> Result<Study, Vec<submission_composition::Finding>> {
            let project = v1::project::project(&project(), None).unwrap();
            let declared = project.submission.unwrap();
            let section = &declared.sections[0];
            let required =
                submission_composition::Required::admit(declared.study.as_ref(), section)?;
            let tree = v1::spec::lower(document, None).into_spec();
            submission_composition::compose(
                required,
                &Catalogue::admit(vec![]).unwrap(),
                &[Input {
                    document,
                    tree: &tree,
                    source: "spec.yaml",
                    artifact: "lb.csv",
                    href: Some("lb.csv"),
                }],
                &[],
                "0.3.0",
                "2026-09-11T00:00:00",
            )
        }
        /// Seam: the decoded entries and the composed `Study`'s merged value metadata,
        /// values no reader returns.
        #[test]
        fn entries_inherit_complete_metadata_and_preserve_written_occurrences() {
            let document = specification();
            let declared = declarations::decode(&document, document.root()).unwrap();
            let columns = &declared.columns().unwrap().value;
            let units = columns.iter().find(|c| c.name.value == "LBORRESU").unwrap();
            let metadata = units.submission.as_ref().unwrap();
            assert_eq!(metadata.value_level.len(), 2);
            let entry = &metadata.value_level[0];
            assert_eq!(entry.id.value, "GLUC");
            assert_eq!(entry.clause.len(), 1);
            assert_eq!(entry.clause[0].column.value, "LBTESTCD");
            assert_eq!(entry.clause[0].values[0].value, "GLUC");
            assert!(entry.metadata.core.is_none());
            assert!(entry.metadata.value_level.is_empty());
            assert_eq!(
                document.field(entry.origin.node.unwrap(), "id"),
                entry.id.origin.node
            );
            let study = compose(&document).unwrap();
            let column = &study.datasets()[0].columns()[3];
            assert_eq!(column.values().len(), 2);
            let value = &column.values()[0];
            assert_eq!(value.id(), "GLUC");
            let merged = value.column().declaration().submission.as_ref().unwrap();
            assert_eq!(
                merged.core.as_ref().unwrap().value,
                cdisc_vocabulary::Core::Exp
            );
            assert_eq!(merged.length.as_ref().unwrap().value, "8");
            assert!(merged.value_level.is_empty());
            assert_eq!(
                merged
                    .origins
                    .first()
                    .unwrap()
                    .source
                    .as_ref()
                    .unwrap()
                    .value,
                cdisc_vocabulary::OriginSource::Vendor
            );
        }
        /// Seam: the raw decoder's refusal kind and path steps for an entry id.
        #[test]
        fn an_entry_id_outside_the_identifier_pattern_is_a_refusal_with_its_entry_path() {
            let mut a = Arena::default();
            let id = a.text("glucose / primary test: 1");
            let terms = a.strings(&["GLUC"]);
            let clause = a.map(vec![("LBTESTCD", terms)]);
            let entry = a.map(vec![("id", id), ("where", clause)]);
            let entries = a.node(N::Sequence(vec![entry]));
            let submission = a.map(vec![("value_level", entries)]);
            let name = a.text("LBORRESU");
            let kind = a.text("str");
            let column = a.map(vec![
                ("name", name),
                ("type", kind),
                ("submission", submission),
            ]);
            let columns = a.node(N::Sequence(vec![column]));
            let root = a.map(vec![("columns", columns)]);
            let document = a.finish(root);
            let error = declarations::decode(&document, document.root()).unwrap_err();
            let declarations::Cause::Findings(findings) = error.cause else {
                panic!("expected a structural identifier refusal");
            };
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].kind, declarations::Kind::InvalidIdentifier);
            use declarations::{FieldName as F, Step};
            assert_eq!(
                findings[0].path.steps().collect::<Vec<_>>(),
                [
                    Step::Field(F::Columns),
                    Step::Index(0),
                    Step::Field(F::Submission),
                    Step::Field(F::ValueLevel),
                    Step::Index(0),
                    Step::Field(F::Id),
                ]
            );
        }
    }
}
