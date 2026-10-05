//! Intermediate field visibility and correlated donor scope over compiler metadata.
use alloc::{collections::BTreeSet, vec::Vec};

use crate::reference_binding::{Catalog, Error};

/// The intermediate's normalized backing relation, without records or execution authority.
#[derive(Clone, Copy, Debug)]
pub enum Source<'a> {
    Dataset(&'a str),
    SelfFields(&'a [&'a str]),
}

/// Metadata for one planned intermediate; an empty readable list means unrestricted visibility.
#[derive(Clone, Copy, Debug)]
pub struct Intermediate<'a> {
    pub source: Source<'a>,
    pub derived: &'a [&'a str],
    pub readable: &'a [&'a str],
    pub dependencies: &'a [&'a str],
}

/// One donor-side read. A missing target already has its own planning failure.
#[derive(Clone, Copy, Debug)]
pub struct Read<'a> {
    pub reader: &'a str,
    pub target_name: &'a str,
    pub target: Option<Intermediate<'a>>,
    pub field: &'a str,
    pub donor_dataset: &'a str,
    pub visible: &'a [&'a str],
}

/// Findings in reference order; hosts attach original names, paths and requirements.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Finding {
    UnknownField,
    SelfPhase,
    UnavailableDependency { dependency: usize },
}

/// Written metadata budgets, independent of language diagnostics.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub entries: usize,
    pub context_bytes: usize,
}

impl Default for Limits {
    /// Bound indexing and repeated names before checking even an otherwise skipped read.
    fn default() -> Self {
        Self {
            entries: 65_536,
            context_bytes: 1_048_576,
        }
    }
}

/// Account for the original entries, including duplicates, before creating an index.
fn admit<'a>(mut names: impl Iterator<Item = &'a str>, limits: Limits) -> Result<(), Error> {
    let (entries, bytes) = names.try_fold((0usize, 0usize), |(entries, bytes), name| {
        Ok::<_, Error>((
            entries.checked_add(1).ok_or(Error::SizeOverflow)?,
            bytes.checked_add(name.len()).ok_or(Error::SizeOverflow)?,
        ))
    })?;
    for (resource, required, limit) in [
        ("intermediate_entries", entries, limits.entries),
        ("intermediate_context_bytes", bytes, limits.context_bytes),
    ] {
        if required > limit {
            return Err(Error::Limit {
                resource,
                limit,
                required,
            });
        }
    }
    Ok(())
}

/// Visit normalized names without building temporary owned context.
fn names<'a>(target: Intermediate<'a>) -> impl Iterator<Item = &'a str> {
    let (dataset, fields) = match target.source {
        Source::Dataset(dataset) => (Some(dataset), &[][..]),
        Source::SelfFields(fields) => (None, fields),
    };
    dataset
        .into_iter()
        .chain(fields.iter().copied())
        .chain(target.derived.iter().copied())
        .chain(target.readable.iter().copied())
        .chain(target.dependencies.iter().copied())
}

/// A missing backing dataset is distinct from a known relation missing a stored field.
fn visible(catalog: &Catalog, field: &str, target: Intermediate<'_>) -> Option<bool> {
    let stored = match target.source {
        Source::Dataset(dataset) => catalog.has_stored_field(dataset, field),
        Source::SelfFields(fields) => Some(fields.contains(&field)),
    };
    stored.map(|stored| {
        (stored || target.derived.contains(&field))
            && (target.readable.is_empty() || target.readable.contains(&field))
    })
}

/// REQ-0125: visible stored/derived fields intersect an optional declared columns list.
/// A missing input relation already has its own diagnostic and adds no field finding.
pub fn validate_field(
    catalog: &Catalog,
    field: &str,
    target: Intermediate<'_>,
    limits: Limits,
) -> Result<Option<Finding>, Error> {
    admit(core::iter::once(field).chain(names(target)), limits)?;
    Ok((visible(catalog, field, target) == Some(false)).then_some(Finding::UnknownField))
}

/// REQ-1263: another intermediate must read input-backed records using this donor's fields.
/// Self-reads defer to shared cycle analysis; phase failures suppress field/dependency findings.
pub fn validate_read(
    catalog: &Catalog,
    read: Read<'_>,
    limits: Limits,
) -> Result<Vec<Finding>, Error> {
    admit(
        [
            read.reader,
            read.target_name,
            read.field,
            read.donor_dataset,
        ]
        .into_iter()
        .chain(read.visible.iter().copied())
        .chain(read.target.into_iter().flat_map(names)),
        limits,
    )?;
    let mut findings = Vec::new();
    if read.reader == read.target_name {
        return Ok(findings);
    }
    let Some(target) = read.target else {
        return Ok(findings);
    };
    if matches!(target.source, Source::SelfFields(_)) {
        findings.push(Finding::SelfPhase);
        return Ok(findings);
    }
    // Donor validation treats a missing backing relation as having no stored
    // fields. It can still expose a derived field; ordinary reads above defer.
    let readable = visible(catalog, read.field, target).unwrap_or_else(|| {
        target.derived.contains(&read.field)
            && (target.readable.is_empty() || target.readable.contains(&read.field))
    });
    if !readable {
        findings.push(Finding::UnknownField);
    }
    let visible: BTreeSet<_> = read.visible.iter().copied().collect();
    for (dependency, name) in target.dependencies.iter().enumerate() {
        let supplied = match name.split_once('.') {
            Some((qualifier, field)) => qualifier == read.donor_dataset && visible.contains(field),
            None => visible.contains(name),
        };
        if !supplied {
            findings.push(Finding::UnavailableDependency { dependency });
        }
    }
    Ok(findings)
}
