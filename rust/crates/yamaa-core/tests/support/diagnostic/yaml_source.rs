#![allow(unused_imports)]
use super::rendered;
pub(crate) use std::collections::BTreeSet;
pub(crate) use yamaa_core::{
    diagnostic::ConditionCode,
    project_diagnostics::Fact,
    yaml_diagnostics::{diagnostics, Failure, UnicodeIssue},
};
pub fn reached() -> BTreeSet<ConditionCode> {
    let issues = [UnicodeIssue {
        path: "$.x".into(),
        code_point: 0xD800,
        offset: 1,
    }];
    let mut reached = BTreeSet::new();
    for (failure, condition, rule, path) in [
        (
            Failure::NonAscii { line: 1, column: 4 },
            "non_ascii_source",
            "SPEC-8",
            "$",
        ),
        (
            Failure::InvalidYaml {
                line: 2,
                column: 9,
                reason: "unterminated sequence",
            },
            "invalid_yaml",
            "SCHEMA-3",
            "$",
        ),
        (
            Failure::InvalidText(&issues),
            "invalid_text",
            "VALUE-11",
            "$.x",
        ),
    ] {
        let result = diagnostics(failure, Some("source.yaml"));
        assert_eq!(result.len(), 1);
        let diagnostic = &result[0];
        let definition = diagnostic.code.definition();
        assert_eq!(
            (definition.phase, definition.condition, definition.rule),
            ("validation", condition, Some(rule))
        );
        assert_eq!(diagnostic.spec_path, path);
        if diagnostic.code == ConditionCode::SourceNonAscii {
            assert_eq!(diagnostic.context["path"], Fact::Text("source.yaml"));
            assert_eq!(diagnostic.context["line"], Fact::Unsigned(1));
            assert_eq!(diagnostic.context["column"], Fact::Unsigned(4));
        } else {
            assert!(!diagnostic.context.contains_key("path"));
        }
        reached.insert(rendered(&diagnostic));
    }
    reached
}
