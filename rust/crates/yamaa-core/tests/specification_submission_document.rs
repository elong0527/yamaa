use yamaa_core::{
    schema::{Document, DocumentLimits, DocumentNode as N},
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
        Document::new(self.0, root, DocumentLimits::default()).unwrap()
    }
}
fn decode(input: &Document) -> Declarations {
    submission::decode(input, input.root(), Limits::default()).unwrap()
}
fn findings(input: &Document) -> Vec<Finding> {
    let error = submission::decode(input, input.root(), Limits::default()).unwrap_err();
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
    let locations = submission::locate(&input, root, Limits::default()).unwrap();
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
fn locations_admit_whole_columns_fields_work_and_declarations_before_growth() {
    let mut a = Arena::default();
    let one = a.node(N::Null);
    let two = a.node(N::Null);
    let first = a.column("A", "str", Some(one));
    let second = a.column("B", "str", Some(two));
    let root = a.root(None, vec![first, second]);
    let input = a.finish(root);
    for (limits, resource) in [
        (
            Limits {
                fields: 6,
                ..Limits::default()
            },
            Resource::Fields,
        ),
        (
            Limits {
                columns: 1,
                ..Limits::default()
            },
            Resource::Columns,
        ),
        (
            Limits {
                declarations: 1,
                ..Limits::default()
            },
            Resource::Declarations,
        ),
        (
            Limits {
                work: 1,
                ..Limits::default()
            },
            Resource::Work,
        ),
    ] {
        let error = submission::locate(&input, root, limits).unwrap_err();
        assert!(std::ptr::eq(error.input, &input));
        assert_eq!(error.cause, Cause::Limit(resource));
    }
}
#[test]
fn locations_absence_and_original_shapes_remain_distinct() {
    let empty = Document::new(vec![N::Mapping(vec![])], 0, DocumentLimits::default()).unwrap();
    let locations = submission::locate(&empty, 0, Limits::default()).unwrap();
    assert!(locations.dataset().is_none());
    assert!(locations.columns().is_empty());
    let input = Document::new(
        vec![N::Integer("7".into()), N::Null, N::Mapping(vec![(0, 1)])],
        2,
        DocumentLimits::default(),
    )
    .unwrap();
    for (root, node) in [(0, 0), (2, 0), (99, 99)] {
        let error = submission::locate(&input, root, Limits::default()).unwrap_err();
        assert_eq!(error.cause, Cause::Shape { node });
        assert!(std::ptr::eq(error.input, &input));
    }
    let mut a = Arena::default();
    let scalar = a.text("columns are not a scalar");
    let root = a.map(vec![("columns", scalar)]);
    let input = a.finish(root);
    assert_eq!(
        submission::locate(&input, root, Limits::default())
            .unwrap_err()
            .cause,
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
fn each_capacity_refusal_retains_exact_input_and_never_returns_fitting_findings() {
    let mut a = Arena::default();
    let dataset = a.dataset("SPECIAL PURPOSE", None);
    let sub = a.texts(&[("core", "Req")]);
    let column = a.column("A", "str", Some(sub));
    let root = a.root(Some(dataset), vec![column]);
    let input = a.finish(root);
    for (limits, resource) in [
        (
            Limits {
                fields: 2,
                ..Limits::default()
            },
            Resource::Fields,
        ),
        (
            Limits {
                columns: 0,
                ..Limits::default()
            },
            Resource::Columns,
        ),
        (
            Limits {
                declarations: 1,
                ..Limits::default()
            },
            Resource::Declarations,
        ),
        (
            Limits {
                work: 128,
                ..Limits::default()
            },
            Resource::Work,
        ),
        (
            Limits {
                nodes: 3,
                ..Limits::default()
            },
            Resource::Nodes,
        ),
        (
            Limits {
                depth: 3,
                ..Limits::default()
            },
            Resource::Depth,
        ),
        (
            Limits {
                list_items: 2,
                ..Limits::default()
            },
            Resource::ListItems,
        ),
        (
            Limits {
                text_bytes: 20,
                ..Limits::default()
            },
            Resource::TextBytes,
        ),
        (
            Limits {
                escaped_bytes: 200,
                ..Limits::default()
            },
            Resource::EscapedBytes,
        ),
    ] {
        let error = submission::decode(&input, root, limits).unwrap_err();
        assert!(std::ptr::eq(error.input, &input));
        assert_eq!(error.cause, Cause::Limit(resource));
    }
    let mut a = Arena::default();
    let core = a.text("wrong");
    let list = a.text("wrong-list");
    let sub = a.map(vec![("core", core), ("codelist", list)]);
    let column = a.column("A", "str", Some(sub));
    let root = a.root(None, vec![column]);
    let input = a.finish(root);
    for (limits, resource) in [
        (
            Limits {
                findings: 1,
                ..Limits::default()
            },
            Resource::Findings,
        ),
        (
            Limits {
                path_segments: 4,
                ..Limits::default()
            },
            Resource::PathSegments,
        ),
    ] {
        let error = submission::decode(&input, root, limits).unwrap_err();
        assert!(std::ptr::eq(error.input, &input));
        assert_eq!(error.cause, Cause::Limit(resource));
    }
    assert_eq!(findings(&input).len(), 2);
}
#[test]
fn malformed_deep_unknown_values_use_bounded_native_walk_before_findings() {
    let mut a = Arena::default();
    let mut value = a.text("deep");
    for _ in 0..10 {
        value = a.node(N::Sequence(vec![value]));
    }
    let sub = a.map(vec![("extension", value)]);
    let column = a.column("A", "str", Some(sub));
    let root = a.root(None, vec![column]);
    let input = a.finish(root);
    let error = submission::decode(
        &input,
        root,
        Limits {
            depth: 7,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.cause, Cause::Limit(Resource::Depth));
    assert!(std::ptr::eq(error.input, &input));
    assert_eq!(findings(&input)[0].kind, Kind::UnknownField);
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
fn cumulative_late_text_and_findings_limits_do_not_reset_between_columns() {
    let mut a = Arena::default();
    let first_sub = a.texts(&[("comment", "first bounded declaration")]);
    let second_sub = a.texts(&[("comment", "second bounded declaration")]);
    let first = a.column("A", "str", Some(first_sub));
    let second = a.column("B", "str", Some(second_sub));
    let root = a.root(None, vec![first, second]);
    let input = a.finish(root);
    // Total selected key/scalar text is independently 116 bytes. Each column
    // fits this quota alone, but the retained whole declaration set does not.
    let error = submission::decode(
        &input,
        root,
        Limits {
            text_bytes: 90,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.cause, Cause::Limit(Resource::TextBytes));
    assert!(std::ptr::eq(error.input, &input));
    let mut a = Arena::default();
    let one = a.texts(&[("core", "wrong")]);
    let two = a.texts(&[("core", "wrong")]);
    let first = a.column("A", "str", Some(one));
    let second = a.column("B", "str", Some(two));
    let root = a.root(None, vec![first, second]);
    let input = a.finish(root);
    let error = submission::decode(
        &input,
        root,
        Limits {
            findings: 1,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.cause, Cause::Limit(Resource::Findings));
    assert!(std::ptr::eq(error.input, &input));
    assert_eq!(findings(&input).len(), 2);
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
    for root in [scalar, usize::MAX] {
        let error = submission::decode(&input, root, Limits::default()).unwrap_err();
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
        DocumentLimits::default(),
    )
    .unwrap();
    let values = findings(&input);
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].node, 0);
    assert_eq!(values[0].key, Some(0));
    assert_eq!(values[0].kind, Kind::NonTextKey);
    assert_eq!(values[0].requirement(), None);
    let empty = Document::new(vec![N::Mapping(vec![])], 0, DocumentLimits::default()).unwrap();
    let owner = decode(&empty);
    assert!(owner.dataset().is_none());
    assert!(owner.columns().is_none());
}
