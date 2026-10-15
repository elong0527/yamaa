use yamaa_core::regex::{CompileError, Resource};
use yamaa_core::schema::{ConstraintError, ConstraintViolation as C};
use yamaa_core::schema::{
    Descriptor, DescriptorError, DescriptorIssue as I, Document, DocumentNode as N,
};

enum V {
    Text(&'static str),
    Owned(String),
    Int(&'static str),
    Bool(bool),
    Null,
    List(Vec<V>),
    Map(Vec<(&'static str, V)>),
}

/// Author decoded documents directly; neither a schema interpreter nor a parser supplies truth.
fn document(value: V) -> Document {
    fn push(value: V, nodes: &mut Vec<N>) -> usize {
        let node = match value {
            V::Text(v) => N::Text(v.into()),
            V::Owned(v) => N::Text(v),
            V::Int(v) => N::Integer(v.into()),
            V::Bool(v) => N::Boolean(v),
            V::Null => N::Null,
            V::List(items) => N::Sequence(items.into_iter().map(|v| push(v, nodes)).collect()),
            V::Map(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(key, value)| (push(V::Text(key), nodes), push(value, nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
    let mut nodes = Vec::new();
    let root = push(value, &mut nodes);
    Document::new(nodes, root).unwrap()
}

fn admit(value: V, class_field: bool) -> yamaa_core::schema::DescriptorReport {
    let d = document(value);
    Descriptor::admit(&d, d.root(), class_field).unwrap()
}

#[test]
fn descriptor_union_references_and_explicit_null_default_retain_written_order() {
    let d = document(V::Map(vec![
        (
            "type",
            V::List(vec![
                V::Text(" recursive "),
                V::Text("dict[str, list[other]]"),
                V::Text("recursive"),
            ]),
        ),
        ("required", V::Bool(false)),
        ("description", V::Text("A field")),
        ("default", V::Null),
    ]));
    let report = Descriptor::admit(&d, d.root(), true).unwrap();
    assert!(report.issues().is_empty());
    assert_eq!(
        report.references(),
        ["recursive", "str", "other", "recursive"]
    );
    let descriptor = report.into_descriptor().unwrap();
    assert!(!descriptor.required());
    assert_eq!(descriptor.members().len(), 3);
    assert_eq!(d.nodes()[descriptor.default_node().unwrap()], N::Null);
}

#[test]
fn findings_follow_keyword_type_required_description_constraint_order() {
    let report = admit(
        V::Map(vec![
            ("mystery", V::Null),
            ("values", V::Bool(true)),
            ("type", V::Text("int")),
            ("size", V::Int("-1")),
            ("required", V::Bool(true)),
            ("default", V::Null),
            ("description", V::Text(" \t")),
            ("pattern", V::Bool(false)),
            ("min_length", V::Bool(false)),
        ]),
        true,
    );
    assert_eq!(
        report.issues(),
        [
            I::UnknownKeyword { key: 0 },
            I::RequiredDefault,
            I::Description,
            I::PatternRequiresString,
            I::PatternText,
            I::MinimumRequiresString,
            I::MinimumNonnegative,
            I::SizeRequiresCollection,
            I::SizeNonnegative,
            I::ValuesRequiresString,
            I::ValuesTextSequence,
        ]
    );
    assert_eq!(report.references(), ["int"]);
    assert!(report.into_descriptor().is_none());

    let report = admit(V::Map(vec![("required", V::Bool(false))]), false);
    assert_eq!(
        report.issues(),
        [I::UnknownKeyword { key: 0 }, I::MissingType]
    );
    assert_eq!(admit(V::Null, false).issues(), [I::ExpectedMapping]);
}

#[test]
fn invalid_types_and_required_flags_do_not_become_boolean_integer_coercions() {
    for value in [
        V::Null,
        V::Bool(true),
        V::List(vec![]),
        V::List(vec![V::Text("str"), V::Bool(true)]),
    ] {
        assert_eq!(
            admit(V::Map(vec![("type", value)]), false).issues(),
            [I::InvalidTypeValue]
        );
    }
    assert_eq!(
        admit(
            V::Map(vec![("type", V::Text("str")), ("required", V::Int("1"))]),
            true
        )
        .issues(),
        [I::RequiredBoolean]
    );
    let report = admit(
        V::Map(vec![(
            "type",
            V::List(vec![V::Text("missing"), V::Text("list[]"), V::Text("str")]),
        )]),
        false,
    );
    assert_eq!(report.references(), ["missing", "str"]);
    assert_eq!(
        report.issues(),
        [I::TypeSyntax {
            member: "list[]".into(),
            byte: 5
        }]
    );
}

#[test]
fn constraints_retain_empty_choices_unicode_lengths_and_arbitrary_width_bounds() {
    let report = admit(
        V::Map(vec![
            ("type", V::Text("str")),
            ("values", V::List(vec![])),
            ("min_length", V::Int("3")),
            ("pattern", V::Text("a|bc")),
        ]),
        false,
    );
    assert!(report.issues().is_empty());
    let descriptor = report.into_descriptor().unwrap();
    assert_eq!(descriptor.permitted(), Some([].as_slice()));
    assert!(!descriptor.length_meets_minimum(2));
    assert!(descriptor.length_meets_minimum(3));
    let (text, pattern) = descriptor.pattern().unwrap();
    assert_eq!(text, "a|bc");
    for (value, expected) in [("a", true), ("bc", true), ("abc", false), ("bc\n", false)] {
        assert_eq!(pattern.full_match(value).unwrap().is_some(), expected);
    }
    let descriptor = admit(
        V::Map(vec![
            ("type", V::Text("list[str]")),
            ("size", V::Int("18446744073709551616")),
        ]),
        false,
    )
    .into_descriptor()
    .unwrap();
    assert_eq!(descriptor.size(), Some("18446744073709551616"));
    assert!(!descriptor.length_matches_size(0));
    assert!(!descriptor.length_matches_size(usize::MAX));
    let descriptor = admit(
        V::Map(vec![
            ("type", V::Text("str")),
            ("min_length", V::Int("18446744073709551616")),
        ]),
        false,
    )
    .into_descriptor()
    .unwrap();
    assert!(!descriptor.length_meets_minimum(usize::MAX));
}

#[test]
fn invalid_pattern_is_a_finding_but_resource_refusal_is_not() {
    let report = admit(
        V::Map(vec![("type", V::Text("str")), ("pattern", V::Text("["))]),
        false,
    );
    assert!(matches!(report.issues(), [I::InvalidPattern { pattern, .. }] if pattern == "["));
    let repaired_by_wrapper = admit(
        V::Map(vec![
            ("type", V::Text("str")),
            ("pattern", V::Text("a)|(?:b")),
        ]),
        false,
    );
    assert!(
        matches!(repaired_by_wrapper.issues(), [I::InvalidPattern { pattern, .. }] if pattern == "a)|(?:b")
    );
    let pattern = format!("{}a{}", "(".repeat(129), ")".repeat(129));
    let d = document(V::Map(vec![
        ("type", V::Text("str")),
        ("pattern", V::Owned(pattern)),
    ]));
    assert!(matches!(
        Descriptor::admit(&d, d.root(), false),
        Err(DescriptorError::Regex(CompileError::Limit {
            resource: Resource::Depth,
            limit: 128
        }))
    ));
}

#[test]
fn repeated_invalid_descriptors_keep_the_same_authored_pattern_finding() {
    let d = document(V::Map(vec![
        ("type", V::Text("str")),
        ("pattern", V::Text("[")),
    ]));
    for _ in 0..3 {
        assert!(
            matches!(Descriptor::admit(&d,d.root(),false).unwrap().issues(),[I::InvalidPattern{pattern,..}] if pattern=="[")
        );
    }
}

#[test]
fn independent_descriptor_pattern_admissions_keep_exact_lookbehind_metadata() {
    let pattern = format!("(a){}(?<=\\1)", "(a|aa)".repeat(8));
    let d = document(V::Map(vec![
        ("type", V::Text("str")),
        ("pattern", V::Owned(pattern.clone())),
    ]));
    for _ in 0..3 {
        let descriptor = Descriptor::admit(&d, d.root(), false)
            .unwrap()
            .into_descriptor()
            .unwrap();
        assert_eq!(descriptor.pattern().unwrap().0, pattern);
    }
}

#[test]
fn repeated_empty_metadata_and_invalid_description_remain_complete() {
    let d = document(V::Map(vec![
        ("type", V::Text("str")),
        (
            "values",
            V::List(vec![V::Text(""), V::Text(""), V::Text("")]),
        ),
    ]));
    for _ in 0..2 {
        let descriptor = Descriptor::admit(&d, d.root(), false)
            .unwrap()
            .into_descriptor()
            .unwrap();
        assert_eq!(descriptor.permitted().unwrap(), ["", "", ""]);
    }
    let d = document(V::Map(vec![
        ("type", V::Text("str")),
        ("description", V::Text("    ")),
    ]));
    for _ in 0..2 {
        assert_eq!(
            Descriptor::admit(&d, d.root(), false).unwrap().issues(),
            [I::Description]
        );
    }
}

#[test]
fn value_constraints_report_every_failure_in_declared_contract_order() {
    let descriptor = admit(
        V::Map(vec![
            ("type", V::Text("str")),
            ("min_length", V::Int("2")),
            ("pattern", V::Text("a+")),
            ("values", V::List(vec![V::Text("aa")])),
        ]),
        false,
    )
    .into_descriptor()
    .unwrap();
    assert_eq!(
        descriptor.check_constraints(&N::Text("é".into())).unwrap(),
        [C::ValueNotPermitted, C::PatternMismatch, C::MinimumLength]
    );
    assert!(descriptor
        .check_constraints(&N::Text("aa".into()))
        .unwrap()
        .is_empty());
    let descriptor = admit(
        V::Map(vec![("type", V::Text("str")), ("min_length", V::Int("2"))]),
        false,
    )
    .into_descriptor()
    .unwrap();
    assert!(descriptor
        .check_constraints(&N::Text("e\u{301}".into()))
        .unwrap()
        .is_empty());
    assert_eq!(
        descriptor.check_constraints(&N::Text("😀".into())).unwrap(),
        [C::MinimumLength]
    );
    let descriptor = admit(
        V::Map(vec![("type", V::Text("list")), ("size", V::Int("0"))]),
        false,
    )
    .into_descriptor()
    .unwrap();
    assert!(descriptor
        .check_constraints(&N::Sequence(vec![]))
        .unwrap()
        .is_empty());
    assert_eq!(
        descriptor.check_constraints(&N::Sequence(vec![0])).unwrap(),
        [C::InvalidSize]
    );
}

#[test]
fn fixed_regex_work_refusal_remains_distinct_from_constraint_mismatches() {
    let descriptor = admit(
        V::Map(vec![
            ("type", V::Text("str")),
            ("values", V::List(vec![V::Text("a")])),
        ]),
        false,
    )
    .into_descriptor()
    .unwrap();
    assert!(descriptor
        .check_constraints(&N::Text("a".into()))
        .unwrap()
        .is_empty());
    let descriptor = admit(
        V::Map(vec![("type", V::Text("str")), ("pattern", V::Text("a+"))]),
        false,
    )
    .into_descriptor()
    .unwrap();
    assert!(descriptor
        .check_constraints(&N::Text("a".into()))
        .unwrap()
        .is_empty());
    let long = N::Text("a".repeat(1_000_000));
    assert!(matches!(
        descriptor.check_constraints(&long),
        Err(ConstraintError::Regex(yamaa_core::regex::MatchError {
            resource: Resource::Work,
            limit: 1_000_000
        }))
    ));
    assert!(descriptor
        .check_constraints(&N::Text("a".into()))
        .unwrap()
        .is_empty());
}
