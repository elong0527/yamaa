//! Pure static codelist admission and binding for environment sources (#1757).
//! Source bytes, schema admission and storage budgets belong to captured inputs.
use crate::project_limits::{AdmissionError, Budget, Limit, Limits};
use crate::value::{compare_present, ColumnType, Value};
use alloc::{collections::BTreeSet, string::String, vec::Vec};
use core::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    Text,
    Integer,
    Float,
}
impl DataType {
    pub fn column_type(self) -> ColumnType {
        match self {
            Self::Text => ColumnType::Str,
            Self::Integer => ColumnType::Int,
            Self::Float => ColumnType::Float,
        }
    }
    fn admits(self, value: &Value) -> bool {
        match (self, value) {
            (Self::Text, Value::Str(_))
            | (Self::Integer, Value::Int(_))
            | (Self::Float, Value::Int(_) | Value::Float(_)) => true,
            (Self::Integer, Value::Float(float)) => {
                let value = float.get();
                value % 1.0 == 0.0
            }
            _ => false,
        }
    }
}

/// Ordered direct values avoid quadratic scans and never round integers to f64.
/// Invalid item kinds still retain exact duplicate findings during admission.
struct ValueKey<'a> {
    numeric: bool,
    value: &'a Value,
}
impl<'a> ValueKey<'a> {
    fn new(kind: DataType, value: &'a Value) -> Self {
        Self {
            numeric: !matches!(kind, DataType::Text),
            value,
        }
    }
    fn category(&self) -> u8 {
        match self.value {
            Value::Missing => 0,
            Value::Str(_) => 1,
            Value::Int(_) => 2,
            Value::Float(_) if self.numeric => 2,
            Value::Float(_) => 3,
            Value::Bool(_) => 4,
            Value::Date(_) => 5,
            Value::DateTime(_) => 6,
        }
    }
}
impl PartialEq for ValueKey<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for ValueKey<'_> {}
impl PartialOrd for ValueKey<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ValueKey<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.numeric
            .cmp(&other.numeric)
            .then_with(|| self.category().cmp(&other.category()))
            .then_with(|| match (self.value, other.value) {
                (Value::Missing, Value::Missing) => Ordering::Equal,
                (Value::Bool(left), Value::Bool(right)) => left.cmp(right),
                _ => compare_present(self.value, other.value)
                    .expect("equal categories have comparable present values"),
            })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub value: Value,
    pub decode: Option<String>,
    pub rank: Option<i64>,
    pub alias: Option<String>,
    pub extended: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct External {
    pub dictionary: String,
    pub version: String,
    pub href: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Codelist {
    pub id: String,
    pub name: String,
    pub data_type: DataType,
    pub extensible: bool,
    pub alias: Option<String>,
    pub format_name: Option<String>,
    pub items: Option<Vec<Item>>,
    pub external: Option<External>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standard {
    pub name: String,
    pub publishing_set: String,
    pub version: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub standard: Option<Standard>,
    pub codelists: Vec<Codelist>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    DuplicateId,
    DuplicateName,
    InvalidShape,
    InvalidValue { item: usize },
    DuplicateValue { item: usize },
    PartialDecode,
    PartialRank,
    ExtensionNotAdmitted { item: usize },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub source: usize,
    pub codelist: usize,
    pub fault: Fault,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingFinding {
    Limit(Limit),
    UnknownCodelist,
    TypeMismatch {
        expected: ColumnType,
        actual: ColumnType,
    },
    ValuesConflict,
}

/// No source overrides another; catalogue order is the authored source/list order.
#[derive(Clone, Debug, PartialEq)]
pub struct Catalogue {
    sources: Vec<Source>,
}
impl Catalogue {
    pub fn admit(sources: Vec<Source>) -> Result<Self, AdmissionError<Finding>> {
        Self::admit_with_limits(sources, Limits::default())
    }
    pub fn admit_with_limits(
        sources: Vec<Source>,
        limits: Limits,
    ) -> Result<Self, AdmissionError<Finding>> {
        Budget::new(limits)
            .sources(&sources)
            .map_err(AdmissionError::Limit)?;
        let findings = validate(&sources);
        if findings.is_empty() {
            Ok(Self { sources })
        } else {
            Err(AdmissionError::Findings(findings))
        }
    }
    pub fn sources(&self) -> &[Source] {
        &self.sources
    }
    pub fn get(&self, id: &str) -> Option<&Codelist> {
        self.sources
            .iter()
            .flat_map(|source| &source.codelists)
            .find(|list| list.id == id)
    }
    /// Only fixed item lists contribute an allowed-values column checkpoint.
    pub fn enforced_items(&self, id: &str) -> Option<&[Item]> {
        let list = self.get(id)?;
        if list.extensible {
            None
        } else {
            list.items.as_deref()
        }
    }
    pub fn bind(
        &self,
        id: &str,
        column: ColumnType,
        allowed_values: Option<&[Value]>,
    ) -> Vec<BindingFinding> {
        let Some(list) = self.get(id) else {
            return alloc::vec![BindingFinding::UnknownCodelist];
        };
        let mut findings = Vec::new();
        if list.data_type.column_type() != column {
            findings.push(BindingFinding::TypeMismatch {
                expected: list.data_type.column_type(),
                actual: column,
            });
        }
        if let (Some(items), Some(allowed)) = (self.enforced_items(id), allowed_values) {
            if let Err(limit) = Budget::new(Limits::default()).binding(items, allowed) {
                return alloc::vec![BindingFinding::Limit(limit)];
            }
            let items: BTreeSet<_> = items
                .iter()
                .map(|item| ValueKey::new(list.data_type, &item.value))
                .collect();
            let allowed: BTreeSet<_> = allowed
                .iter()
                .map(|value| ValueKey::new(list.data_type, value))
                .collect();
            let equal = items == allowed;
            if !equal {
                findings.push(BindingFinding::ValuesConflict);
            }
        }
        findings
    }
}

/// Validate every list across every captured source without resolving dictionaries.
pub fn validate(sources: &[Source]) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for (source_index, source) in sources.iter().enumerate() {
        for (codelist, list) in source.codelists.iter().enumerate() {
            let mut push = |fault| {
                findings.push(Finding {
                    source: source_index,
                    codelist,
                    fault,
                })
            };
            if !ids.insert(&list.id) {
                push(Fault::DuplicateId);
            }
            if !names.insert(&list.name) {
                push(Fault::DuplicateName);
            }
            if list.items.is_some() == list.external.is_some() {
                push(Fault::InvalidShape);
            }
            if let Some(items) = &list.items {
                let mut values = BTreeSet::new();
                for (item_index, item) in items.iter().enumerate() {
                    if !list.data_type.admits(&item.value) {
                        push(Fault::InvalidValue { item: item_index });
                    }
                    if !values.insert(ValueKey::new(list.data_type, &item.value)) {
                        push(Fault::DuplicateValue { item: item_index });
                    }
                    if item.extended && (!list.extensible || source.standard.is_none()) {
                        push(Fault::ExtensionNotAdmitted { item: item_index });
                    }
                }
                if items.iter().any(|item| item.decode.is_some())
                    && items.iter().any(|item| item.decode.is_none())
                {
                    push(Fault::PartialDecode);
                }
                if items.iter().any(|item| item.rank.is_some())
                    && items.iter().any(|item| item.rank.is_none())
                {
                    push(Fault::PartialRank);
                }
            }
        }
    }
    findings
}
