//! The 1.x header causes (SCHEMA-2, LAYER-4, P-VER-1), each raised by its
//! real detector and pinned to its phase, condition, rule, path and context.
use super::rendered;
use crate::support::document::Tree as T;
use std::collections::BTreeSet;
use yamaa_core::diagnostic::{ConditionCode, Diagnostic, InheritanceFinding as F};
use yamaa_core::json::Value as JsonValue;
use yamaa_core::language::{
    read_header, FileType, LanguageVersion, PathBase, Reference, ReferenceFault,
    LANGUAGE_VERSION,
};
use yamaa_core::schema::{
    DocumentNode as N, SchemaContext as C, SchemaDiagnostic, SchemaModule, SchemaStructure,
};

fn mapping(condition: &str, rule: &str) -> (&'static str, String, Option<String>) {
    ("validation", condition.into(), Some(rule.into()))
}

fn defined(code: ConditionCode) -> (&'static str, String, Option<String>) {
    let definition = code.definition();
    (
        definition.phase,
        definition.condition.into(),
        definition.rule.map(String::from),
    )
}

fn schema_finding(
    reached: &mut BTreeSet<ConditionCode>,
    finding: &SchemaDiagnostic,
    path: &str,
    condition: &str,
    rule: &str,
) {
    assert_eq!(defined(finding.code), mapping(condition, rule));
    assert_eq!(finding.path, path);
    reached.insert(rendered(&finding));
}

fn field_diagnostic(
    reached: &mut BTreeSet<ConditionCode>,
    finding: &Diagnostic,
    path: &str,
    condition: &str,
    rule: &str,
    context: &[(&str, JsonValue)],
) {
    assert_eq!(defined(finding.code), mapping(condition, rule));
    assert_eq!(finding.spec_paths, [path]);
    let expected = context
        .iter()
        .map(|(key, value)| ((*key).to_string(), value.clone()))
        .collect();
    assert_eq!(finding.context, expected);
    assert!(finding.source_span.is_none());
    assert!(finding.operand_route.is_none());
    reached.insert(rendered(&finding));
}

fn text(value: &str) -> JsonValue {
    JsonValue::String(value.into())
}

fn texts(values: &[&str]) -> JsonValue {
    JsonValue::Array(values.iter().map(|value| text(value)).collect())
}

fn header(version: &str, file_type: &str) -> T {
    T::map(vec![
        ("yamaa", T::text(version)),
        ("type", T::text(file_type)),
    ])
}

