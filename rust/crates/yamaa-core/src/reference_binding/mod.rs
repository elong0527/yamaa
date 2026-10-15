//! Owned reference catalogs and output-reference validation, without data access.
use std::collections::BTreeMap;

use crate::value::ColumnType;
mod joined_binding;
pub use joined_binding::JoinedFinding;

/// Borrowed field metadata supplied by a schema/normalization adapter.
#[derive(Clone, Copy, Debug)]
pub struct Field<'a> {
    pub name: &'a str,
    pub column_type: ColumnType,
}

/// A relation's declared name and ordered stored fields, not its records.
#[derive(Clone, Copy, Debug)]
pub struct Dataset<'a> {
    pub name: &'a str,
    pub fields: &'a [Field<'a>],
}

/// Rejected metadata or query shape; unresolved language names are results.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("reference binding empty name")]
    EmptyName,
    #[error("reference binding duplicate output")]
    DuplicateOutput,
    #[error("reference binding duplicate dataset")]
    DuplicateDataset,
    #[error("reference binding duplicate field")]
    DuplicateField,
    #[error("reference binding invalid available output")]
    InvalidAvailableOutput,
    #[error("reference binding invalid candidate dataset")]
    InvalidCandidateDataset,
    #[error("reference binding qualified output query")]
    QualifiedOutputQuery,
    #[error("reference binding bare qualified query")]
    BareQualifiedQuery,
}

/// A resolved declaration and exact declared type, with no host representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Binding {
    Output {
        column: usize,
        column_type: ColumnType,
    },
    Dataset {
        dataset: usize,
        field: usize,
        column_type: ColumnType,
    },
}

/// One output-reference condition; hosts retain the authored reference path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    UnknownField,
    UnresolvableName {
        dataset: usize,
    },
    PhaseBoundary {
        column: usize,
    },
    IncompatibleInputType {
        expected: ColumnType,
        actual: ColumnType,
    },
}

/// An immutable, owned catalog safe to reuse independently of source buffers.
#[derive(Debug)]
pub struct Catalog {
    record_derivation: bool,
    record_kinds: Option<Vec<Option<ColumnType>>>,
    outputs: BTreeMap<String, (usize, ColumnType)>,
    datasets: BTreeMap<String, DatasetCatalog>,
    selected: BTreeMap<String, SelectedRelation>,
    joined: BTreeMap<String, JoinedRelation>,
    row_joins: Option<RowJoinScope>,
}
#[derive(Debug)]
struct RowJoinScope {
    driver: String,
    groups: Option<std::vec::Vec<String>>,
}
#[derive(Debug)]
pub(crate) struct JoinedRelation {
    pub source: usize,
    pub fields: BTreeMap<String, (usize, ColumnType)>,
    pub inference: crate::key_relation::Inference,
    pub keys: std::vec::Vec<(String, usize)>,
    pub schema: std::sync::Arc<crate::table::TableSchema>,
}

/// A kept relation used by a scalar callback, with its original key dependencies.
#[derive(Debug)]
pub(crate) struct SelectedRelation {
    pub index: usize,
    pub fields: BTreeMap<String, (usize, Option<ColumnType>)>,
    pub dependencies: std::vec::Vec<usize>,
}
pub(crate) type SelectedRead<'a> = (
    crate::bound_expression::Read,
    Option<ColumnType>,
    &'a [usize],
);

#[derive(Debug)]
struct DatasetCatalog {
    index: usize,
    fields: BTreeMap<String, (usize, ColumnType)>,
}

