//! Qualified stored-field scope and grouping rules over immutable catalog metadata.

use crate::{
    reference_binding::{Binding, Catalog, Error},
    value::ColumnType,
};

/// The operation's normalized access mode; this does not authorize table access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reach {
    Scalar,
    Record,
    Relation,
    Declared,
}

/// Group membership supplied after host normalization and phase selection.
#[derive(Clone, Copy, Debug)]
pub enum Phase<'a> {
    Row { group_by: Option<&'a [&'a str]> },
    Column { groups: &'a [&'a [&'a str]] },
}

/// Context for one direct qualified reference. Named-intermediate traversal is separate.
#[derive(Clone, Copy, Debug)]
pub struct Scope<'a> {
    pub drivers: &'a [&'a str],
    pub current_driver: bool,
    pub reach: Reach,
    pub joined: bool,
    pub phase: Phase<'a>,
}

/// Core-selected findings in observable order; hosts attach paths and authored requirements.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Finding {
    DriverMismatch,
    UnknownField,
    RowPhase,
    RowGroup,
    ColumnGroup,
    IncompatibleInputType {
        expected: ColumnType,
        actual: ColumnType,
    },
}

/// Preserve driver/existence priority, join suppression, and grouping-before-type findings.
/// Scope failures do not suppress a subsequent type finding; unresolved names do.
pub fn validate(
    catalog: &Catalog,
    name: &str,
    expected: Option<ColumnType>,
    scope: Scope<'_>,
) -> Result<Vec<Finding>, Error> {
    let Some((qualifier, _)) = name.split_once('.') else {
        return Err(Error::BareQualifiedQuery);
    };
    // No data or host effects occur here; the result is consumed only after
    // driver priority.
    let bound = catalog.bind(name);
    let mut findings = Vec::new();
    if scope.current_driver
        && (scope.drivers.is_empty() || scope.drivers.iter().any(|driver| *driver != qualifier))
    {
        findings.push(Finding::DriverMismatch);
        return Ok(findings);
    }
    let Some(Binding::Dataset {
        column_type: actual,
        ..
    }) = bound
    else {
        findings.push(Finding::UnknownField);
        return Ok(findings);
    };
    let is_driver = scope.drivers.contains(&qualifier);
    if matches!(scope.phase, Phase::Column { .. })
        && !matches!(scope.reach, Reach::Declared | Reach::Relation)
        && !scope.joined
        && !is_driver
    {
        // The applicable-key prepass already owns this join failure.
        return Ok(findings);
    }
    match scope.phase {
        Phase::Row { group_by } => {
            let exempt = scope.joined
                || matches!(scope.reach, Reach::Declared | Reach::Record)
                || (scope.reach == Reach::Scalar && !is_driver);
            if !exempt {
                if scope.drivers.first().copied() != Some(qualifier) {
                    findings.push(Finding::RowPhase);
                } else if scope.reach == Reach::Scalar
                    && group_by.is_some_and(|group| !group.contains(&name))
                {
                    findings.push(Finding::RowGroup);
                }
            }
        }
        Phase::Column { groups } => {
            if scope.reach == Reach::Scalar && groups.iter().any(|group| !group.contains(&name)) {
                findings.push(Finding::ColumnGroup);
            }
        }
    }
    if let Some(expected) = expected.filter(|expected| *expected != actual) {
        findings.push(Finding::IncompatibleInputType { expected, actual });
    }
    Ok(findings)
}
