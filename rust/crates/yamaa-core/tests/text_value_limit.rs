//! The one bound on a text value a verb builds (#4103): `str_pad`,
//! `str_decimal`, `str_template`, `str_replace` and `STR_JOIN` each admit a
//! result of exactly `Limits::TEXT_BYTES` bytes and refuse one byte more.
//!
//! The seam is each verb's own building step, the call that sizes and then
//! allocates its result: a benchmark reaches the refusal
//! (`negative-str-pad-text-limit`), but the admitted edge would need a
//! mebibyte of expected text.
use yamaa_core::{
    aggregate_reduction::{reduce_values, AggregateReductionError, ReductionFields},
    expression::Reducer,
    limits::{LimitExceeded, Limits},
    table::ValueRef,
    text_decimal::{self, Step, StringDecimal},
    text_pad::StringPad,
    text_replace::{self, StringReplace},
    text_template::{MaterializeError, StringTemplate},
    value::Value,
};

/// Build a text of `bytes` bytes through one verb; its length, or the bound.
type Build = fn(usize) -> Result<usize, LimitExceeded>;

fn length(value: Value) -> usize {
    let Value::Str(text) = value else {
        panic!("a text verb returns text, not {value:?}")
    };
    text.len()
}

/// `x` padded to `bytes` characters, each one byte.
fn str_pad(bytes: usize) -> Result<usize, LimitExceeded> {
    StringPad::new("SRC.X", "columns.Y.derivation.str_pad", &bytes.to_string())
        .unwrap()
        .prepare_text(&Value::Str("x".into()))
        .unwrap()
        .render()
        .map(length)
}

/// `7`, with no decimals, padded to a `width` of `bytes`.
fn str_decimal_width(bytes: usize) -> Result<usize, LimitExceeded> {
    decimal("0", Some(&bytes.to_string()), 7)
}

/// `0` with `bytes - 2` decimals: `0.` and that many zeros.
fn str_decimal_decimals(bytes: usize) -> Result<usize, LimitExceeded> {
    decimal(&(bytes - 2).to_string(), None, 0)
}

fn decimal(decimals: &str, width: Option<&str>, value: i64) -> Result<usize, LimitExceeded> {
    let operation = StringDecimal::new(
        "SRC.X".into(),
        "columns.Y.derivation.str_decimal".into(),
        decimals,
        width,
        None,
    )
    .unwrap();
    match operation.apply(ValueRef::Int(value)) {
        Ok(Step::Value(value)) => Ok(length(value)),
        Err(text_decimal::Error::Limit(limit)) => Err(limit),
        other => panic!("a number writes text, not {other:?}"),
    }
}

/// `{A}-{B}`, with `bytes - 1` bytes of text split between `A` and `B`.
fn str_template(bytes: usize) -> Result<usize, LimitExceeded> {
    let half = "a".repeat((bytes - 1) / 2);
    let rest = "a".repeat(bytes - 1 - half.len());
    StringTemplate::new(
        "{A}-{B}".into(),
        "columns.Y.derivation.str_template".into(),
        None,
    )
    .unwrap()
    .materialize(&[Value::Str(half), Value::Str(rest)])
    .map(|text| text.len())
    .map_err(|error| match error {
        MaterializeError::Limit(limit) => limit,
        other => panic!("two texts fill a template, not {other:?}"),
    })
}

/// `x` replaced by `bytes` bytes of `y`.
fn str_replace(bytes: usize) -> Result<usize, LimitExceeded> {
    let operation = StringReplace::new(
        "SRC.X".into(),
        "columns.Y.derivation.str_replace".into(),
        "x".into(),
        "y".repeat(bytes),
        None,
    )
    .unwrap();
    let Ok(text_replace::Step::Matched { subject, spans }) = operation.apply(ValueRef::Str("x"))
    else {
        panic!("the pattern matches its subject")
    };
    operation
        .render(subject, &spans)
        .map(|text| text.len())
        .map_err(|error| match error {
            text_replace::RenderError::Limit(limit) => limit,
            other => panic!("a replacement is sized, not {other:?}"),
        })
}

/// `bytes - 3` bytes of `a` and the int `7`, joined by `", "`.
fn str_join(bytes: usize) -> Result<usize, LimitExceeded> {
    let text = "a".repeat(bytes - 3);
    let fields = ReductionFields {
        separator: ", ",
        distinct: false,
        quantile: 0.0,
    };
    let values = [ValueRef::Str(&text), ValueRef::Missing, ValueRef::Int(7)];
    match reduce_values(&values, Reducer::StrJoin, fields, "STR_JOIN(D.V)") {
        Ok(value) => Ok(length(value)),
        Err(AggregateReductionError::Limit(limit)) => Err(limit),
        Err(other) => panic!("two present values join, not {other:?}"),
    }
}

#[test]
fn every_growing_text_verb_admits_the_text_limit_and_refuses_one_byte_more() {
    // Whether the refusal reports the length: the decimal writing refuses
    // its decimals without counting the text it would write.
    let verbs: [(&str, Build, bool); 6] = [
        ("str_pad", str_pad, true),
        ("str_decimal width", str_decimal_width, true),
        ("str_decimal decimals", str_decimal_decimals, false),
        ("str_template", str_template, true),
        ("str_replace", str_replace, true),
        ("STR_JOIN", str_join, true),
    ];
    for (label, build, counted) in verbs {
        assert_eq!(build(Limits::TEXT_BYTES), Ok(1_048_576), "{label} at");
        assert_eq!(
            build(Limits::TEXT_BYTES + 1),
            Err(LimitExceeded {
                resource: "text_bytes",
                limit: 1_048_576,
                actual: counted.then_some(1_048_577),
            }),
            "{label} above"
        );
    }
}
