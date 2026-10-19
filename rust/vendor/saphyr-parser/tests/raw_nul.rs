use yamaa_saphyr_parser::{Event, Parser, ScalarStyle, ScanError};

/// Upstream's `run_parser`: parse through both `StrInput` and `BufferedInput`,
/// require the same events, spans and error from each, and return the events.
fn run_parser(input: &str) -> Result<Vec<Event<'_>>, ScanError> {
    let string = Parser::new_from_str(input).collect::<Result<Vec<_>, _>>();
    let iterator = Parser::new_from_iter(input.chars()).collect::<Result<Vec<_>, _>>();
    assert_eq!(
        string, iterator,
        "string/iterator events and spans for {input:?}"
    );
    string.map(|events| events.into_iter().map(|(event, _)| event).collect())
}

#[test]
fn test_raw_nul_truncates_document() {
    for s in [
        "key: a\0b\n",
        "key: a\0b\nother: c\n",
        "k\0ey: a\n",
        "# note\0\nkey: a\n",
    ] {
        assert!(run_parser(s).is_err(), "expected an error for {s:?}");
    }
}

#[test]
fn test_escaped_nul_is_preserved() {
    let events = run_parser(r#"key: "a\0b""#).unwrap();
    assert!(events.contains(&Event::Scalar(
        "a\0b".into(),
        ScalarStyle::DoubleQuoted,
        0,
        None
    )));
}
