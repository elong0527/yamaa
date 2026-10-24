//! The text verbs' entry points (TEXT-1 to TEXT-10).
use super::{text, value, File};
use crate::support::diagnostic::json::Json;
use yamaa_core::{
    diagnostic::Diagnostic,
    table::ValueRef,
    text_case::{Case, Error as CaseError, StringCase},
    text_extract::{Error as ExtractError, Step, StringExtract},
    text_pad::StringPad,
    text_template::StringTemplate,
};

pub(super) const FILES: &[File] = &[fixture!("text.json")];

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    Some(match operation {
        "str_case" => str_case(input),
        "str_extract" => str_extract(input),
        "str_extract_apply" => str_extract_apply(input),
        "str_extract_unknown" => str_extract_unknown(input),
        "str_extract_unmatched" => str_extract_unmatched(input),
        "str_pad" => str_pad(input),
        "str_template" => str_template(input),
        "str_template_missing" => str_template_missing(input),
        _ => return None,
    })
}

/// `str_pad`'s declaration, `{source, path, width}`: its refusal at `path`.
fn str_pad(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["source", "path", "width"]);
    let path = text(input, "path");
    match StringPad::new(text(input, "source"), path, text(input, "width")) {
        Ok(_) => Vec::new(),
        Err(refusal) => refusal.diagnostic(path).into_iter().collect(),
    }
}

/// `str_case` in `case` applied to `value`, `{source, path, case, value}`.
fn str_case(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["source", "path", "case", "value"]);
    let case = match text(input, "case") {
        "upper" => Case::Upper,
        "lower" => Case::Lower,
        "sentence" => Case::Sentence,
        "title" => Case::Title,
        other => panic!("no case {other:?}"),
    };
    let operation = StringCase::new(
        text(input, "source").into(),
        text(input, "path").into(),
        case,
        None,
    )
    .expect("an admitted declaration");
    let value = value(input.field("value"));
    match operation.apply(ValueRef::from(&value)) {
        Ok(_) => Vec::new(),
        Err(CaseError::Diagnostic(diagnostic)) => vec![diagnostic],
    }
}

/// `str_template`'s declaration, `{template, path}`.
fn str_template(input: &Json) -> Vec<Diagnostic> {
    template(input.closed(&["template", "path"]))
        .declaration_diagnostic()
        .cloned()
        .into_iter()
        .collect()
}

/// The failure of `str_template` when its `part`-th placeholder reads a
/// missing value, `{template, path, part}`.
fn str_template_missing(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["template", "path", "part"]);
    template(input)
        .missing_diagnostic(input.field("part").integer())
        .into_iter()
        .collect()
}

fn template(input: &Json) -> StringTemplate {
    StringTemplate::new(
        text(input, "template").into(),
        text(input, "path").into(),
        None,
    )
    .expect("an admitted declaration")
}

/// The members every `str_extract` operation reads.
const EXTRACT: [&str; 4] = ["source", "path", "pattern", "group"];

/// `str_extract`'s declaration, `{source, path, pattern, group}`.
fn str_extract(input: &Json) -> Vec<Diagnostic> {
    extract(input.closed(&EXTRACT))
        .declaration_diagnostic()
        .cloned()
        .into_iter()
        .collect()
}

/// `str_extract` applied to `value`.
fn str_extract_apply(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&[EXTRACT.as_slice(), &["value"]].concat());
    let value = value(input.field("value"));
    match extract(input).apply(ValueRef::from(&value)) {
        Ok(_) => Vec::new(),
        Err(ExtractError::Diagnostic(diagnostic)) => vec![diagnostic],
    }
}

/// The failure of `str_extract` when its pattern matches nowhere in `value`.
fn str_extract_unmatched(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&[EXTRACT.as_slice(), &["value"]].concat());
    extract(input)
        .failure_diagnostic(Step::NoMatch(text(input, "value")))
        .into_iter()
        .collect()
}

/// The failure of `str_extract` when its source names no field.
fn str_extract_unknown(input: &Json) -> Vec<Diagnostic> {
    vec![extract(input.closed(&EXTRACT)).unknown_diagnostic()]
}

fn extract(input: &Json) -> StringExtract {
    StringExtract::new(
        text(input, "source").into(),
        text(input, "path").into(),
        text(input, "pattern").into(),
        text(input, "group").into(),
        None,
        None,
    )
    .expect("an admitted declaration")
}
