#[path = "../examples/support/string_template_syntax.rs"]
mod fixture;
use serde_json::json;

#[test]
fn typed_occurrences_preserve_escaped_text_spans_and_repeated_bindings() {
    let observed = fixture::observe("é{{{A}}}:{A}/{B}");
    assert_eq!(observed["outcome"]["identifiers"], json!(["A", "B"]));
    assert_eq!(
        observed["outcome"]["parts"],
        json!([
            {"kind":"text","value":"é{","span":[0,3]},
            {"kind":"placeholder","name":"A","span":[4,5]},
            {"kind":"text","value":"}:","span":[6,9]},
            {"kind":"placeholder","name":"A","span":[10,11]},
            {"kind":"text","value":"/","span":[12,13]},
            {"kind":"placeholder","name":"B","span":[14,15]}
        ])
    );
    assert_eq!(fixture::observe("")["outcome"]["parts"], json!([]));
}

#[test]
fn typed_refusals_preserve_grammar_reason_placeholder_and_unicode_position() {
    let observed = fixture::observe("é{A + B}");
    assert_eq!(
        observed["outcome"],
        json!({
            "status":"invalid", "condition":"invalid_string_template",
            "requirement":"REQ-0461",
            "context":{"reason":"invalid_placeholder","placeholder":"A + B"},
            "position":{"byte":2,"character":1},
            "span":{"byte_start":2,"byte_end":9,"character_start":1,"character_end":8}
        })
    );
}