pub fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();

    // The file a call reads: at its own `yamaa` and `type` (SCHEMA-2). The
    // engine reads 1.0 (#3030 D76), so a newer version is refused.
    let refused = header("1.1", "spec").document();
    let finding = read_header(&refused, Some(LANGUAGE_VERSION), FileType::Spec)
        .unwrap_err()
        .finding();
    schema_finding(
        &mut reached,
        &finding,
        "yamaa",
        "unsupported_language_version",
        "SCHEMA-2",
    );
    let version = refused.field(refused.root(), "yamaa").unwrap();
    assert_eq!(
        finding.context,
        [
            ("version", C::InputValue(version)),
            ("language_versions", C::TextList(vec!["1.0".into()])),
        ]
    );
    let project = header("1.0", "project").document();
    let finding = read_header(&project, Some(LANGUAGE_VERSION), FileType::Spec)
        .unwrap_err()
        .finding();
    schema_finding(
        &mut reached,
        &finding,
        "type",
        "unexpected_file_type",
        "SCHEMA-2",
    );
    let kind = project.field(project.root(), "type").unwrap();
    assert_eq!(
        finding.context,
        [
            ("type", C::InputValue(kind)),
            ("expected", C::Text("spec".into())),
        ]
    );

    // The file a referencing field names: at that field (SCHEMA-2).
    let reference = Reference {
        field: "functions[0]".into(),
        written: "functions/bmi.yaml".into(),
        expected: FileType::Function,
        base: PathBase::Declaring,
    };
    let Err(ReferenceFault::Field(finding)) = reference.check(
        "study/bmi.yaml",
        &header("1.2", "function").document(),
        LANGUAGE_VERSION,
    ) else {
        panic!("a newer function file");
    };
    field_diagnostic(
        &mut reached,
        &finding,
        "functions[0]",
        "unsupported_language_version",
        "SCHEMA-2",
        &[
            ("source", text("study/bmi.yaml")),
            ("version", text("1.2")),
            ("language_versions", texts(&["1.0"])),
        ],
    );
    // A reader that decoded the named file before it knew the field, as a
    // project's preparation decodes each file it lists, keeps the fault and
    // reports it at the field as the check does.
    let newer = header("1.2", "function").document();
    let kept = read_header(&newer, Some(LANGUAGE_VERSION), FileType::Function).unwrap_err();
    assert_eq!(
        reference.refuse("study/bmi.yaml", &kept),
        reference
            .check("study/bmi.yaml", &newer, LANGUAGE_VERSION)
            .unwrap_err()
    );
    // A malformed header stays a finding in the named file.
    let headerless = T::map(vec![("name", T::text("bmi"))]).document();
    let kept = read_header(&headerless, Some(LANGUAGE_VERSION), FileType::Function).unwrap_err();
    let ReferenceFault::Finding(finding) = reference.refuse("study/bmi.yaml", &kept) else {
        panic!("a finding in the named file");
    };
    assert_eq!(
        (finding.path.as_str(), finding.code),
        ("yamaa", ConditionCode::SchemaMissingRequiredField)
    );
    let Err(ReferenceFault::Field(finding)) = reference.check(
        "study/bmi.yaml",
        &header("1.0", "spec").document(),
        LANGUAGE_VERSION,
    ) else {
        panic!("a specification where a function file is expected");
    };
    field_diagnostic(
        &mut reached,
        &finding,
        "functions[0]",
        "unexpected_file_type",
        "SCHEMA-2",
        &[
            ("source", text("study/bmi.yaml")),
            ("type", text("spec")),
            ("expected", text("function")),
        ],
    );

    // A parent layer: at the child's `parent` (LAYER-4).
    let reads = vec!["1.0".to_string()];
    let finding = F::LanguageVersion {
        source: "base",
        entry: "entry",
        version: "1.3",
        reads: &reads,
        is_entry: false,
    }
    .diagnostic()
    .unwrap();
    field_diagnostic(
        &mut reached,
        &finding,
        "parent",
        "unsupported_language_version",
        "LAYER-4",
        &[
            ("source", text("base")),
            ("entry", text("entry")),
            ("version", text("1.3")),
            ("language_versions", texts(&["1.0"])),
        ],
    );
    let finding = F::FileType {
        source: "base",
        entry: "entry",
        file_type: "project",
        is_entry: false,
    }
    .diagnostic()
    .unwrap();
    field_diagnostic(
        &mut reached,
        &finding,
        "parent",
        "unexpected_file_type",
        "LAYER-4",
        &[
            ("source", text("base")),
            ("entry", text("entry")),
            ("type", text("project")),
            ("expected", text("spec")),
        ],
    );
    // The entry of a chain keeps its own header's paths and the call's rule.
    for (finding, path, condition) in [
        (
            F::LanguageVersion {
                source: "entry",
                entry: "entry",
                version: "1.3",
                reads: &reads,
                is_entry: true,
            },
            "yamaa",
            "unsupported_language_version",
        ),
        (
            F::FileType {
                source: "entry",
                entry: "entry",
                file_type: "project",
                is_entry: true,
            },
            "type",
            "unexpected_file_type",
        ),
    ] {
        let finding = finding.diagnostic().unwrap();
        assert_eq!(defined(finding.code), mapping(condition, "SCHEMA-2"));
        assert_eq!(finding.spec_paths, [path]);
        assert_eq!(finding.context["source"], text("entry"));
    }

    // A field newer than the version a document declares (SCHEMA-2, P-VER-1).
    let field = |name: &str, since: Option<&str>| {
        let mut descriptor = vec![("type", T::text("str"))];
        if let Some(since) = since {
            descriptor.push(("since", T::text(since)));
        }
        T::map(vec![(name, T::map(descriptor))])
    };
    let schema = SchemaStructure::admit(
        vec![SchemaModule {
            name: "schema_spec.yaml".into(),
            document: T::map(vec![
                ("version", T::text("1.0")),
                (
                    "root_class",
                    T::List(vec![field("kept", None), field("added", Some("1.1"))]),
                ),
            ])
            .document(),
        }],
        0,
        "root_class",
    )
    .unwrap();
    let written = T::map(vec![("kept", T::text("a")), ("added", T::text("b"))]).document();
    let types = schema.parse_query_types(&["root_class".into()]).unwrap();
    let older = schema.at_version(LanguageVersion::new(0));
    let findings = older
        .validate_types(&types, &written, written.root(), "", false)
        .unwrap();
    assert_eq!(findings.len(), 1);
    schema_finding(
        &mut reached,
        &findings[0],
        "added",
        "unknown_field",
        "SCHEMA-2",
    );
    assert_eq!(
        findings[0].context,
        [
            ("field", C::Text("added".into())),
            ("class", C::Text("root_class".into())),
            ("since", C::Text("1.1".into())),
        ]
    );
    let current = schema.at_version(LanguageVersion::new(1));
    assert!(current
        .validate_types(&types, &written, written.root(), "", false)
        .unwrap()
        .is_empty());
    // A field no version declares keeps the plain cause.
    let unknown = T::map(vec![("other", T::Scalar(N::Null))]).document();
    assert_eq!(
        older
            .validate_types(&types, &unknown, unknown.root(), "", false)
            .unwrap()[0]
            .code,
        ConditionCode::SchemaUnknownField
    );
    reached
}
