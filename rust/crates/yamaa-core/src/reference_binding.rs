//! Owned reference catalogs and output-reference validation, without data access.
use alloc::{collections::BTreeMap, string::String, vec};

use crate::value::ColumnType;

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

/// Trusted catalog/query policies, independent of language error conditions.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub outputs: usize,
    pub datasets: usize,
    pub fields: usize,
    pub catalog_bytes: usize,
    pub reference_bytes: usize,
}

impl Default for Limits {
    /// Bound owned metadata and individual lookups without granting access to data.
    fn default() -> Self {
        Self {
            outputs: 4096,
            datasets: 256,
            fields: 65_536,
            catalog_bytes: 1_048_576,
            reference_bytes: 65_536,
        }
    }
}

/// Rejected metadata or resource policy; unresolved language names are results.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Limit {
        resource: &'static str,
        limit: usize,
        required: usize,
    },
    SizeOverflow,
    EmptyName,
    DuplicateOutput,
    DuplicateDataset,
    DuplicateField,
    InvalidAvailableOutput,
    InvalidCandidateDataset,
    QualifiedOutputQuery,
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
    outputs: BTreeMap<String, (usize, ColumnType)>,
    datasets: BTreeMap<String, DatasetCatalog>,
    limits: Limits,
}

#[derive(Debug)]
struct DatasetCatalog {
    index: usize,
    fields: BTreeMap<String, (usize, ColumnType)>,
}

/// Enforce a named budget without collapsing a resource failure into a language error.
fn limit(resource: &'static str, required: usize, allowed: usize) -> Result<(), Error> {
    if required > allowed {
        Err(Error::Limit {
            resource,
            limit: allowed,
            required,
        })
    } else {
        Ok(())
    }
}

impl Catalog {
    /// Inspect literal stored-field visibility, retaining an absent relation as distinct metadata.
    pub(crate) fn has_stored_field(&self, dataset: &str, field: &str) -> Option<bool> {
        self.datasets
            .get(dataset)
            .map(|dataset| dataset.fields.contains_key(field))
    }

    /// Admit resource budgets and nonempty names before allocating reusable name indexes.
    pub fn compile(
        outputs: &[Field<'_>],
        datasets: &[Dataset<'_>],
        limits: Limits,
    ) -> Result<Self, Error> {
        limit("outputs", outputs.len(), limits.outputs)?;
        limit("datasets", datasets.len(), limits.datasets)?;
        let fields = datasets.iter().try_fold(0usize, |count, dataset| {
            count
                .checked_add(dataset.fields.len())
                .ok_or(Error::SizeOverflow)
        })?;
        limit("fields", fields, limits.fields)?;
        let names = outputs
            .iter()
            .map(|field| field.name)
            .chain(datasets.iter().flat_map(|dataset| {
                core::iter::once(dataset.name).chain(dataset.fields.iter().map(|field| field.name))
            }));
        let bytes = names.clone().try_fold(0usize, |bytes, name| {
            bytes.checked_add(name.len()).ok_or(Error::SizeOverflow)
        })?;
        limit("catalog_bytes", bytes, limits.catalog_bytes)?;
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
            outputs: output_index,
            datasets: dataset_index,
            limits,
        })
    }

    /// Resolve bare outputs or a dataset plus its exact stored suffix (REQ-0103/1265).
    /// A dotted suffix is a field only when literally present; it is never an ODM path.
    pub fn bind(&self, name: &str) -> Result<Option<Binding>, Error> {
        limit("reference_bytes", name.len(), self.limits.reference_bytes)?;
        Ok(if let Some((qualifier, field)) = name.split_once('.') {
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
        })
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
        limit("reference_bytes", name.len(), self.limits.reference_bytes)?;
        if name.contains('.') {
            return Err(Error::QualifiedOutputQuery);
        }
        if let Some(available) = available {
            limit("available_outputs", available.len(), self.limits.outputs)?;
            if available.iter().any(|&index| index >= self.outputs.len()) {
                return Err(Error::InvalidAvailableOutput);
            }
        }
        limit("candidate_datasets", candidates.len(), self.limits.datasets)?;
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
