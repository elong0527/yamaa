//! Schema contracts.
#[allow(dead_code)]
mod support;
mod schema {
    mod specification_submission_document {
        use yamaa_core::{
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
            let role = a.text("Qualifier");
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
            assert_eq!(sub.core.as_ref().unwrap().value, Core::Conditional);
            assert!(sub.mandatory.as_ref().unwrap().value);
            assert_eq!(sub.role.as_ref().unwrap().value, "Qualifier");
            assert_eq!(sub.data_type.as_ref().unwrap().value, DataType::Text);
            assert_eq!(sub.display_format.as_ref().unwrap().value, "$20.");
            assert_eq!(sub.codelist.as_ref().unwrap().value, "SEX");
            assert!(!sub.inventory_vocabulary.as_ref().unwrap().value);
            assert_eq!(sub.fields[0].field, FieldName::Comment);
            assert_eq!(sub.fields[1].field, FieldName::Origin);
            let origin = sub.origin_metadata.as_ref().unwrap();
            assert_eq!(origin.kind.value, OriginType::Collected);
            assert_eq!(origin.source.as_ref().unwrap().value, OriginSource::Subject);
            assert_eq!(origin.documents.value.len(), 3);
            let DocumentReference::Class(first) = &origin.documents.value[0] else {
                panic!("first class")
            };
            assert_eq!(first.fields[0].field, FieldName::Pages);
            let pages = first.pages.as_ref().unwrap();
            assert_eq!(pages.kind.value, PageType::Physical);
            assert_eq!(pages.refs.value, "2-4");
            assert_eq!(pages.title.as_ref().unwrap().value, "Collection pages");
            let DocumentReference::Plain(second) = &origin.documents.value[1] else {
                panic!("second plain")
            };
            assert_eq!(second.value, "protocol");
            let DocumentReference::Class(third) = &origin.documents.value[2] else {
                panic!("third class")
            };
            assert_eq!(third.pages.as_ref().unwrap().kind.value, PageType::Named);
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
                        .origin_metadata
                        .as_ref()
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
                        .origin_metadata
                        .as_ref()
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
            assert!(values.iter().all(|v| v.requirement().is_none()));
        }
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
                    .map(|v| v.requirement())
                    .collect::<Vec<_>>(),
                [
                    Some("REQ-1046"),
                    Some("REQ-1141"),
                    Some("REQ-1145"),
                    Some("REQ-1046"),
                    Some("REQ-1142"),
                    Some("REQ-1143"),
                    Some("REQ-1144")
                ]
            );
            let mut a = Arena::default();
            let dataset = a.dataset("wrong", Some("wrong"));
            let root = a.root(Some(dataset), vec![]);
            let input = a.finish(root);
            assert_eq!(
                findings(&input)
                    .iter()
                    .map(|v| v.requirement())
                    .collect::<Vec<_>>(),
                [Some("REQ-1146"), Some("REQ-1147")]
            );
        }
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
                .all(|v| v.requirement().is_none() && v.origin.is_none()));
        }
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
            assert!(values.iter().all(|v| v.requirement().is_none()));
        }
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
                Core::Required
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
            assert_eq!(values[0].requirement(), Some("REQ-1046"));
            assert_eq!(values[1].requirement(), None);
            assert_eq!(values[2].requirement(), None);
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
            assert_eq!(values[0].requirement(), None);
            let empty = Document::new(vec![N::Mapping(vec![])], 0).unwrap();
            let owner = decode(&empty);
            assert!(owner.dataset().is_none());
            assert!(owner.columns().is_none());
        }
    }
    mod submission_composition {
        //! Independent metadata arenas exercise complete multi-domain composition.
        use yamaa_core::{
            project_terminology::Catalogue,
            schema::{Document, DocumentNode as N},
            submission_composition::{self, Input},
            submission_document,
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
        fn environment(count: usize) -> Document {
            let mut a = Arena::default();
            let study = a.texts(&[
                ("id", "STUDY01"),
                ("name", "Independent admission study"),
                ("description", "Thirty domains with column and row metadata"),
                ("protocol_name", "PROTOCOL01"),
            ]);
            let standard = a.texts(&[("name", "SDTMIG"), ("version", "3.4"), ("status", "Final")]);
            let entries = (0..count)
                .map(|index| a.text(&format!("d{index}.yaml")))
                .collect();
            let specs = a.node(N::Sequence(entries));
            let define = a.text("define.xml");
            let section = a.map(vec![
                ("standard", standard),
                ("specs", specs),
                ("define", define),
            ]);
            let root = a.map(vec![("study", study), ("sdtm", section)]);
            a.finish(root)
        }
        fn specification(index: usize, count: usize, row_count: usize) -> Document {
            let mut a = Arena::default();
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
                let derivation = a.texts(&[("literal", "X")]);
                let origin = a.texts(&[("type", "Assigned"), ("source", "Sponsor")]);
                let core = a.text("Exp");
                let length = a.node(N::Integer("1".into()));
                let submission =
                    a.map(vec![("core", core), ("length", length), ("origin", origin)]);
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
            for _ in 0..row_count {
                let entries = (0..count)
                    .map(|column| {
                        let name = a.text(&format!("V{column:04}"));
                        let value = a.texts(&[("literal", "X")]);
                        (name, value)
                    })
                    .collect();
                let derivations = a.node(N::Mapping(entries));
                rows.push(a.map(vec![("derivations", derivations)]));
            }
            let rows = a.node(N::Sequence(rows));
            let root = a.map(vec![
                ("domain", domain),
                ("submission", dataset),
                ("output", output),
                ("columns", columns),
                ("rows", rows),
            ]);
            a.finish(root)
        }
        fn compose(
            documents: &[Document],
        ) -> Result<submission_composition::Study, Vec<submission_composition::Finding>> {
            let environment = environment(documents.len());
            let declared = submission_document::decode(&environment, environment.root()).unwrap();
            let sources: Vec<_> = (0..documents.len())
                .map(|index| format!("d{index}.yaml"))
                .collect();
            let artifacts: Vec<_> = (0..documents.len())
                .map(|index| format!("d{index}.csv"))
                .collect();
            let inputs: Vec<_> = documents
                .iter()
                .enumerate()
                .map(|(index, document)| Input {
                    document,
                    source: &sources[index],
                    artifact: &artifacts[index],
                    href: &artifacts[index],
                })
                .collect();
            submission_composition::compose(
                declared.study().unwrap(),
                &declared.sections()[0],
                &Catalogue::admit(vec![]).unwrap(),
                &inputs,
                &[],
                "1.0",
                "2026-09-11T00:00:00",
            )
        }
        #[test]
        fn thirty_domains_with_fifteen_hundred_nodes_admit_complete_column_and_row_scans() {
            let documents: Vec<_> = (0..30).map(|index| specification(index, 30, 6)).collect();
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
        #[test]
        fn thirty_domains_with_two_hundred_fifty_columns_keep_complete_ordered_metadata() {
            use yamaa_core::specification_submission_document::{
                ColumnType, Core, OriginSource, OriginType,
            };
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
                    assert_eq!(metadata.core.as_ref().unwrap().value, Core::Expected);
                    assert_eq!(metadata.length.as_ref().unwrap().value, "1");
                    let origin = metadata.origin_metadata.as_ref().unwrap();
                    assert_eq!(origin.kind.value, OriginType::Assigned);
                    assert_eq!(origin.source.as_ref().unwrap().value, OriginSource::Sponsor);
                    assert!(compiled.values().is_empty());
                }
            }
        }
    }
    mod submission_document {
        use yamaa_core::{
            project_environment::Submission,
            schema::{Document, DocumentNode as N},
            submission_document::{self, Cause},
        };

        fn declarations() -> Document {
            // Each value is a distinct occurrence. Null values deliberately establish
            // locations only; no submission validation/acceptance is inferred here.
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
                    N::Text("language".into()),
                    N::Text("r".into()),
                    N::Mapping(vec![(0, 1), (2, 3), (4, 5), (6, 7), (8, 9)]),
                ],
                10,
            )
            .unwrap()
        }

        #[test]
        fn borrowed_declarations_keep_original_occurrences_and_authored_section_order() {
            let input = declarations();
            let facts = submission_document::locate(&input, 10).unwrap();
            assert!(std::ptr::eq(facts.input(), &input));
            assert_eq!(facts.study(), Some(3));
            assert_eq!(
                facts.sections(),
                [
                    (Submission::Send, 1),
                    (Submission::Adam, 5),
                    (Submission::Sdtm, 7)
                ]
            );
            assert!(matches!(facts.input().nodes()[3], N::Null));
        }

        #[test]
        fn repeated_location_scans_preserve_exact_input_and_complete_facts() {
            let input = declarations();
            for _ in 0..3 {
                let facts = submission_document::locate(&input, 10).unwrap();
                assert!(std::ptr::eq(facts.input(), &input));
                assert_eq!(facts.study(), Some(3));
                assert_eq!(
                    facts.sections(),
                    [
                        (Submission::Send, 1),
                        (Submission::Adam, 5),
                        (Submission::Sdtm, 7)
                    ]
                );
                assert_eq!(facts.input().field(10, "sdtm"), Some(7));
            }
        }

        #[test]
        fn absent_declarations_and_original_shape_refusals_remain_distinct() {
            let empty = Document::new(vec![N::Mapping(vec![])], 0).unwrap();
            let facts = submission_document::locate(&empty, 0).unwrap();
            assert_eq!(facts.study(), None);
            assert!(facts.sections().is_empty());
            let input = Document::new(
                vec![N::Integer("1".into()), N::Null, N::Mapping(vec![(0, 1)])],
                2,
            )
            .unwrap();
            for (root, node) in [(0, 0), (2, 0), (99, 99)] {
                let error = submission_document::locate(&input, root).unwrap_err();
                assert!(std::ptr::eq(error.input, &input));
                assert_eq!(error.cause, Cause::Shape { node });
            }
        }

        use submission_document::{
            Context, DocumentKind, FieldName as F, GuideName, GuideStatus, Kind, Step,
        };

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
            fn section(&mut self, guide: &str) -> usize {
                let guide = self.guide(guide);
                let one = self.text("specs/dm.yaml");
                let two = self.text("specs/ae.yaml");
                let specs = self.node(N::Sequence(vec![one, two]));
                let define = self.text("results/define.xml");
                self.map(vec![
                    ("standard", guide),
                    ("specs", specs),
                    ("define", define),
                ])
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
            let enabled = a.node(N::Boolean(true));
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
            let adam = a.map(vec![
                ("documents", docs),
                ("originator", originator),
                ("standard", guide),
                ("specs", specs),
                ("define", define),
                ("dataset_json", enabled),
                ("context", context),
                ("xml_lang", xml_lang),
                ("file_oid", file),
                ("metadata_version", metadata),
                ("stylesheet", style),
            ]);
            let language = a.text("python");
            let root = a.map(vec![
                ("send", send),
                ("language", language),
                ("study", study),
                ("adam", adam),
                ("sdtm", sdtm),
            ]);
            a.finish(root)
        }
        #[test]
        fn complete_owned_metadata_preserves_order_overrides_and_occurrences_after_input_drops() {
            let input = full();
            let root = input.root();
            let study_node = input.field(root, "study").unwrap();
            let id_node = input.field(study_node, "id").unwrap();
            let owned = submission_document::decode(&input, root).unwrap();
            drop(input);
            assert_eq!(owned.root(), root);
            let study = owned.study().unwrap();
            assert_eq!(study.id.value, "STUDY.alpha-1");
            assert_eq!(study.name.value, "Independent study");
            assert_eq!(study.description.value, "A & B < C");
            assert_eq!(study.protocol_name.value, "P-001");
            assert_eq!(study.origin.node, Some(study_node));
            assert_eq!(study.id.origin.container, study_node);
            assert_eq!(study.id.origin.node, Some(id_node));
            assert_eq!(study.id.origin.field, F::Id);
            assert_eq!(
                owned
                    .sections()
                    .iter()
                    .map(|s| s.family)
                    .collect::<Vec<_>>(),
                [Submission::Send, Submission::Adam, Submission::Sdtm]
            );
            let send = &owned.sections()[0];
            assert_eq!(send.standard.name.value, GuideName::SendIgDart);
            assert_eq!(send.standard.version.value, "3.4");
            assert_eq!(send.standard.status.value, GuideStatus::Final);
            assert_eq!(
                send.specs
                    .value
                    .iter()
                    .map(|s| s.value.as_str())
                    .collect::<Vec<_>>(),
                ["specs/dm.yaml", "specs/ae.yaml"]
            );
            assert_eq!(send.define.value, "results/define.xml");
            assert!(send.documents.value.is_empty());
            assert!(!send.dataset_json.value);
            assert_eq!(send.context.value, Context::Submission);
            assert_eq!(send.xml_lang.value, "en");
            for origin in [
                send.documents.origin,
                send.dataset_json.origin,
                send.context.origin,
                send.xml_lang.origin,
            ] {
                assert_eq!(origin.node, None);
                assert_eq!(origin.key, None);
                assert_eq!(origin.container, send.origin.node.unwrap());
            }
            assert!(
                send.file_oid.is_none()
                    && send.metadata_version.is_none()
                    && send.originator.is_none()
                    && send.stylesheet.is_none()
            );
            let adam = &owned.sections()[1];
            assert_eq!(adam.standard.name.value, GuideName::AdamIgMd);
            assert!(adam.dataset_json.value);
            assert_eq!(adam.context.value, Context::Other);
            assert_eq!(adam.xml_lang.value, "fr-CA");
            assert_eq!(adam.file_oid.as_ref().unwrap().value, "FILE.analysis-1");
            let metadata = adam.metadata_version.as_ref().unwrap();
            assert_eq!(metadata.id.value, "MDV.analysis-1");
            assert_eq!(metadata.name.value, "Analysis metadata");
            assert_eq!(
                metadata.description.as_ref().unwrap().value,
                "Sponsor metadata"
            );
            assert_eq!(
                adam.originator.as_ref().unwrap().value,
                "Independent sponsor"
            );
            assert_eq!(adam.stylesheet.as_ref().unwrap().value, "styles/define.xsl");
            assert_eq!(
                adam.documents
                    .value
                    .iter()
                    .map(|d| (
                        d.id.value.as_str(),
                        d.kind.value,
                        d.link.value.as_str(),
                        d.title.value.as_str()
                    ))
                    .collect::<Vec<_>>(),
                [
                    (
                        "review",
                        DocumentKind::Supplemental,
                        "reviewers/review.pdf",
                        "Reviewer guide"
                    ),
                    (
                        "acrf",
                        DocumentKind::AnnotatedCrf,
                        "forms/acrf.pdf",
                        "Annotated CRF"
                    ),
                    ("other", DocumentKind::Other, "notes/data.txt", "Notes"),
                ]
            );
            assert_eq!(owned.sections()[2].standard.name.value, GuideName::SdtmIgAp);
        }
        #[test]
        fn every_shipped_guide_spelling_is_closed_metadata_not_family_composition() {
            for (text, expected) in [
                ("ADaMIG", GuideName::AdamIg),
                ("ADaMIG-MD", GuideName::AdamIgMd),
                ("BIMO", GuideName::Bimo),
                ("CDISC/NCI", GuideName::CdiscNci),
                ("SDTMIG", GuideName::SdtmIg),
                ("SDTMIG-AP", GuideName::SdtmIgAp),
                ("SDTMIG-MD", GuideName::SdtmIgMd),
                ("SENDIG", GuideName::SendIg),
                ("SENDIG-AR", GuideName::SendIgAr),
                ("SENDIG-DART", GuideName::SendIgDart),
                ("SENDIG-GENETOX", GuideName::SendIgGenetox),
            ] {
                let mut a = Arena::default();
                let section = a.section(text);
                let root = a.map(vec![("sdtm", section)]);
                let input = a.finish(root);
                let owned = submission_document::decode(&input, root).unwrap();
                assert_eq!(owned.sections()[0].standard.name.value, expected);
                assert_eq!(owned.sections()[0].standard.name.value.name(), text);
                assert!(owned.study().is_none()); // missing generator prerequisites remain a separate use case.
            }
        }
        #[test]
        fn absent_declarations_and_explicit_default_values_have_distinct_origin_facts() {
            let mut a = Arena::default();
            let language = a.text("r");
            let root = a.map(vec![("language", language)]);
            let input = a.finish(root);
            let owned = submission_document::decode(&input, root).unwrap();
            assert!(owned.study().is_none());
            assert!(owned.sections().is_empty());
            let mut a = Arena::default();
            let guide = a.guide("SDTMIG");
            let specs = a.node(N::Sequence(vec![]));
            let define = a.text("define.xml");
            let documents = a.node(N::Sequence(vec![]));
            let flag = a.node(N::Boolean(false));
            let context = a.text("Submission");
            let lang = a.text("en");
            let section = a.map(vec![
                ("standard", guide),
                ("specs", specs),
                ("define", define),
                ("documents", documents),
                ("dataset_json", flag),
                ("context", context),
                ("xml_lang", lang),
            ]);
            let root = a.map(vec![("sdtm", section)]);
            let input = a.finish(root);
            let owned = submission_document::decode(&input, root).unwrap();
            let s = &owned.sections()[0];
            for (origin, expected) in [
                (s.documents.origin, documents),
                (s.dataset_json.origin, flag),
                (s.context.origin, context),
                (s.xml_lang.origin, lang),
            ] {
                assert_eq!(origin.node, Some(expected));
                assert!(origin.key.is_some());
            }
            assert!(s.specs.value.is_empty());
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
            let flag = a.text("true");
            let context = a.text("submission");
            let language = a.text("e-123456789");
            let file = a.text("");
            let metadata = a.text_map(&[("id", "MDV"), ("name", ""), ("description", "")]);
            let originator = a.node(N::Boolean(true));
            let style = a.text("/absolute.xsl");
            let unknown = a.text("1.0");
            let section = a.map(vec![
                ("standard", guide),
                ("specs", specs),
                ("define", define),
                ("documents", documents),
                ("dataset_json", flag),
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
        #[test]
        fn structural_findings_are_complete_ordered_and_only_supported_causes_are_portable() {
            let input = invalid();
            let error = submission_document::decode(&input, input.root()).unwrap_err();
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
                    Kind::ExpectedBoolean,
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
            assert_eq!(
                findings
                    .iter()
                    .filter_map(|f| f.requirement())
                    .collect::<Vec<_>>(),
                ["REQ-1068", "REQ-1074", "REQ-1046", "REQ-1070", "REQ-1073", "REQ-1070"]
            );
            assert_eq!(findings[7].requirement(), None); // null is an original shape, not a fabricated normative message.
            assert!(matches!(error.input.nodes()[findings[7].node], N::Null));
        }
        #[test]
        fn complete_findings_preserve_original_invalid_input() {
            let input = invalid();
            for _ in 0..2 {
                let error = submission_document::decode(&input, input.root()).unwrap_err();
                assert!(std::ptr::eq(error.input, &input));
                assert!(error.input.field(error.input.root(), "adam").is_some());
                assert!(matches!(error.cause, Cause::Findings(v) if v.len() == 20));
            }
        }

        #[test]
        fn independent_whole_model_decodes_keep_all_sections() {
            let input = full();
            for _ in 0..2 {
                let owner = submission_document::decode(&input, input.root()).unwrap();
                assert_eq!(owner.sections().len(), 3);
                assert_eq!(owner.study().unwrap().id.value, "STUDY.alpha-1");
                assert_eq!(owner.sections()[1].documents.value.len(), 3);
            }
        }

        #[test]
        fn null_optional_fields_are_original_shape_findings_not_absent_defaults() {
            let mut a = Arena::default();
            let guide = a.guide("SENDIG");
            let specs = a.node(N::Sequence(vec![]));
            let define = a.text("d.xml");
            let mut fields = vec![("standard", guide), ("specs", specs), ("define", define)];
            for key in [
                "documents",
                "dataset_json",
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
            let error = submission_document::decode(&input, root).unwrap_err();
            let Cause::Findings(findings) = error.cause else {
                panic!("expected findings")
            };
            assert_eq!(
                findings.iter().map(|f| f.kind).collect::<Vec<_>>(),
                [
                    Kind::ExpectedSequence,
                    Kind::ExpectedBoolean,
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
                .all(|f| f.requirement().is_none() && matches!(input.nodes()[f.node], N::Null)));
        }
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
                let error = submission_document::decode(&input, root).unwrap_err();
                assert!(std::ptr::eq(error.input, &input));
                assert!(
                    matches!(error.cause, Cause::Findings(v) if v.len() == 1 && v[0].node == node && v[0].kind == Kind::ExpectedMapping && v[0].requirement().is_none())
                );
            }
            let mut a = Arena::default();
            let key = a.node(N::Boolean(true));
            let value = a.node(N::Null);
            let study = a.node(N::Mapping(vec![(key, value)]));
            let root = a.map(vec![("study", study)]);
            let input = a.finish(root);
            let Cause::Findings(findings) =
                submission_document::decode(&input, root).unwrap_err().cause
            else {
                panic!("findings")
            };
            assert_eq!(findings.len(), 5);
            assert_eq!(findings[0].node, key);
            assert_eq!(findings[0].kind, Kind::NonTextKey);
            assert!(findings[1..]
                .iter()
                .all(|f| f.kind == Kind::Missing && f.origin.unwrap().node.is_none()));
        }
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
            let error = submission_document::decode(&input, root).unwrap_err();
            assert!(
                matches!(error.cause, Cause::Findings(v) if v.len() == 1 && v[0].kind == Kind::ExpectedMapping)
            );
        }

        #[test]
        fn unknown_fields_and_nontext_keys_retain_exact_container_and_key_occurrences() {
            let input = invalid();
            let root = input.root();
            let section = input.field(root, "adam").unwrap();
            let Cause::Findings(findings) =
                submission_document::decode(&input, root).unwrap_err().cause
            else {
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
            assert_eq!(unknown.requirement(), None);
            let mut a = Arena::default();
            let key = a.node(N::Integer("2".into()));
            let value = a.node(N::Null);
            let root = a.node(N::Mapping(vec![(key, value)]));
            let input = a.finish(root);
            let Cause::Findings(findings) =
                submission_document::decode(&input, root).unwrap_err().cause
            else {
                panic!("findings")
            };
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].kind, Kind::NonTextKey);
            assert_eq!(findings[0].container, Some(root));
            assert_eq!(findings[0].key, Some(key));
            assert_eq!(findings[0].node, key);
        }
        #[test]
        fn present_metadata_description_is_optional_and_draft_status_is_retained() {
            let mut a = Arena::default();
            let guide = a.text_map(&[("name", "ADaMIG"), ("version", "1.3"), ("status", "Draft")]);
            let specs = a.node(N::Sequence(vec![]));
            let define = a.text("define.xml");
            let metadata = a.text_map(&[("id", "MDV"), ("name", "Metadata")]);
            let section = a.map(vec![
                ("standard", guide),
                ("specs", specs),
                ("define", define),
                ("metadata_version", metadata),
            ]);
            let root = a.map(vec![("adam", section)]);
            let input = a.finish(root);
            let owned = submission_document::decode(&input, root).unwrap();
            assert_eq!(
                owned.sections()[0].standard.status.value,
                GuideStatus::Draft
            );
            assert_eq!(owned.sections()[0].standard.version.value, "1.3");
            assert!(owned.sections()[0]
                .metadata_version
                .as_ref()
                .unwrap()
                .description
                .is_none());
        }
        #[test]
        fn default_text_and_empty_root_remain_owned_without_authored_values() {
            let mut a = Arena::default();
            let guide = a.guide("SDTMIG");
            let specs = a.node(N::Sequence(vec![]));
            let define = a.text("d.xml");
            let section = a.map(vec![
                ("standard", guide),
                ("specs", specs),
                ("define", define),
            ]);
            let root = a.map(vec![("sdtm", section)]);
            let input = a.finish(root);
            let owned = submission_document::decode(&input, root).unwrap();
            assert_eq!(owned.sections()[0].xml_lang.value, "en");
            assert_eq!(owned.sections()[0].xml_lang.origin.node, None);
            let empty = Document::new(vec![N::Mapping(vec![])], 0).unwrap();
            let owned = submission_document::decode(&empty, 0).unwrap();
            assert!(owned.sections().is_empty());
            assert!(owned.study().is_none());
        }

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
            let owned = submission_document::decode(&input, root).unwrap();
            drop(input);
            assert_eq!(owned.study().unwrap().name.value, "A\nB\t'\\");
            assert_eq!(owned.study().unwrap().description.value, "é < 漢字 & 😀");
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
                    ("standard", guide),
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

        #[test]
        fn malformed_document_items_retain_actual_occurrences_in_every_family_and_position() {
            let (input, expected) = malformed_document_lists();
            let error = submission_document::decode(&input, input.root()).unwrap_err();
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
                assert_eq!(finding.requirement(), None);
                assert_eq!(
                    finding.origin,
                    Some(submission_document::Origin {
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

        #[test]
        fn all_malformed_document_items_keep_the_original_input_and_nodes() {
            let (input, expected) = malformed_document_lists();
            let error = submission_document::decode(&input, input.root()).unwrap_err();
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

        #[test]
        fn valid_document_fields_and_omitted_defaults_remain_owned_after_original_input_drops() {
            let input = full();
            let adam = input.field(input.root(), "adam").unwrap();
            let sequence = input.field(adam, "documents").unwrap();
            let N::Sequence(items) = &input.nodes()[sequence] else {
                panic!("document list")
            };
            let document_nodes = items.clone();
            let field_nodes = document_nodes
                .iter()
                .map(|&node| {
                    ["id", "kind", "link", "title"].map(|name| input.field(node, name).unwrap())
                })
                .collect::<Vec<_>>();
            let owned = submission_document::decode(&input, input.root()).unwrap();
            drop(input);
            let docs = &owned.sections()[1].documents;
            assert_eq!(docs.origin.container, adam);
            assert_eq!(docs.origin.node, Some(sequence));
            assert!(docs.origin.key.is_some());
            assert_eq!(docs.value.len(), 3);
            for ((document, &node), expected) in
                docs.value.iter().zip(&document_nodes).zip(&field_nodes)
            {
                assert_eq!(document.node, node);
                for (origin, &value_node) in [
                    document.id.origin,
                    document.kind.origin,
                    document.link.origin,
                    document.title.origin,
                ]
                .iter()
                .zip(expected)
                {
                    assert_eq!(origin.container, node);
                    assert_eq!(origin.node, Some(value_node));
                    assert!(origin.key.is_some());
                }
            }
            assert_eq!(docs.value[0].title.value, "Reviewer guide");
            assert_eq!(docs.value[1].kind.value, DocumentKind::AnnotatedCrf);
            assert_eq!(docs.value[2].link.value, "notes/data.txt");
            let send = &owned.sections()[0];
            assert!(send.documents.value.is_empty());
            assert_eq!(send.documents.origin.container, send.origin.node.unwrap());
            assert_eq!(send.documents.origin.key, None);
            assert_eq!(send.documents.origin.node, None);
        }

        #[test]
        fn approved_source_system_version_is_admitted_for_every_section() {
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
                let root = a.map(vec![(family, section)]);
                let input = a.finish(root);
                let owned = submission_document::decode(&input, root).unwrap();
                drop(input);
                let section = &owned.sections()[0];
                let field = section.source_system_version.as_ref().unwrap();
                assert_eq!(field.value, "1.0");
                assert_eq!(field.origin.field, F::SourceSystemVersion);
                assert_eq!(field.origin.node, Some(version));
                assert_eq!(field.origin.key, Some(key));
                let selected = section.source_versions("0.3.0").unwrap();
                assert_eq!(selected.metadata(), "1.0");
                assert_eq!(selected.installed(), "0.3.0");
                assert_eq!(selected.override_origin(), Some(field.origin));
                assert_eq!(selected.metadata().as_ptr(), field.value.as_ptr());
                assert_eq!(
                    section.source_versions(""),
                    Err(submission_document::VersionError::EmptyInstalledVersion)
                );
            }
        }

        #[test]
        fn omitted_submission_version_uses_actual_generator_without_creating_an_authored_field() {
            let input = full();
            let owned = submission_document::decode(&input, input.root()).unwrap();
            for section in owned.sections() {
                assert!(section.source_system_version.is_none());
                for installed in ["0.3.0", "0.4.0.dev7"] {
                    let selected = section.source_versions(installed).unwrap();
                    assert_eq!(selected.metadata(), installed);
                    assert_eq!(selected.installed(), installed);
                    assert_eq!(selected.override_origin(), None);
                    assert_eq!(selected.installed().as_ptr(), installed.as_ptr());
                }
            }
        }

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
                let error = submission_document::decode(&input, root).unwrap_err();
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
                assert_eq!(findings[0].requirement(), None);
            }
        }

        #[test]
        fn full_submission_version_text_keeps_its_authored_origin_after_ownership() {
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
            let owned = submission_document::decode(&input, root).unwrap();
            drop(input);
            let selected = owned.sections()[0].source_system_version.as_ref().unwrap();
            assert_eq!(selected.value, "v".repeat(4096));
            assert_eq!(selected.origin.node, Some(version));
            assert_eq!(selected.origin.key, Some(key));
        }
    }
    mod submission_values {
        //! Independently authored normalized metadata; no generator supplies expectations.
        use yamaa_core::{
            project_terminology::Catalogue,
            schema::{Document, DocumentNode as N},
            specification_submission_document as declarations,
            submission_composition::{self, Input, Study},
            submission_document, submission_xml,
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
            fn finish(self, root: usize) -> Document {
                Document::new(self.0, root).unwrap()
            }
        }
        fn environment() -> Document {
            let mut a = Arena::default();
            let study = a.texts(&[
                ("id", "VLSTUDY"),
                ("name", "Value metadata study"),
                ("description", "Independent structure witness"),
                ("protocol_name", "VLPROTOCOL"),
            ]);
            let standard = a.texts(&[("name", "SDTMIG"), ("version", "3.4"), ("status", "Final")]);
            let filename = a.text("spec.yaml");
            let specs = a.node(N::Sequence(vec![filename]));
            let define = a.text("define.xml");
            let version = a.text("1.0");
            let section = a.map(vec![
                ("standard", standard),
                ("specs", specs),
                ("define", define),
                ("source_system_version", version),
            ]);
            let root = a.map(vec![("study", study), ("sdtm", section)]);
            a.finish(root)
        }
        fn specification(case: &str) -> Document {
            let mut a = Arena::default();
            let domain = a.text(if case == "no discriminator" {
                "XX"
            } else {
                "LB"
            });
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
            for name in ["LBTESTCD", "LBORRESU"] {
                let origin = a.texts(&[("type", "Assigned"), ("source", "Sponsor")]);
                let core = a.text("Exp");
                let length = a.node(N::Integer("8".into()));
                let submission =
                    a.map(vec![("core", core), ("length", length), ("origin", origin)]);
                let name_node = a.text(name);
                let kind = a.text("str");
                let label = a.text(if name == "LBORRESU" {
                    "Original units"
                } else {
                    "Test code"
                });
                let mut fields = vec![
                    ("name", name_node),
                    ("type", kind),
                    ("label", label),
                    ("submission", submission),
                ];
                if name == "LBORRESU" && case != "no derivation" {
                    let literal = a.texts(&[("literal", "mg/dL")]);
                    fields.push(("derivation", literal));
                }
                columns.push(a.map(fields));
            }
            let columns = a.node(N::Sequence(columns));
            let mut names = vec![a.text("LBTESTCD")];
            if case != "not output" {
                names.push(a.text("LBORRESU"));
            }
            let output_names = a.node(N::Sequence(names));
            let output_path = a.text("lb.csv");
            let output = a.map(vec![("path", output_path), ("columns", output_names)]);
            let mut rows = Vec::new();
            let row_order: &[usize] = match case {
                "reordered values" => &[1, 0],
                "inserted value" => &[2, 0, 1],
                _ => &[0, 1],
            };
            for &index in row_order {
                let id = a.text(match (index, case) {
                    (0, "arbitrary row ID" | "arbitrary invalid source") => {
                        "glucose / primary test: 1"
                    }
                    (0, "empty row ID") => "",
                    (0, _) => "glucose",
                    (1, _) => "creatinine",
                    _ => "albumin",
                });
                let literal = if case == "dynamic discriminator" && index == 0 {
                    a.texts(&[("source", "ODM.TestCode")])
                } else if case == "ambiguous discriminator" && index == 0 {
                    a.texts(&[("literal", "GLUC"), ("source", "ODM.TestCode")])
                } else if case == "nonstring discriminator" && index == 0 {
                    let number = a.node(N::Integer("1".into()));
                    a.map(vec![("literal", number)])
                } else {
                    a.texts(&[(
                        "literal",
                        if index == 0 && case == "escaped test code" {
                            "A.&\" /é"
                        } else if index == 0 || matches!(case, "conflict" | "identical") {
                            "GLUC"
                        } else if index == 1 {
                            "CREAT"
                        } else {
                            "ALB"
                        },
                    )])
                };
                let literal = if case == "normalized discriminator" {
                    let unconvertible = a.node(N::Null);
                    a.map(vec![("value", literal), ("unconvertible", unconvertible)])
                } else {
                    literal
                };
                let derivations = a.map(vec![("LBTESTCD", literal)]);
                // A literal refutes Derived: the value is read from no input.
                let origin_type = if case == "refuted origin" && index == 0 {
                    "Derived"
                } else {
                    "Assigned"
                };
                let source = if (case == "invalid source" && index == 1)
                    || (case == "arbitrary invalid source" && index == 0)
                {
                    "Investigator"
                } else if index == 0 || case == "identical" {
                    "Vendor"
                } else {
                    "Sponsor"
                };
                let origin = a.texts(&[("type", origin_type), ("source", source)]);
                let method = a.text("Units assigned from the supplied test metadata");
                let mut metadata_fields = vec![("origin", origin), ("method", method)];
                if matches!(
                    case,
                    "stable values" | "reordered values" | "inserted value"
                ) {
                    let comment = a.text("Independent value metadata comment");
                    metadata_fields.push(("comment", comment));
                }
                let metadata = a.map(metadata_fields);
                let submission = a.map(vec![(
                    if case == "unknown column" && index == 0 {
                        "BOGUS"
                    } else {
                        "LBORRESU"
                    },
                    metadata,
                )]);
                rows.push(a.map(vec![
                    ("id", id),
                    ("derivations", derivations),
                    ("submission", submission),
                ]));
            }
            let rows = a.node(N::Sequence(rows));
            let root = a.map(vec![
                ("domain", domain),
                ("submission", dataset),
                ("columns", columns),
                ("output", output),
                ("rows", rows),
            ]);
            a.finish(root)
        }
        fn compose(document: &Document) -> Result<Study, Vec<submission_composition::Finding>> {
            let environment = environment();
            let declared = submission_document::decode(&environment, environment.root()).unwrap();
            submission_composition::compose(
                declared.study().unwrap(),
                &declared.sections()[0],
                &Catalogue::admit(vec![]).unwrap(),
                &[Input {
                    document,
                    source: "spec.yaml",
                    artifact: "lb.csv",
                    href: "lb.csv",
                }],
                &[],
                "0.3.0",
                "2026-09-11T00:00:00",
            )
        }

        #[test]
        fn overrides_inherit_complete_metadata_and_preserve_written_occurrences() {
            let document = specification("valid");
            let declared = declarations::decode(&document, document.root()).unwrap();
            assert_eq!(declared.value_rows().len(), 2);
            let row = &declared.value_rows()[0];
            assert_eq!(row.id.value, "glucose");
            assert_eq!(document.field(row.node, "submission"), row.origin.node);
            assert_eq!(row.overrides[0].name.value, "LBORRESU");
            assert!(row.overrides[0].submission.core.is_none());
            let study = compose(&document).unwrap();
            let column = &study.datasets()[0].columns()[1];
            assert_eq!(column.values().len(), 2);
            let value = &column.values()[0];
            assert_eq!(value.test_code(), "GLUC");
            let metadata = value.column().declaration().submission.as_ref().unwrap();
            assert_eq!(
                metadata.core.as_ref().unwrap().value,
                declarations::Core::Expected
            );
            assert_eq!(metadata.length.as_ref().unwrap().value, "8");
            assert_eq!(
                metadata
                    .origin_metadata
                    .as_ref()
                    .unwrap()
                    .source
                    .as_ref()
                    .unwrap()
                    .value,
                declarations::OriginSource::Vendor
            );
        }
        #[test]
        fn complete_row_metadata_keeps_all_admitted_overrides() {
            let document = specification("valid");
            assert_eq!(
                declarations::decode(&document, document.root())
                    .unwrap()
                    .value_rows()
                    .len(),
                2
            );
        }
        #[test]
        fn arbitrary_row_labels_and_normalized_literal_handlers_retain_their_meaning() {
            for case in ["arbitrary row ID", "normalized discriminator"] {
                let document = specification(case);
                let declared = declarations::decode(&document, document.root()).unwrap();
                let study = compose(&document).unwrap();
                let value = &study.datasets()[0].columns()[1].values()[0];
                assert_eq!(value.test_code(), "GLUC");
                if case == "arbitrary row ID" {
                    assert_eq!(
                        declared.value_rows()[0].id.value,
                        "glucose / primary test: 1"
                    );
                }
            }
            let findings = compose(&specification("arbitrary invalid source")).unwrap_err();
            assert!(findings.iter().any(|finding| finding.definition().condition
                == "origin_source_not_admitted"
                && finding
                    .path
                    .ends_with("rows.glucose / primary test: 1.submission.LBORRESU.origin")));
        }
        #[test]
        fn an_empty_row_label_is_a_text_refusal_with_its_original_field_occurrence() {
            let document = specification("empty row ID");
            let error = declarations::decode(&document, document.root()).unwrap_err();
            assert!(std::ptr::eq(error.input, &document));
            let declarations::Cause::Findings(findings) = error.cause else {
                panic!("expected a structural text refusal");
            };
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].kind, declarations::Kind::EmptyText);
            assert!(findings[0].requirement().is_none());
            assert_eq!(
                findings[0].path.steps().collect::<Vec<_>>(),
                [
                    declarations::Step::Field(declarations::FieldName::Rows),
                    declarations::Step::Index(0),
                    declarations::Step::Field(declarations::FieldName::Id),
                ]
            );
        }
        #[test]
        fn each_independent_value_admission_control_has_the_required_condition_and_row_path() {
            for (case, condition, requirement, suffix) in [
                (
                    "no discriminator",
                    "value_metadata_requires_testcd",
                    "REQ-1163",
                    "rows.glucose.submission",
                ),
                (
                    "dynamic discriminator",
                    "value_metadata_requires_literal_testcd",
                    "REQ-1164",
                    "rows.glucose.submission",
                ),
                (
                    "ambiguous discriminator",
                    "value_metadata_requires_literal_testcd",
                    "REQ-1164",
                    "rows.glucose.submission",
                ),
                (
                    "nonstring discriminator",
                    "value_metadata_requires_literal_testcd",
                    "REQ-1164",
                    "rows.glucose.submission",
                ),
                (
                    "unknown column",
                    "value_metadata_unknown_column",
                    "REQ-1165",
                    "rows.glucose.submission.BOGUS",
                ),
                (
                    "no derivation",
                    "value_metadata_unknown_column",
                    "REQ-1165",
                    "rows.glucose.submission.LBORRESU",
                ),
                (
                    "not output",
                    "value_metadata_not_output_column",
                    "REQ-1166",
                    "rows.glucose.submission.LBORRESU",
                ),
                (
                    "conflict",
                    "conflicting_value_metadata",
                    "REQ-1167",
                    "rows.creatinine.submission.LBORRESU",
                ),
                (
                    "refuted origin",
                    "refuted_value_origin",
                    "REQ-1168",
                    "rows.glucose.submission.LBORRESU",
                ),
                (
                    "invalid source",
                    "origin_source_not_admitted",
                    "REQ-0919",
                    "rows.creatinine.submission.LBORRESU.origin",
                ),
            ] {
                let findings = compose(&specification(case)).unwrap_err();
                assert!(
                    findings
                        .iter()
                        .any(|finding| finding.definition().condition == condition
                            && finding.definition().requirement == Some(requirement)
                            && finding.path.ends_with(suffix)),
                    "{case}: {findings:?}"
                );
            }
        }
        #[test]
        fn identical_declarations_at_distinct_nodes_share_one_value_definition() {
            let document = specification("identical");
            let declared = declarations::decode(&document, document.root()).unwrap();
            let a = &declared.value_rows()[0].overrides[0].submission;
            let b = &declared.value_rows()[1].overrides[0].submission;
            assert_ne!(a, b, "distinct occurrence facts are retained");
            assert!(a.same_values(b));
            let study = compose(&document).unwrap();
            assert_eq!(study.datasets()[0].columns()[1].values().len(), 1);
        }
        #[test]
        fn fixed_time_xml_has_one_column_list_two_ordered_item_and_where_references() {
            let study = compose(&specification("valid")).unwrap();
            let xml = String::from_utf8(submission_xml::render(&study).unwrap()).unwrap();
            assert!(xml.contains("CreationDateTime=\"2026-09-11T00:00:00\""));
            assert!(xml.contains("SourceSystemVersion=\"1.0\""));
            assert_eq!(xml.matches("<def:ValueListDef ").count(), 1);
            assert_eq!(xml.matches("<def:ValueListRef ").count(), 1);
            assert_eq!(xml.matches("<def:WhereClauseDef ").count(), 2);
            assert_eq!(xml.matches("<ItemDef ").count(), 4);
            assert!(xml.contains("<def:ValueListRef ValueListOID=\"VL.LB.LBORRESU\"/>"));
            for (index, code, source) in [(1, "GLUC", "Vendor"), (2, "CREAT", "Sponsor")] {
                let identity = format!("LB.LBORRESU.{code}");
                assert!(xml.contains(&format!("ItemOID=\"IT.{identity}\" OrderNumber=\"{index}\" Mandatory=\"No\" MethodOID=\"MT.{identity}\"")));
                assert!(xml.contains(&format!(
                    "<def:WhereClauseRef WhereClauseOID=\"WC.{identity}\"/>"
                )));
                assert!(xml.contains(&format!("<CheckValue>{code}</CheckValue>")));
                assert!(xml.contains(&format!(
                    "<def:Origin Type=\"Assigned\" Source=\"{source}\"/>"
                )));
                assert!(xml.contains(&format!("<MethodDef OID=\"MT.{identity}\"")));
            }
            assert_eq!(
                xml.matches("Comparator=\"EQ\" SoftHard=\"Soft\" def:ItemOID=\"IT.LB.LBTESTCD\"")
                    .count(),
                2
            );
            assert!(
                xml.find("<def:ValueListDef ").unwrap() < xml.find("<def:WhereClauseDef ").unwrap()
            );
            assert!(
                xml.find("<def:WhereClauseDef ").unwrap() < xml.find("<ItemGroupDef ").unwrap()
            );
            let start = xml.find("<ItemDef OID=\"IT.LB.LBORRESU\"").unwrap();
            let item = &xml[start..start + xml[start..].find("</ItemDef>").unwrap()];
            assert!(item.find("<def:Origin ").unwrap() < item.find("<def:ValueListRef ").unwrap());
        }

        #[test]
        fn value_identifiers_survive_unrelated_insertion_and_row_reordering() {
            let render = |case| {
                String::from_utf8(
                    submission_xml::render(&compose(&specification(case)).unwrap()).unwrap(),
                )
                .unwrap()
            };
            let original = render("stable values");
            let reordered = render("reordered values");
            let inserted = render("inserted value");
            for xml in [&original, &reordered, &inserted] {
                for code in ["GLUC", "CREAT"] {
                    for prefix in ["IT", "MT", "COM", "WC"] {
                        let oid = format!("{prefix}.LB.LBORRESU.{code}");
                        assert_eq!(xml.matches(&format!(" OID=\"{oid}\"")).count(), 1);
                    }
                    assert!(xml.contains(&format!(
                        "<def:WhereClauseRef WhereClauseOID=\"WC.LB.LBORRESU.{code}\"/>"
                    )));
                }
                assert_eq!(xml.matches(" OID=\"VL.LB.LBORRESU\"").count(), 1);
            }
            assert!(original.contains("ItemOID=\"IT.LB.LBORRESU.GLUC\" OrderNumber=\"1\""));
            assert!(reordered.contains("ItemOID=\"IT.LB.LBORRESU.GLUC\" OrderNumber=\"2\""));
            assert!(inserted.contains("ItemOID=\"IT.LB.LBORRESU.GLUC\" OrderNumber=\"2\""));
            assert!(inserted.contains("ItemOID=\"IT.LB.LBORRESU.ALB\" OrderNumber=\"1\""));
        }

        #[test]
        fn value_identifier_retains_the_literal_with_xml_attribute_escaping() {
            let xml = String::from_utf8(
                submission_xml::render(&compose(&specification("escaped test code")).unwrap())
                    .unwrap(),
            )
            .unwrap();
            assert!(xml.contains("ItemOID=\"IT.LB.LBORRESU.A.&amp;&quot; /é\""));
            assert!(xml.contains(" OID=\"WC.LB.LBORRESU.A.&amp;&quot; /é\""));
            assert!(xml.contains("<CheckValue>A.&amp;\" /é</CheckValue>"));
        }
    }
}
