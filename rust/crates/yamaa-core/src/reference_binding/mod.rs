//! Owned reference catalogs and output-reference validation, without data access.
use std::collections::BTreeMap;

use crate::value::ColumnType;
mod joined_binding;
pub use joined_binding::JoinedFinding;

/// Borrowed field metadata supplied by a schema/normalization adapter: a
/// field's name and its type, `None` where the schema source does not decide
/// it (#3805), so every check that compares it is skipped.
#[derive(Clone, Copy, Debug)]
pub struct Field<'a> {
    pub name: &'a str,
    pub column_type: Option<ColumnType>,
}

/// One side of a pair relation's joined record (LOOKUP-27): its name and each
/// `(field, position, type)` it reads, at the field's position in the record.
pub(crate) type JoinedSide<'a> = (&'a str, &'a [(&'a str, usize, Option<ColumnType>)]);

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

/// A resolved declaration and its declared type, `None` where the schema
/// source does not decide it, with no host representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Binding {
    Output {
        column: usize,
        column_type: Option<ColumnType>,
    },
    Dataset {
        dataset: usize,
        field: usize,
        column_type: Option<ColumnType>,
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
    record_types: Option<Vec<crate::result_type::ResultType>>,
    outputs: BTreeMap<String, (usize, Option<ColumnType>)>,
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
    pub fields: BTreeMap<String, (usize, Option<ColumnType>)>,
    /// The column-phase match, its indices into `keys`.
    pub inference: crate::key_relation::Inference,
    /// The input's declared key, which the join pairs in full, each column
    /// with the current row's same-named output column; `None` names a
    /// declared column the specification does not declare (LOOKUP-1).
    pub keys: std::vec::Vec<(String, Option<usize>)>,
    pub schema: std::sync::Arc<crate::table::TableSchema>,
    /// A pair relation's port, which declares no input `key`
    /// (LOOKUP-27).
    pub pair: bool,
}