impl Catalog {
    /// Donor compilers explicitly admit current-record fields before using scalar expressions.
    pub(crate) fn with_record_derivation(mut self) -> Self {
        self.record_derivation = true;
        self
    }
    pub(crate) fn record_derivation(&self) -> bool {
        self.record_derivation
    }
    /// Raw donor results need not have one known kind despite having a layout slot.
    pub(crate) fn with_record_kinds(mut self, kinds: Vec<Option<ColumnType>>) -> Self {
        self.record_kinds = Some(kinds);
        self
    }
    pub(crate) fn with_selected(
        mut self,
        selected: std::vec::Vec<(String, SelectedRelation)>,
    ) -> Result<Self, Error> {
        for (name, relation) in selected {
            if name.is_empty() {
                return Err(Error::EmptyName);
            }
            if self.datasets.contains_key(&name) || self.selected.contains_key(&name) {
                return Err(Error::DuplicateDataset);
            }
            self.selected.insert(name, relation);
        }
        Ok(self)
    }

    pub(crate) fn selected_read(&self, name: &str) -> Option<Result<SelectedRead<'_>, Diagnostic>> {
        let (owner, field) = name.split_once('.')?;
        let relation = self.selected.get(owner)?;
        Some(
            relation
                .fields
                .get(field)
                .map(|&(column, kind)| {
                    (
                        crate::bound_expression::Read::Intermediate {
                            index: relation.index,
                            column,
                        },
                        kind,
                        relation.dependencies.as_slice(),
                    )
                })
                .ok_or(Diagnostic::UnknownField),
        )
    }
    /// Stored and augmented donor fields obey the current declaration's order.
    pub(crate) fn restrict_record_fields(&mut self, available: &[usize]) {
        let width = self.outputs.len();
        for dataset in self
            .datasets
            .values_mut()
            .filter(|dataset| dataset.index == 0)
        {
            dataset
                .fields
                .retain(|_, (field, _)| *field >= width || available.contains(field));
        }
    }
    pub(crate) fn complete_record_field(&mut self, field: usize, column: &crate::table::Column) {
        for dataset in self
            .datasets
            .values_mut()
            .filter(|dataset| dataset.index == 0)
        {
            dataset
                .fields
                .insert(column.name.clone(), (field, column.kind));
        }
    }
    pub(crate) fn joined(&self, name: &str) -> Option<&JoinedRelation> {
        self.joined.get(name)
    }
    pub(crate) fn type_of(&self, name: &str) -> Option<ColumnType> {
        if let Some(binding) = self.bind(name) {
            return match binding {
                Binding::Output {
                    column,
                    column_type,
                } => self
                    .record_kinds
                    .as_ref()
                    .map_or(Some(column_type), |kinds| {
                        kinds.get(column).copied().flatten()
                    }),
                Binding::Dataset {
                    dataset: 0,
                    field,
                    column_type,
                } => self
                    .record_kinds
                    .as_ref()
                    .map_or(Some(column_type), |kinds| {
                        kinds.get(field).copied().flatten()
                    }),
                Binding::Dataset { column_type, .. } => Some(column_type),
            };
        }
        name.split_once('.').and_then(|(relation, field)| {
            self.joined(relation)?
                .fields
                .get(field)
                .map(|(_, kind)| *kind)
        })
    }

    /// Row-template implicit joins match raw driver fields, independently of output derivations.
    pub(crate) fn with_row_joins(mut self, driver: &str, groups: Option<&[String]>) -> Self {
        self.row_joins = Some(RowJoinScope {
            driver: driver.into(),
            groups: groups.map(<[String]>::to_vec),
        });
        self
    }

    /// Retain pure donor schemas and inferred current-output equality metadata.
    pub(crate) fn with_joined(
        mut self,
        relations: &[(usize, Dataset<'_>)],
        keys: &[usize],
    ) -> Result<Self, Error> {
        let outputs = self.output_names();
        let keys = keys
            .iter()
            .map(|&key| {
                outputs
                    .get(key)
                    .copied()
                    .map(String::from)
                    .ok_or(Error::InvalidAvailableOutput)
            })
            .collect::<Result<std::vec::Vec<_>, _>>()?;
        let retained_keys = keys
            .iter()
            .map(|name| (name.clone(), self.outputs[name].0))
            .collect::<std::vec::Vec<_>>();
        for (source, relation) in relations {
            let names = keys
                .iter()
                .map(String::as_str)
                .collect::<std::vec::Vec<_>>();
            let inference = crate::key_relation::infer(&self, &names, relation.fields)?;
            let fields = relation
                .fields
                .iter()
                .enumerate()
                .map(|(index, field)| (field.name.into(), (index, field.column_type)))
                .collect();
            let schema = std::sync::Arc::new(
                crate::table::TableSchema::new(
                    relation
                        .fields
                        .iter()
                        .map(|field| crate::table::Column {
                            name: field.name.into(),
                            kind: field.column_type,
                        })
                        .collect(),
                )
                .map_err(|_| Error::DuplicateField)?,
            );
            self.joined.insert(
                relation.name.into(),
                JoinedRelation {
                    source: *source,
                    fields,
                    inference,
                    keys: retained_keys.clone(),
                    schema,
                },
            );
        }
        Ok(self)
    }
    pub(crate) fn output_names(&self) -> std::vec::Vec<&str> {
        let mut names = self
            .outputs
            .iter()
            .map(|(name, (index, _))| (*index, name.as_str()))
            .collect::<std::vec::Vec<_>>();
        names.sort_unstable_by_key(|(index, _)| *index);
        names.into_iter().map(|(_, name)| name).collect()
    }
    pub(crate) fn dataset_name(&self, index: usize) -> Option<&str> {
        self.datasets
            .iter()
            .find(|(_, dataset)| dataset.index == index)
            .map(|(name, _)| name.as_str())
    }
    /// Suggest a qualifier only when exactly one admitted dataset owns a bare field.
    pub(crate) fn unique_field_owner(&self, field: &str) -> Option<&str> {
        let mut owners = self
            .datasets
            .iter()
            .filter(|(_, dataset)| dataset.fields.contains_key(field))
            .map(|(name, _)| name.as_str())
            .chain(
                self.joined
                    .iter()
                    .filter(|(_, relation)| {
                        relation.fields.contains_key(field)
                            && matches!(
                                relation.inference,
                                crate::key_relation::Inference::Keys { .. }
                            )
                    })
                    .map(|(name, _)| name.as_str()),
            );
        let first = owners.next();
        if owners.next().is_none() {
            first
        } else {
            None
        }
    }
    /// Borrow a declared relation's fields in stored column order.
    pub(crate) fn fields(&self, name: &str) -> Option<std::vec::Vec<Field<'_>>> {
        let dataset = self.datasets.get(name)?;
        let mut fields = dataset
            .fields
            .iter()
            .map(|(name, (index, column_type))| {
                (
                    *index,
                    Field {
                        name,
                        column_type: *column_type,
                    },
                )
            })
            .collect::<std::vec::Vec<_>>();
        fields.sort_unstable_by_key(|(index, _)| *index);
        Some(fields.into_iter().map(|(_, field)| field).collect())
    }
    pub(crate) fn output_width(&self) -> usize {
        self.outputs.len()
    }
    /// Read an exact output declaration without interpreting it as a source path.
    pub(crate) fn output_type(&self, name: &str) -> Option<ColumnType> {
        self.outputs.get(name).map(|&(_, kind)| kind)
    }

    /// Inspect literal stored-field visibility, retaining an absent relation as distinct metadata.
    pub(crate) fn has_stored_field(&self, dataset: &str, field: &str) -> Option<bool> {
        self.datasets
            .get(dataset)
            .map(|dataset| dataset.fields.contains_key(field))
    }

    /// Validate nonempty names before allocating reusable name indexes.
    pub fn compile(outputs: &[Field<'_>], datasets: &[Dataset<'_>]) -> Result<Self, Error> {
        let names = outputs
            .iter()
            .map(|field| field.name)
            .chain(datasets.iter().flat_map(|dataset| {
                std::iter::once(dataset.name).chain(dataset.fields.iter().map(|field| field.name))
            }));
        if names.clone().any(str::is_empty) {
            return Err(Error::EmptyName);
        }
        let mut output_index = BTreeMap::new();
        for (index, field) in outputs.iter().enumerate() {
            if output_index
                .insert(String::from(field.name), (index, field.column_type))
                .is_some()
            {
                return Err(Error::DuplicateOutput);
            }
        }
        let mut dataset_index = BTreeMap::new();
        for (index, dataset) in datasets.iter().enumerate() {
            let mut fields = BTreeMap::new();
            for (index, field) in dataset.fields.iter().enumerate() {
                if fields
                    .insert(String::from(field.name), (index, field.column_type))
                    .is_some()
                {
                    return Err(Error::DuplicateField);
                }
            }
            if dataset_index
                .insert(String::from(dataset.name), DatasetCatalog { index, fields })
                .is_some()
            {
                return Err(Error::DuplicateDataset);
            }
        }
        Ok(Self {
            record_derivation: false,
            record_kinds: None,
            outputs: output_index,
            datasets: dataset_index,
            selected: BTreeMap::new(),
            joined: BTreeMap::new(),
            row_joins: None,
        })
    }

    /// Resolve a declared relation without inventing a stored field, including empty schemas.
    pub fn bind_relation(&self, name: &str) -> Option<usize> {
        self.datasets.get(name).map(|dataset| dataset.index)
    }

    /// Resolve bare outputs or a dataset plus its exact stored suffix (REQ-0103/1265).
    /// A dotted suffix is a field only when literally present; it is never an ODM path.
    pub fn bind(&self, name: &str) -> Option<Binding> {
        if let Some((qualifier, field)) = name.split_once('.') {
            self.datasets.get(qualifier).and_then(|dataset| {
                dataset
                    .fields
                    .get(field)
                    .map(|&(index, column_type)| Binding::Dataset {
                        dataset: dataset.index,
                        field: index,
                        column_type,
                    })
            })
        } else {
            self.outputs
                .get(name)
                .map(|&(column, column_type)| Binding::Output {
                    column,
                    column_type,
                })
        }
    }

    /// Check one bare reference in unknown-name, phase, then expected-type order.
    /// `available` is absent during column derivation and lists available output
    /// declarations during row construction. Candidate datasets only affect the
    /// qualified spelling suggested for an unresolved bare name (REQ-0106).
    pub fn validate_output(
        &self,
        name: &str,
        expected: Option<ColumnType>,
        available: Option<&[usize]>,
        candidates: &[usize],
    ) -> Result<Option<Diagnostic>, Error> {
        if name.contains('.') {
            return Err(Error::QualifiedOutputQuery);
        }
        if let Some(available) = available {
            if available.iter().any(|&index| index >= self.outputs.len()) {
                return Err(Error::InvalidAvailableOutput);
            }
        }
        if candidates.iter().any(|&index| index >= self.datasets.len()) {
            return Err(Error::InvalidCandidateDataset);
        }
        let Some(&(column, actual)) = self.outputs.get(name) else {
            if candidates.is_empty() {
                return Ok(Some(Diagnostic::UnknownField));
            }
            // Admission above bounds these declaration indices. Build membership
            // once instead of rescanning every candidate for every dataset.
            let mut eligible = vec![false; self.datasets.len()];
            for &index in candidates {
                eligible[index] = true;
            }
            // The map's lexicographic order preserves the reference compiler's
            // sorted dataset-name suggestion, regardless of declaration order.
            return Ok(Some(
                self.datasets
                    .values()
                    .find(|dataset| eligible[dataset.index] && dataset.fields.contains_key(name))
                    .map_or(Diagnostic::UnknownField, |dataset| {
                        Diagnostic::UnresolvableName {
                            dataset: dataset.index,
                        }
                    }),
            ));
        };
        if available.is_some_and(|available| !available.contains(&column)) {
            return Ok(Some(Diagnostic::PhaseBoundary { column }));
        }
        Ok(expected
            .filter(|&expected| expected != actual)
            .map(|expected| Diagnostic::IncompatibleInputType { expected, actual }))
    }
}
