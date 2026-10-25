//! Callback arguments borrow the same admitted implicit-join metadata as scalar sources.
use super::*;
use crate::{
    bound_expression::Read,
    dataset::{AggregateSource, JoinedRead, MatchKey},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JoinedFinding {
    UnknownField,
    CurrentKey {
        identifier: String,
        finding: crate::reference_scope::Finding,
    },
    /// An input that declares no `key` (LOOKUP-1), or a pair relation,
    /// which declares none (LOOKUP-27).
    UndeclaredKey {
        dataset: String,
        pair: bool,
    },
    /// A declared key column the input does not store, as `INPUT.COLUMN`
    /// (LOOKUP-5).
    DonorKey {
        identifier: String,
    },
    /// A declared key column the specification declares no column of
    /// (LOOKUP-5).
    MatchValue {
        identifier: String,
    },
    KeyType {
        source: String,
        expected: ColumnType,
        actual: ColumnType,
    },
    UnavailableKey {
        column: usize,
    },
}
impl JoinedFinding {
    /// The diagnostic at the read `path` when the join's key does not
    /// resolve: no key (LOOKUP-1, LOOKUP-5), a declared
    /// column (LOOKUP-5), or a pair whose types cannot compare (LOOKUP-9).
    /// Every other cause is the reading site's own.
    pub(crate) fn key_diagnostic(&self, path: &str) -> Option<crate::diagnostic::Diagnostic> {
        use crate::diagnostic::lookup::{self, Owner};
        Some(match self {
            Self::UndeclaredKey { dataset, pair } => {
                lookup::undeclared_input_key(path, dataset, false, *pair)
            }
            Self::DonorKey { identifier } => {
                lookup::donor_key_reference(path, Owner::Unnamed, identifier)
            }
            Self::MatchValue { identifier } => {
                lookup::match_value_reference(path, Owner::Unnamed, identifier)
            }
            Self::KeyType {
                source,
                expected,
                actual,
            } => lookup::key_type(
                path,
                Owner::Unnamed,
                source,
                (*expected).into(),
                (*actual).into(),
            ),
            Self::UnknownField | Self::CurrentKey { .. } | Self::UnavailableKey { .. } => {
                return None
            }
        })
    }
}
type JoinedReadValue = (Read, Option<ColumnType>, Vec<usize>);
type JoinedResolution = Result<Option<Result<JoinedReadValue, JoinedFinding>>, Error>;
impl Catalog {
    pub(crate) fn joined_read(&self, name: &str, available: Option<&[usize]>) -> JoinedResolution {
        let Some((owner, field)) = name.split_once('.') else {
            return Ok(None);
        };
        let Some(donor) = self.joined.get(owner) else {
            return Ok(None);
        };
        let Some(&(column, kind)) = donor.fields.get(field) else {
            return Ok(Some(Err(JoinedFinding::UnknownField)));
        };
        let no_keys = || JoinedFinding::UndeclaredKey {
            dataset: owner.into(),
            pair: donor.pair,
        };
        let mut record_keys = None;
        let row_pairs;
        let matched = if let Some(scope) = &self.row_joins {
            use crate::reference_scope::{self, Phase, Reach, Scope};
            let groups = scope
                .groups
                .as_ref()
                .map(|names| names.iter().map(String::as_str).collect::<Vec<_>>());
            let mut pairs = Vec::new();
            let mut records = Vec::new();
            for (index, (name, output_column)) in donor.keys.iter().enumerate() {
                // Every column of a declared key applies (LOOKUP-1, LOOKUP-5).
                let Some(&(_, actual)) = donor.fields.get(name) else {
                    return Ok(Some(Err(JoinedFinding::DonorKey {
                        identifier: std::format!("{owner}.{name}"),
                    })));
                };
                if output_column.is_none() {
                    return Ok(Some(Err(JoinedFinding::MatchValue {
                        identifier: name.clone(),
                    })));
                }
                let identifier = std::format!("{}.{}", scope.driver, name);
                let findings = reference_scope::validate(
                    self,
                    &identifier,
                    None,
                    Scope {
                        drivers: &[&scope.driver],
                        current_driver: false,
                        reach: Reach::Scalar,
                        joined: false,
                        phase: Phase::Row {
                            group_by: groups.as_deref(),
                        },
                    },
                )?;
                if let Some(&finding) = findings.first() {
                    return Ok(Some(Err(JoinedFinding::CurrentKey {
                        identifier,
                        finding,
                    })));
                }
                let Some(Binding::Dataset { field, .. }) = self.bind(&identifier) else {
                    return Err(Error::InvalidAvailableOutput);
                };
                // A pair whose either type is not decided is not compared.
                if let Some((expected, actual)) =
                    self.type_of(&identifier)
                        .zip(actual)
                        .filter(|(expected, actual)| {
                            !crate::result_type::ResultType::from(*expected)
                                .comparable((*actual).into())
                        })
                {
                    return Ok(Some(Err(JoinedFinding::KeyType {
                        source: name.clone(),
                        expected,
                        actual,
                    })));
                }
                pairs.push(index);
                records.push(field);
            }
            if pairs.is_empty() {
                return Ok(Some(Err(no_keys())));
            }
            record_keys = Some(records);
            row_pairs = pairs;
            &row_pairs
        } else {
            match &donor.inference {
                crate::key_relation::Inference::Keys { keys } => keys,
                crate::key_relation::Inference::NoApplicableKeys => {
                    return Ok(Some(Err(no_keys())))
                }
                crate::key_relation::Inference::Incompatible {
                    key,
                    expected,
                    actual,
                } => {
                    return Ok(Some(Err(JoinedFinding::KeyType {
                        source: donor.keys[*key].0.clone(),
                        expected: *expected,
                        actual: *actual,
                    })))
                }
                crate::key_relation::Inference::UnknownColumn { key } => {
                    return Ok(Some(Err(JoinedFinding::DonorKey {
                        identifier: std::format!("{owner}.{}", donor.keys[*key].0),
                    })))
                }
                crate::key_relation::Inference::UnknownValue { key } => {
                    return Ok(Some(Err(JoinedFinding::MatchValue {
                        identifier: donor.keys[*key].0.clone(),
                    })))
                }
            }
        };
        let mut keys = Vec::new();
        let mut dependencies = Vec::new();
        for &index in matched {
            let (name, output_column) = &donor.keys[index];
            let output_column = output_column.ok_or(Error::InvalidAvailableOutput)?;
            if record_keys.is_none()
                && available.is_some_and(|available| !available.contains(&output_column))
            {
                return Ok(Some(Err(JoinedFinding::UnavailableKey {
                    column: output_column,
                })));
            }
            let source_column = donor
                .fields
                .get(name)
                .ok_or(Error::InvalidAvailableOutput)?
                .0;
            keys.push(MatchKey {
                source_column,
                output_column,
            });
            if record_keys.is_none() {
                dependencies.push(output_column);
            }
        }
        let source = if donor.source == 0 {
            AggregateSource::Primary
        } else {
            AggregateSource::Secondary(donor.source - 1)
        };
        Ok(Some(Ok((
            Read::Joined(Box::new(JoinedRead {
                source,
                column,
                keys,
                name: owner.into(),
                schema: donor.schema.clone(),
                record_keys,
            })),
            kind,
            dependencies,
        ))))
    }
}
