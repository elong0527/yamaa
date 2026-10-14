use std::collections::BTreeSet;
use yamaa_core::{
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
    for (failure, condition, requirement, path) in [
        (
            Failure::NonAscii { line: 1, column: 4 },
            "non_ascii_source",
            "REQ-0705",
            "$",
        ),
        (
            Failure::InvalidYaml {
                line: 2,
                column: 9,
                reason: "unterminated sequence",
            },
            "invalid_yaml",
            "REQ-0280",
            "$",
        ),
        (
            Failure::InvalidText(&issues),
            "invalid_text",
            "REQ-0029",
            "$.x",
        ),
    ] {
        let result = diagnostics(failure, Some("source.yaml"));
        assert_eq!(result.len(), 1);
        let diagnostic = &result[0];
        let definition = diagnostic.code.definition();
        assert_eq!(
            (
                definition.phase,
                definition.condition,
                definition.requirement
            ),
            ("validation", condition, Some(requirement))
        );
        assert_eq!(diagnostic.spec_path, path);
        if diagnostic.code == ConditionCode::SourceNonAscii {
            assert_eq!(diagnostic.context["path"], Fact::Text("source.yaml"));
            assert_eq!(diagnostic.context["line"], Fact::Unsigned(1));
            assert_eq!(diagnostic.context["column"], Fact::Unsigned(4));
        } else {
            assert!(!diagnostic.context.contains_key("path"));
        }
        reached.insert(diagnostic.code);
    }
    reached
}
#[test]
fn reason_and_identity_borrow_original_facts_and_unicode_order_is_complete() {
    let reason = String::from("retained parser reason");
    let identity = String::from("source.yaml");
    let result = diagnostics(
        Failure::InvalidYaml {
            line: 1,
            column: 2,
            reason: &reason,
        },
        Some(&identity),
    );
    let Fact::Text(held) = result[0].context["reason"] else {
        panic!("borrowed reason")
    };
    assert!(std::ptr::eq(held.as_ptr(), reason.as_ptr()));
    let result = diagnostics(Failure::NonAscii { line: 1, column: 2 }, Some(&identity));
    let Fact::Text(held) = result[0].context["path"] else {
        panic!("borrowed source identity")
    };
    assert!(std::ptr::eq(held.as_ptr(), identity.as_ptr()));
    let result = diagnostics(Failure::NonAscii { line: 1, column: 2 }, None);
    assert_eq!(result[0].context.len(), 2);
    let issues = [
        UnicodeIssue {
            path: "$.first".into(),
            code_point: 0xD800,
            offset: 0,
        },
        UnicodeIssue {
            path: "$.second[2]".into(),
            code_point: 0xDFFF,
            offset: usize::MAX,
        },
    ];
    let result = diagnostics(Failure::InvalidText(&issues), Some(&identity));
    assert_eq!(
        result
            .iter()
            .map(|d| d.spec_path.as_str())
            .collect::<Vec<_>>(),
        ["$.first", "$.second[2]"]
    );
    assert_eq!(
        result[0].context["code_point"],
        Fact::OwnedText("U+D800".into())
    );
    assert_eq!(result[1].context["offset"], Fact::Unsigned(usize::MAX));
}
