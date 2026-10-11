//! Closed dictionary and interval selection, preserving raw literal results.
use crate::{
    diagnostic::{ConditionCode as C, ContextValue as V, Diagnostic},
    table::ValueRef,
    value::{compare_present_refs, FiniteFloat, Value},
};
use alloc::{collections::BTreeMap, format, string::String, vec, vec::Vec};
use core::cmp::Ordering;

pub const MAX_DECLARATION_BYTES: usize = 16_777_216;
#[derive(Clone, Debug, PartialEq)]
pub enum AdmissionError {
    Limit,
    Diagnostic(Diagnostic),
}
/// A selected result remains borrowed until the engine admits its copy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SelectedValue<'a> {
    Literal(&'a Value),
    Label(&'a str),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selected<'a> {
    pub value: SelectedValue<'a>,
    pub handler: Option<&'static str>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Mapping {
    source: String,
    path: String,
    entries: Vec<(String, Value)>,
    case_sensitive: bool,
    missing: Option<Value>,
    unmapped: Option<Value>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Cut {
    source: String,
    path: String,
    breaks: Vec<f64>,
    labels: Vec<String>,
    right: bool,
    missing: Option<Value>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum MappingCut {
    Mapping(Mapping),
    Cut(Cut),
}
fn text_bytes(value: &Value) -> usize {
    match value {
        Value::Str(text) => text.len(),
        _ => 0,
    }
}
fn metadata(
    source: &str,
    path: &str,
    mut extra: impl Iterator<Item = usize>,
) -> Result<(), AdmissionError> {
    if source.is_empty() || path.is_empty() {
        return Err(AdmissionError::Limit);
    }
    let total = extra.try_fold(source.len().saturating_add(path.len()), |total, n| {
        total.checked_add(n)
    });
    if total
        .and_then(|n| n.checked_mul(2))
        .and_then(|n| n.checked_add(1024))
        .is_none_or(|n| n > MAX_DECLARATION_BYTES)
    {
        return Err(AdmissionError::Limit);
    }
    Ok(())
}
fn diagnostic(
    code: C,
    path: &str,
    context: impl IntoIterator<Item = (&'static str, V)>,
) -> Diagnostic {
    Diagnostic {
        code,
        spec_paths: vec![path.into()],
        context: context
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
        source_span: None,
        operand_route: None,
    }
}
fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
}
fn actual(value: ValueRef<'_>) -> &'static str {
    match value {
        ValueRef::Str(_) => "str",
        ValueRef::Int(_) => "int",
        ValueRef::Float(_) => "float",
        ValueRef::Bool(_) => "bool",
        ValueRef::Date(_) => "date",
        ValueRef::DateTime(_) => "datetime",
        ValueRef::Missing => "missing",
    }
}
impl Mapping {
    /// Collision admission happens before any source resolution and retains authored entries.
    pub fn new(
        source: String,
        path: String,
        entries: Vec<(String, Value)>,
        case_sensitive: bool,
        missing: Option<Value>,
        unmapped: Option<Value>,
    ) -> Result<Self, AdmissionError> {
        metadata(
            &source,
            &path,
            entries
                .iter()
                .map(|(key, value)| {
                    key.len()
                        .saturating_add(text_bytes(value))
                        .saturating_add(64)
                })
                .chain(missing.iter().chain(unmapped.iter()).map(text_bytes)),
        )?;
        if !case_sensitive {
            let mut groups: BTreeMap<String, Vec<&str>> = BTreeMap::new();
            for (key, _) in &entries {
                groups
                    .entry(key.to_ascii_uppercase())
                    .or_default()
                    .push(key);
            }
            if let Some((folded, entries)) =
                groups.into_iter().find(|(_, entries)| entries.len() > 1)
            {
                return Err(AdmissionError::Diagnostic(diagnostic(
                    C::MappingAmbiguousDictionary,
                    &format!("{path}.dict"),
                    [
                        ("folded_key", text(&folded)),
                        (
                            "entries",
                            V::Sequence(entries.into_iter().map(text).collect()),
                        ),
                    ],
                )));
            }
        }
        Ok(Self {
            source,
            path,
            entries,
            case_sensitive,
            missing,
            unmapped,
        })
    }
    pub fn select(&self, value: ValueRef<'_>) -> Result<Selected<'_>, Diagnostic> {
        match value {
            ValueRef::Missing => self
                .missing
                .as_ref()
                .map(|value| Selected {
                    value: SelectedValue::Literal(value),
                    handler: Some("missing"),
                })
                .ok_or_else(|| {
                    diagnostic(
                        C::MappingMissing,
                        &self.path,
                        [("variable", text(&self.source))],
                    )
                }),
            ValueRef::Str(input) => {
                for (key, value) in &self.entries {
                    let matches = if self.case_sensitive {
                        input == key
                    } else {
                        input.eq_ignore_ascii_case(key)
                    };
                    if matches {
                        return Ok(Selected {
                            value: SelectedValue::Literal(value),
                            handler: None,
                        });
                    }
                }
                self.unmapped
                    .as_ref()
                    .map(|value| Selected {
                        value: SelectedValue::Literal(value),
                        handler: Some("unmapped"),
                    })
                    .ok_or_else(|| {
                        diagnostic(
                            C::MappingUnmapped,
                            &self.path,
                            [("source", text(&self.source)), ("value", text(input))],
                        )
                    })
            }
            other => Err(self.input_diagnostic(actual(other))),
        }
    }
    pub fn input_diagnostic(&self, actual: &str) -> Diagnostic {
        diagnostic(
            C::MappingInput,
            &format!("{}.source", self.path),
            [
                ("source", text(&self.source)),
                ("expected", text("str")),
                ("actual", text(actual)),
            ],
        )
    }
}
impl Cut {
    pub fn new(
        source: String,
        path: String,
        breaks: Vec<f64>,
        labels: Vec<String>,
        right: bool,
        missing: Option<Value>,
    ) -> Result<Self, AdmissionError> {
        metadata(
            &source,
            &path,
            labels
                .iter()
                .map(|label| label.len().saturating_add(32))
                .chain(missing.iter().map(text_bytes))
                .chain(core::iter::once(breaks.len().saturating_mul(8))),
        )?;
        let expected = if labels.len() != breaks.len().saturating_add(1) {
            Some("one label more than breaks")
        } else if breaks.iter().any(|n| !n.is_finite()) {
            Some("finite numeric breaks")
        } else if breaks.windows(2).any(|pair| pair[1] <= pair[0]) {
            Some("ascending breaks")
        } else {
            None
        };
        if let Some(expected) = expected {
            return Err(AdmissionError::Diagnostic(diagnostic(
                C::CutDeclaration,
                &path,
                [("operation", text("cut")), ("expected", text(expected))],
            )));
        }
        Ok(Self {
            source,
            path,
            breaks,
            labels,
            right,
            missing,
        })
    }
    pub fn select(&self, value: ValueRef<'_>) -> Result<Selected<'_>, Diagnostic> {
        if matches!(value, ValueRef::Missing) {
            return self
                .missing
                .as_ref()
                .map(|value| Selected {
                    value: SelectedValue::Literal(value),
                    handler: Some("missing"),
                })
                .ok_or_else(|| {
                    diagnostic(
                        C::CutMissing,
                        &self.path,
                        [("variable", text(&self.source))],
                    )
                });
        }
        if !matches!(value, ValueRef::Int(_) | ValueRef::Float(_)) {
            return Err(self.input_diagnostic(actual(value)));
        }
        // Exact mixed-number comparison avoids rounding an integer to a neighboring boundary.
        let index = self.breaks.partition_point(|&boundary| {
            let ordering = compare_present_refs(
                value,
                ValueRef::Float(FiniteFloat::new(boundary).expect("admitted finite boundary")),
            )
            .expect("numeric pair");
            ordering == Ordering::Greater || (!self.right && ordering == Ordering::Equal)
        });
        Ok(Selected {
            value: SelectedValue::Label(&self.labels[index]),
            handler: None,
        })
    }
    pub fn input_diagnostic(&self, actual: &str) -> Diagnostic {
        diagnostic(
            C::CutInput,
            &format!("{}.source", self.path),
            [
                ("source", text(&self.source)),
                ("expected", text("numeric")),
                ("actual", text(actual)),
            ],
        )
    }
}
impl MappingCut {
    pub fn source(&self) -> &str {
        match self {
            Self::Mapping(item) => &item.source,
            Self::Cut(item) => &item.source,
        }
    }
    pub fn path(&self) -> &str {
        match self {
            Self::Mapping(item) => &item.path,
            Self::Cut(item) => &item.path,
        }
    }
    pub fn missing(&self) -> Option<&Value> {
        match self {
            Self::Mapping(item) => item.missing.as_ref(),
            Self::Cut(item) => item.missing.as_ref(),
        }
    }
    pub fn unmapped(&self) -> Option<&Value> {
        match self {
            Self::Mapping(item) => item.unmapped.as_ref(),
            Self::Cut(_) => None,
        }
    }
    pub fn select(&self, value: ValueRef<'_>) -> Result<Selected<'_>, Diagnostic> {
        match self {
            Self::Mapping(item) => item.select(value),
            Self::Cut(item) => item.select(value),
        }
    }
    pub fn input_diagnostic(&self, actual: &str) -> Diagnostic {
        match self {
            Self::Mapping(item) => item.input_diagnostic(actual),
            Self::Cut(item) => item.input_diagnostic(actual),
        }
    }
    pub fn accepts(&self, kind: crate::value::ColumnType) -> bool {
        match self {
            Self::Mapping(_) => kind == crate::value::ColumnType::Str,
            Self::Cut(_) => matches!(
                kind,
                crate::value::ColumnType::Int | crate::value::ColumnType::Float
            ),
        }
    }
    /// Bound complete comparison work before entering the selector's bounded scan.
    pub fn comparison_work(&self, input: ValueRef<'_>) -> usize {
        match self {
            Self::Mapping(item) => item.entries.iter().fold(1usize, |n, (key, _)| {
                n.saturating_add(
                    key.len()
                        .saturating_add(match input {
                            ValueRef::Str(input) => input.len(),
                            _ => 0,
                        })
                        .saturating_add(1),
                )
            }),
            Self::Cut(item) => item.breaks.len().saturating_add(1),
        }
    }
}
