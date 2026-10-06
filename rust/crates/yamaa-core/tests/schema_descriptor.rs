use yamaa_core::regex::{CompileError, MatchLimits, Resource};
use yamaa_core::schema::{ConstraintBudget, ConstraintError, ConstraintViolation as C};
use yamaa_core::schema::{
    Descriptor, DescriptorBudget, DescriptorError, DescriptorIssue as I, DescriptorLimits,
    DescriptorResource, DescriptorUsage, Document, DocumentLimits, DocumentNode as N,
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
    Document::new(nodes, root, DocumentLimits::default()).unwrap()
}

fn admit(value: V, class_field: bool) -> yamaa_core::schema::DescriptorReport {
    let d = document(value);
    Descriptor::admit(
        &d,
        d.root(),
        class_field,
        &mut DescriptorBudget::new(DescriptorLimits::default()),
    )
    .unwrap()
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
    let report = Descriptor::admit(
        &d,
        d.root(),
        true,
        &mut DescriptorBudget::new(DescriptorLimits::default()),
    )
    .unwrap();
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
        assert_eq!(
            pattern
                .full_match(value, MatchLimits::default())
                .unwrap()
                .is_some(),
            expected
        );
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
    let d = document(V::Map(vec![
        ("type", V::Text("str")),
        ("pattern", V::Text("ab")),
    ]));
    let limits = DescriptorLimits {
        regex: yamaa_core::regex::CompileLimits {
            bytes: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(
        Descriptor::admit(&d, d.root(), false, &mut DescriptorBudget::new(limits)).unwrap_err(),
        DescriptorError::Regex(CompileError::Limit {
            resource: Resource::PatternBytes,
            limit: 1
        })
    );
}

#[test]
fn cumulative_descriptor_charges_survive_failed_admission_and_fresh_attempts_reset() {
    let d = document(V::Map(vec![
        ("type", V::Text("str")),
        ("pattern", V::Text("[")),
    ]));
    for (limits, resource, limit, prefix) in [
        (
            DescriptorLimits {
                descriptors: 1,
                ..Default::default()
            },
            DescriptorResource::Descriptors,
            1,
            DescriptorUsage {
                descriptors: 1,
                input_nodes: 5,
                input_text_bytes: 15,
                type_bytes: 3,
                pattern_bytes: 1,
            },
        ),
        (
            DescriptorLimits {
                type_bytes: 5,
                ..Default::default()
            },
            DescriptorResource::TypeBytes,
            5,
            DescriptorUsage {
                descriptors: 2,
                input_nodes: 10,
                input_text_bytes: 30,
                type_bytes: 3,
                pattern_bytes: 1,
            },
        ),
        (
            DescriptorLimits {
                pattern_bytes: 1,
                ..Default::default()
            },
            DescriptorResource::PatternBytes,
            1,
            DescriptorUsage {
                descriptors: 2,
                input_nodes: 10,
                input_text_bytes: 30,
                type_bytes: 6,
                pattern_bytes: 1,
            },
        ),
    ] {
        let mut budget = DescriptorBudget::new(limits);
        assert!(matches!(
            Descriptor::admit(&d, d.root(), false, &mut budget)
                .unwrap()
                .issues(),
            [I::InvalidPattern { .. }]
        ));
        assert_eq!(
            Descriptor::admit(&d, d.root(), false, &mut budget).unwrap_err(),
            DescriptorError::Limit { resource, limit }
        );
        assert_eq!(budget.used(), prefix);
        assert!(Descriptor::admit(&d, d.root(), false, &mut DescriptorBudget::new(limits)).is_ok());
    }
}

#[test]
fn individually_legal_patterns_cannot_restart_shared_width_analysis() {
    let pattern = format!("(a){}(?<=\\1)", "(a|aa)".repeat(8));
    let d = document(V::Map(vec![
        ("type", V::Text("str")),
        ("pattern", V::Owned(pattern)),
    ]));
    let mut budget = DescriptorBudget::new(DescriptorLimits::default());
    let mut refusal = None;
    for _ in 0..512 {
        match Descriptor::admit(&d, d.root(), false, &mut budget) {
            Ok(report) => assert!(report.issues().is_empty()),
            Err(error) => {
                refusal = Some(error);
                break;
            }
        }
    }
    assert!(matches!(
        refusal,
        Some(DescriptorError::Regex(CompileError::Limit {
            resource: Resource::WidthWork | Resource::WidthCells,
            ..
        }))
    ));
    assert!(Descriptor::admit(
        &d,
        d.root(),
        false,
        &mut DescriptorBudget::new(DescriptorLimits::default())
    )
    .unwrap()
    .issues()
    .is_empty());
}

#[test]
fn repeated_metadata_is_bounded_even_when_strings_are_empty_or_not_retained() {
    let d = document(V::Map(vec![
        ("type", V::Text("str")),
        (
            "values",
            V::List(vec![V::Text(""), V::Text(""), V::Text("")]),
        ),
    ]));
    let limits = DescriptorLimits {
        input_nodes: 15,
        ..Default::default()
    };
    let mut budget = DescriptorBudget::new(limits);
    assert!(Descriptor::admit(&d, d.root(), false, &mut budget)
        .unwrap()
        .issues()
        .is_empty());
    assert_eq!(budget.used().input_nodes, 8);
    assert_eq!(
        Descriptor::admit(&d, d.root(), false, &mut budget).unwrap_err(),
        DescriptorError::Limit {
            resource: DescriptorResource::InputNodes,
            limit: 15
        }
    );
    assert!(Descriptor::admit(&d, d.root(), false, &mut DescriptorBudget::new(limits)).is_ok());

    let d = document(V::Map(vec![
        ("type", V::Text("str")),
        ("description", V::Text("    ")),
    ]));
    let mut budget = DescriptorBudget::new(DescriptorLimits {
        input_text_bytes: 43,
        ..Default::default()
    });
    assert_eq!(
        Descriptor::admit(&d, d.root(), false, &mut budget)
            .unwrap()
            .issues(),
        [I::Description]
    );
    assert_eq!(budget.used().input_text_bytes, 22);
    assert_eq!(
        Descriptor::admit(&d, d.root(), false, &mut budget).unwrap_err(),
        DescriptorError::Limit {
            resource: DescriptorResource::InputTextBytes,
            limit: 43
        }
    );
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
    let mut budget = ConstraintBudget::new(1_000, MatchLimits::default());
    assert_eq!(
        descriptor
            .check_constraints(&N::Text("é".into()), MatchLimits::default(), &mut budget)
            .unwrap(),
        [C::ValueNotPermitted, C::PatternMismatch, C::MinimumLength]
    );
    assert!(descriptor
        .check_constraints(&N::Text("aa".into()), MatchLimits::default(), &mut budget)
        .unwrap()
        .is_empty());
    let descriptor = admit(
        V::Map(vec![("type", V::Text("str")), ("min_length", V::Int("2"))]),
        false,
    )
    .into_descriptor()
    .unwrap();
    assert!(descriptor
        .check_constraints(
            &N::Text("e\u{301}".into()),
            MatchLimits::default(),
            &mut budget
        )
        .unwrap()
        .is_empty());
    assert_eq!(
        descriptor
            .check_constraints(&N::Text("😀".into()), MatchLimits::default(), &mut budget)
            .unwrap(),
        [C::MinimumLength]
    );
    let descriptor = admit(
        V::Map(vec![("type", V::Text("list")), ("size", V::Int("0"))]),
        false,
    )
    .into_descriptor()
    .unwrap();
    assert!(descriptor
        .check_constraints(&N::Sequence(vec![]), MatchLimits::default(), &mut budget)
        .unwrap()
        .is_empty());
    assert_eq!(
        descriptor
            .check_constraints(&N::Sequence(vec![0]), MatchLimits::default(), &mut budget)
            .unwrap(),
        [C::InvalidSize]
    );
}

#[test]
fn constraint_work_and_regex_limits_remain_distinct_from_mismatches() {
    let descriptor = admit(
        V::Map(vec![
            ("type", V::Text("str")),
            ("values", V::List(vec![V::Text("a")])),
        ]),
        false,
    )
    .into_descriptor()
    .unwrap();
    let value = N::Text("a".into());
    assert_eq!(
        descriptor.check_constraints(
            &value,
            MatchLimits::default(),
            &mut ConstraintBudget::new(3, MatchLimits::default())
        ),
        Err(ConstraintError::Work { limit: 3 })
    );
    assert!(descriptor
        .check_constraints(
            &value,
            MatchLimits::default(),
            &mut ConstraintBudget::new(4, MatchLimits::default())
        )
        .unwrap()
        .is_empty());
    let descriptor = admit(
        V::Map(vec![("type", V::Text("str")), ("pattern", V::Text("a+"))]),
        false,
    )
    .into_descriptor()
    .unwrap();
    let mut baseline = ConstraintBudget::new(100, MatchLimits::default());
    descriptor
        .check_constraints(&value, MatchLimits::default(), &mut baseline)
        .unwrap();
    let limits = MatchLimits {
        work: baseline.used().regex.work * 2 - 1,
        ..Default::default()
    };
    let mut budget = ConstraintBudget::new(100, limits);
    assert!(descriptor
        .check_constraints(&value, MatchLimits::default(), &mut budget)
        .unwrap()
        .is_empty());
    let prefix = budget.used();
    assert!(matches!(
        descriptor.check_constraints(&value, MatchLimits::default(), &mut budget),
        Err(ConstraintError::Regex(yamaa_core::regex::MatchError {
            resource: Resource::Work,
            ..
        }))
    ));
    assert!(budget.used().regex.work >= prefix.regex.work);
    assert!(descriptor
        .check_constraints(
            &value,
            MatchLimits::default(),
            &mut ConstraintBudget::new(100, limits)
        )
        .unwrap()
        .is_empty());
}
