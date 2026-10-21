//! `yamaa::schema` gives the JSON Schema of a file type as canonical JSON
//! text: the document committed in `yaml/v1/json/`, which the engine embeds
//! when it is built (#3030 section 2.12, D149). Any other text is refused as
//! `unexpected_file_type` (SCHEMA-2), as `cannot give the schema of <type>`
//! (D39). No build feature is needed: the schema is text the engine holds.

use std::path::Path;

use yamaa::{ContextValue, Severity, FILE_TYPES};

/// The committed JSON Schema of `file_type`, as canonical JSON text.
fn committed(file_type: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../yaml/v1/json")
        .join(format!("{file_type}.schema.json"));
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    yamaa_adapters::json_wire::encode(&yamaa_adapters::json_wire::decode_slice(&bytes).unwrap())
}

#[test]
fn each_file_type_gives_its_committed_json_schema_as_canonical_text() {
    assert_eq!(FILE_TYPES, ["spec", "project", "codelists", "function"]);
    for file_type in FILE_TYPES {
        let text = yamaa::schema(file_type).unwrap();
        assert_eq!(text, committed(file_type), "{file_type}");
        assert!(text.is_ascii() && !text.contains('\n'), "{file_type}");
        assert_eq!(yamaa::schema(file_type).unwrap(), text, "{file_type}");
    }
}

/// Text that is not one of the four types, exactly, is refused with one issue
/// that names no spec path or location, and the refusal wrote nothing.
#[test]
fn any_other_text_is_refused_as_an_unexpected_file_type() {
    let expected = ContextValue::List(
        FILE_TYPES
            .iter()
            .map(|name| ContextValue::Text((*name).to_owned()))
            .collect(),
    );
    for text in [
        "specs",
        "Spec",
        " spec",
        "spec\n",
        "",
        "environment",
        "codelist",
    ] {
        let failure = yamaa::schema(text).unwrap_err();
        let [issue] = failure.diagnostics.as_slice() else {
            panic!("one issue for {text:?}");
        };
        assert_eq!(issue.severity, Severity::Error);
        assert_eq!(
            (
                issue.phase.as_str(),
                issue.code.as_str(),
                issue.rule.as_deref()
            ),
            ("validation", "unexpected_file_type", Some("SCHEMA-2")),
            "{text:?}"
        );
        assert!(issue.spec_path.is_empty() && issue.location.is_none());
        assert_eq!(
            issue.context.get("type"),
            Some(&ContextValue::Text(text.to_owned()))
        );
        assert_eq!(issue.context.get("expected"), Some(&expected));
        assert_eq!(issue.context.len(), 2);
        assert_eq!(
            issue.message,
            format!(
                "a {text} file is given where a spec, project, codelists, function file is \
                 expected"
            )
        );
        assert_eq!(failure.issues.rows(), 1);
        assert!(failure.paths.is_empty());
    }
    assert_eq!(
        yamaa::schema("specs").unwrap_err().message,
        "cannot give the schema of specs: a specs file is given where a spec, project, \
         codelists, function file is expected [unexpected_file_type, SCHEMA-2]"
    );
    // The error text is printable ASCII, whatever the text given.
    assert_eq!(
        yamaa::schema("spec\n").unwrap_err().message,
        "cannot give the schema of spec\\n: a spec\\n file is given where a spec, project, \
         codelists, function file is expected [unexpected_file_type, SCHEMA-2]"
    );
}
