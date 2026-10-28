//! Pure static codelist admission and binding over lowered codelist sources.
//! Source bytes and schema admission belong to captured inputs.
use crate::ast::{Codelist, CodelistItem, CodelistType, Codelists};
use crate::value::{compare_present, ColumnType, Value};
use std::cmp::Ordering;
use std::collections::BTreeSet;

fn admits(kind: CodelistType, value: &Value) -> bool {
    match (kind, value) {
        (CodelistType::Text, Value::Str(_))
        | (CodelistType::Integer, Value::Int(_))
        | (CodelistType::Float, Value::Int(_) | Value::Float(_)) => true,
        (CodelistType::Integer, Value::Float(float)) => {
            let value = float.get();
            value % 1.0 == 0.0
        }
        _ => false,
    }
}

/// Ordered direct values avoid quadratic scans and never round integers to f64.
/// Invalid item kinds still retain exact duplicate findings during admission.
struct ValueKey<'a> {
    numeric: bool,
    value: &'a Value,
}
impl<'a> ValueKey<'a> {
    fn new(kind: CodelistType, value: &'a Value) -> Self {
        Self {
            numeric: !matches!(kind, CodelistType::Text),
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    DuplicateId,
    DuplicateName,
    FormatNamePrefix,
    InvalidShape,
    InvalidValue { item: usize },
    DuplicateValue { item: usize },
    PartialDecode,
    PartialRank,
    ExtensionNotAdmitted { item: usize },
    ExtendedAlias { item: usize },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub source: usize,
    pub codelist: usize,
    pub fault: Fault,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingFinding {
    UnknownCodelist,
    TypeMismatch {
        expected: ColumnType,
        actual: ColumnType,
    },
    /// An `allowed_values` verification restates the bound list's items.
    RedundantAllowedValues,
}

/// The composed codelists: a later source's list replaces each earlier list of
/// its `id` whole (CT-1). Each source keeps the lists no later source
/// replaces, in authored source/list order, so a list stays with the source
/// whose `standard` it is drawn from (CT-2).
#[derive(Clone, Debug, PartialEq)]
pub struct Catalogue {
    sources: Vec<Codelists>,
}
impl Catalogue {
    /// Compose the globally admitted environment's original sources, which
    /// keep their indices: a source whose lists are all replaced stays empty.
    pub(crate) fn from_admitted(mut sources: Vec<Codelists>) -> Self {
        let replaced = replaced(&sources);
        for (index, source) in sources.iter_mut().enumerate() {
            let mut codelist = 0;
            source.codelists.retain(|_| {
                codelist += 1;
                !replaced.contains(&(index, codelist - 1))
            });
        }
        Self { sources }
    }
    pub fn admit(sources: Vec<Codelists>) -> Result<Self, Vec<Finding>> {
        let findings = validate(&sources);
        if findings.is_empty() {
            Ok(Self::from_admitted(sources))
        } else {
            Err(findings)
        }
    }
    pub fn sources(&self) -> &[Codelists] {
        &self.sources
    }
    pub fn get(&self, id: &str) -> Option<&Codelist> {
        self.sources
            .iter()
            .flat_map(|source| &source.codelists)
            .find(|list| list.id.name == id)
    }
    /// Every list that declares items, extensible or not, contributes an
    /// allowed-values column checkpoint; an external dictionary contributes none.
    pub fn enforced_items(&self, id: &str) -> Option<&[CodelistItem]> {
        self.get(id)?.items.as_deref()
    }
    /// `allowed_values` reports whether the bound column also declares an
    /// `allowed_values` verification, which a list of items already states.
    pub fn bind(&self, id: &str, column: ColumnType, allowed_values: bool) -> Vec<BindingFinding> {
        let Some(list) = self.get(id) else {
            return std::vec![BindingFinding::UnknownCodelist];
        };
        let mut findings = Vec::new();
        if list.data_type.column_type() != column {
            findings.push(BindingFinding::TypeMismatch {
                expected: list.data_type.column_type(),
                actual: column,
            });
        }
        if allowed_values && list.items.is_some() {
            findings.push(BindingFinding::RedundantAllowedValues);
        }
        findings
    }
}

/// The position, by source and list, of each list a later source replaces by
/// declaring its `id` (CT-1).
fn replaced(sources: &[Codelists]) -> BTreeSet<(usize, usize)> {
    let mut later = BTreeSet::new();
    let mut replaced = BTreeSet::new();
    for (source, declared) in sources.iter().enumerate().rev() {
        for (codelist, list) in declared.codelists.iter().enumerate() {
            if later.contains(list.id.name.as_str()) {
                replaced.insert((source, codelist));
            }
        }
        later.extend(declared.codelists.iter().map(|list| list.id.name.as_str()));
    }
    replaced
}

/// Validate every list across every captured source without resolving dictionaries.
/// A replaced list is still checked on its own; an `id` repeats only within
/// one source, and a `name` is unique over the composed lists (CT-1).
pub fn validate(sources: &[Codelists]) -> Vec<Finding> {
    let mut findings = Vec::new();
    let replaced = replaced(sources);
    let mut names = BTreeSet::new();
    for (source_index, source) in sources.iter().enumerate() {
        let mut ids = BTreeSet::new();
        for (codelist, list) in source.codelists.iter().enumerate() {
            let mut push = |fault| {
                findings.push(Finding {
                    source: source_index,
                    codelist,
                    fault,
                })
            };
            if !ids.insert(&list.id.name) {
                push(Fault::DuplicateId);
            }
            if !replaced.contains(&(source_index, codelist)) && !names.insert(&list.name.text) {
                push(Fault::DuplicateName);
            }
            // A text codelist's SAS format is a character format, whose name
            // begins with `$`, and a numeric codelist's is not (CT-12).
            if list.format_name.as_ref().is_some_and(|name| {
                name.text.starts_with('$') != (list.data_type == CodelistType::Text)
            }) {
                push(Fault::FormatNamePrefix);
            }
            if list.items.is_some() == list.external.is_some() {
                push(Fault::InvalidShape);
            }
            if let Some(items) = &list.items {
                let mut values = BTreeSet::new();
                for (item_index, item) in items.iter().enumerate() {
                    if !admits(list.data_type, &item.value) {
                        push(Fault::InvalidValue { item: item_index });
                    }
                    if !values.insert(ValueKey::new(list.data_type, &item.value)) {
                        push(Fault::DuplicateValue { item: item_index });
                    }
                    if item.extended && (!list.extensible || source.standard.is_none()) {
                        push(Fault::ExtensionNotAdmitted { item: item_index });
                    }
                    // A sponsor addition has no published identifier (CT-6).
                    if item.extended && item.alias.is_some() {
                        push(Fault::ExtendedAlias { item: item_index });
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