impl JoinedRelation {
    /// Each field's kind, in the order of `schema`.
    pub fn kinds(&self) -> std::vec::Vec<Option<ColumnType>> {
        let mut kinds = std::vec![None; self.schema.columns().len()];
        for &(index, kind) in self.fields.values() {
            kinds[index] = kind;
        }
        kinds
    }
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
    fields: BTreeMap<String, (usize, Option<ColumnType>)>,
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
    /// Raw donor results need not have one known kind despite having a
    /// layout slot: each record field's result type, which may be decided
    /// per value (EXPR-14).
    pub(crate) fn with_record_types(mut self, types: Vec<crate::result_type::ResultType>) -> Self {
        self.record_types = Some(types);
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
                .insert(column.name.clone(), (field, Some(column.kind)));
        }
    }
    pub(crate) fn joined(&self, name: &str) -> Option<&JoinedRelation> {
        self.joined.get(name)
    }
    /// The one type of the field `name` reads, where it has one.
    pub(crate) fn type_of(&self, name: &str) -> Option<ColumnType> {
        self.result_type(name).column_type()
    }

    /// The result type of the field `name` reads (EXPR-14): a record field's
    /// as its donor derives it, which may be decided per value, and any
    /// other field's kind; unknown where no source decides it.
    pub(crate) fn result_type(&self, name: &str) -> crate::result_type::ResultType {
        use crate::result_type::ResultType;
        let record = |slot: usize, column_type| {
            self.record_types
                .as_ref()
                .map_or(ResultType::declared(column_type), |types| {
                    types.get(slot).copied().unwrap_or(ResultType::UNKNOWN)
                })
        };
        if let Some(binding) = self.bind(name) {
            return match binding {
                Binding::Output {
                    column,
                    column_type,
                } => record(column, column_type),
                Binding::Dataset {
                    dataset: 0,
                    field,
                    column_type,
                } => record(field, column_type),
                Binding::Dataset { column_type, .. } => ResultType::declared(column_type),
            };
        }
        ResultType::declared(name.split_once('.').and_then(|(relation, field)| {
            self.joined(relation)?
                .fields
                .get(field)
                .and_then(|(_, kind)| *kind)
        }))
    }

    /// Row-template implicit joins match raw driver fields, independently of output derivations.
    pub(crate) fn with_row_joins(mut self, driver: &str, groups: Option<&[String]>) -> Self {
        self.row_joins = Some(RowJoinScope {
            driver: driver.into(),
            groups: groups.map(<[String]>::to_vec),
        });
        self
    }

    /// Retain pure donor schemas and each relation's current-output equality
    /// metadata: the input's declared key, as each relation gives it.
    pub(crate) fn with_joined(
        mut self,
        relations: &[(usize, Dataset<'_>, std::vec::Vec<&str>, bool)],
    ) -> Result<Self, Error> {
        for (source, relation, names, pair) in relations {
            let inference = crate::key_relation::declared(&self, names, relation.fields)?;
            let retained_keys = names
                .iter()
                .map(|&name| {
                    (
                        String::from(name),
                        self.outputs.get(name).map(|&(column, _)| column),
                    )
                })
                .collect::<std::vec::Vec<_>>();
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
                            // A layout slot; the kind a check reads is `fields`'s.
                            kind: field.column_type.unwrap_or(ColumnType::Str),
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
                    keys: retained_keys,
                    schema,
                    pair: *pair,
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
    /// The name of every relation the catalog holds a record of: its datasets
    /// and the relations it joins.
    pub(crate) fn relation_names(&self) -> impl Iterator<Item = &str> {
        self.datasets
            .keys()
            .chain(self.joined.keys())
            .map(String::as_str)
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
    /// Whether `name` is an output declaration, whatever its type.
    pub(crate) fn has_output(&self, name: &str) -> bool {
        self.outputs.contains_key(name)
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
            record_types: None,
            outputs: output_index,
            datasets: dataset_index,
            selected: BTreeMap::new(),
            joined: BTreeMap::new(),
            row_joins: None,
        })
    }

    /// The catalog of a pair relation's joined record (LOOKUP-27): `outputs`
    /// are the record's fields, and each side's name reads its own fields of
    /// that one record, each `(field, position, type)` at its position in
    /// the record, so a qualified name of either side reads the record.
    pub(crate) fn compile_joined(
        outputs: &[Field<'_>],
        sides: &[JoinedSide<'_>],
    ) -> Result<Self, Error> {
        let mut catalog = Self::compile(outputs, &[])?;
        for &(name, fields) in sides {
            if name.is_empty() || fields.iter().any(|(field, ..)| field.is_empty()) {
                return Err(Error::EmptyName);
            }
            let mut indexed = BTreeMap::new();
            for &(field, position, kind) in fields {
                if indexed
                    .insert(String::from(field), (position, kind))
                    .is_some()
                {
                    return Err(Error::DuplicateField);
                }
            }
            if catalog
                .datasets
                .insert(
                    String::from(name),
                    DatasetCatalog {
                        index: 0,
                        fields: indexed,
                    },
                )
                .is_some()
            {
                return Err(Error::DuplicateDataset);
            }
        }
        Ok(catalog)
    }

    /// Resolve a declared relation without inventing a stored field, including empty schemas.
    pub fn bind_relation(&self, name: &str) -> Option<usize> {
        self.datasets.get(name).map(|dataset| dataset.index)
    }

    /// Resolve bare outputs or a dataset plus its exact stored suffix (NAME-3 and NAME-7).
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
    /// qualified spelling suggested for an unresolved bare name (NAME-3).
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
        let Some(&(column, _)) = self.outputs.get(name) else {
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
            .zip(self.type_of(name))
            .filter(|(expected, actual)| expected != actual)
            .map(|(expected, actual)| Diagnostic::IncompatibleInputType { expected, actual }))
    }
}
