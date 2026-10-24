//! Closed dictionary and interval selection, preserving raw literal results.
use crate::expression_table::Verb;
use crate::json::Value as V;
use crate::{
    diagnostic::{ConditionCode as C, Diagnostic},
    table::ValueRef,
    value::{compare_present_refs, FiniteFloat, Value},
};
use std::cmp::Ordering;
use std::{collections::BTreeMap, vec};

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum AdmissionError {
    #[error("mapping cut shape")]
    Shape,
    #[error("mapping cut diagnostic: {0:?}")]
    Diagnostic(Diagnostic),
}
/// A selected result borrows its declared literal or label. `Missing` is the
/// default substitution for a missing source without a declared `missing`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SelectedValue<'a> {
    Literal(&'a Value),
    Label(&'a str),
    Missing,
}
/// `handler` names the handler path a substitution is counted under, whether
/// the handler is declared or its default substitution answers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selected<'a> {
    pub value: SelectedValue<'a>,
    pub handler: Option<&'static str>,
}
/// A missing source takes the declared `missing` literal, or missing by
/// default, and is counted under `missing` either way (COLUMN-10).
fn missing_source(missing: Option<&Value>) -> Selected<'_> {
    Selected {
        value: missing.map_or(SelectedValue::Missing, SelectedValue::Literal),
        handler: Some("missing"),
    }
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
fn shape(source: &str, path: &str) -> Result<(), AdmissionError> {
    if source.is_empty() || path.is_empty() {
        return Err(AdmissionError::Shape);
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
    V::String(value.into())
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
        shape(&source, &path)?;
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
                        ("entries", V::Array(entries.into_iter().map(text).collect())),
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
            ValueRef::Missing => Ok(missing_source(self.missing.as_ref())),
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
        shape(&source, &path)?;
        if let Some(finding) = Self::declaration_findings(&path, &breaks, &labels)
            .into_iter()
            .next()
        {
            return Err(AdmissionError::Diagnostic(finding));
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
    /// Independent declaration defects share the same admitted context and order.
    pub(crate) fn declaration_findings(
        path: &str,
        breaks: &[f64],
        labels: &[String],
    ) -> Vec<Diagnostic> {
        let expected = [
            (
                labels.len() != breaks.len().saturating_add(1),
                "one label more than breaks",
            ),
            (
                breaks.iter().any(|n| !n.is_finite()),
                "finite numeric breaks",
            ),
            (
                breaks.windows(2).any(|pair| pair[1] <= pair[0]),
                "ascending breaks",
            ),
        ];
        expected
            .into_iter()
            .filter(|(invalid, _)| *invalid)
            .map(|(_, expected)| {
                diagnostic(
                    C::CutDeclaration,
                    path,
                    [
                        ("operation", text(Verb::Cut.name())),
                        ("expected", text(expected)),
                    ],
                )
            })
            .collect()
    }
    pub fn select(&self, value: ValueRef<'_>) -> Result<Selected<'_>, Diagnostic> {
        if matches!(value, ValueRef::Missing) {
            return Ok(missing_source(self.missing.as_ref()));
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
}
