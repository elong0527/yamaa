//! Authored ODM declarations select fixed source admission before study capture.
use super::*;
use crate::{
    bound_expression::BoundPredicate,
    dataset::{AggregatePhase, BoundOdm, Expression, OdmRead, OdmScope},
    reference_scope::Phase,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub(super) struct Declaration {
    declared: bool,
    dataset: String,
    item: String,
    item_oid: String,
    path: String,
    selectors: [Option<Vec<String>>; 3],
    filter: Option<(String, Result<CompiledExpression, CompileError>)>,
}

pub(super) fn prepare(
    d: &Document,
    payload: usize,
    path: &str,
) -> Result<Declaration, PrepareError> {
    let item = text(d, field(d, payload, "item")?)?;
    let (dataset, item_oid) = item.split_once('.').ok_or(PrepareError::Internal)?;
    let selectors = ["event", "form", "item_group"].map(|name| {
        d.field(payload, name)
            .filter(|&node| !matches!(d.nodes()[node], N::Null))
            .map(|node| {
                sequence(d, node)?
                    .iter()
                    .map(|&node| text(d, node).map(String::from))
                    .collect()
            })
            .transpose()
    });
    let [events, forms, item_groups] = selectors;
    let filter = d
        .field(payload, "filter")
        .filter(|&node| !matches!(d.nodes()[node], N::Null))
        .map(|node| {
            let expression = text(d, node)?;
            let compiled = compile(
                expression,
                ExpressionContext::Predicate,
                &format!("{path}.filter"),
            );
            match &compiled {
                Ok(_) | Err(CompileError::Invalid { .. }) => (),
                Err(error) => {
                    return Err(PrepareError::expression(
                        error.clone(),
                        Some(&format!("{path}.filter")),
                    ))
                }
            }
            Ok((expression.into(), compiled))
        })
        .transpose()?;
    Ok(Declaration {
        declared: mapping(d, field(d, d.root(), "input")?)?
            .iter()
            .any(|&(name, _)| text(d, name).is_ok_and(|name| name == dataset)),
        dataset: dataset.into(),
        item: item.into(),
        item_oid: item_oid.into(),
        path: path.into(),
        selectors: [events?, forms?, item_groups?],
        filter,
    })
}

pub(super) fn bind(
    declaration: &Declaration,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: Phase<'_>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    if !declaration.declared {
        findings.push(BindFinding::UnknownReference {
            path: format!("{}.item", declaration.path),
            name: declaration.item.clone(),
        });
        return Ok(None);
    }
    if declaration.dataset != driver && !matches!(phase, Phase::Column { .. }) {
        findings.push(BindFinding::Odm(
            crate::diagnostic::OdmFinding::InvalidContext {
                path: &declaration.path,
                dataset: &declaration.dataset,
                location: if declaration.path.starts_with("rows[") {
                    crate::diagnostic::OdmLocation::Row
                } else {
                    crate::diagnostic::OdmLocation::Column
                },
            }
            .diagnostic(),
        ));
        return Ok(None);
    }
    let before = findings.len();
    let filter = if let Some((expression, compiled)) = &declaration.filter {
        let plan = match compiled {
            Ok(plan) => plan,
            Err(CompileError::Invalid {
                position, failure, ..
            }) => {
                findings.push(BindFinding::Predicate(failure.diagnostic(
                    ExpressionContext::Predicate,
                    &format!("{}.filter", declaration.path),
                    expression,
                    position.character,
                )));
                return Ok(None);
            }
            Err(error) => return Err(BindError::ExpressionPolicy(error.clone())),
        };
        let mut bindings = Vec::new();
        let mut seen = BTreeSet::new();
        for identifier in plan.identifiers() {
            if !seen.insert(identifier) {
                continue;
            }
            if identifier
                .split_once('.')
                .is_none_or(|(dataset, _)| dataset != declaration.dataset)
            {
                findings.push(BindFinding::Source(source_expressions::SourceFinding::new(
                    crate::diagnostic::ConditionCode::SourceFilterReference,
                    plan.spec_path(),
                    &[
                        ("identifier", identifier),
                        ("dataset", &declaration.dataset),
                    ],
                )));
                continue;
            }
            match catalog.bind(identifier) {
                Some(reference_binding::Binding::Dataset { field, .. }) => bindings.push(Binding {
                    name: identifier.into(),
                    read: Read::Source(field),
                }),
                _ => findings.push(BindFinding::UnknownReference {
                    path: plan.spec_path().into(),
                    name: identifier.into(),
                }),
            }
        }
        if findings.len() == before {
            let typed =
                plan.typed(|name| crate::result_type::ResultType::declared(catalog.type_of(name)));
            Some(BoundPredicate::new(typed, bindings).map_err(BindError::InvalidPredicateBinding)?)
        } else {
            None
        }
    } else {
        None
    };
    if findings.len() != before {
        return Ok(None);
    }
    let scope = match phase {
        Phase::Row {
            group_by: Some(groups),
        } => OdmScope::Groups(
            crate::odm_source::HIERARCHY
                .into_iter()
                .filter(|&column| {
                    groups.iter().any(|name| {
                        *name
                            == format!(
                                "{}.{}",
                                declaration.dataset,
                                crate::odm_source::FIELDS[column]
                            )
                    })
                })
                .collect(),
        ),
        Phase::Row { group_by: None } => OdmScope::Records,
        Phase::Column { .. } => OdmScope::Keys,
    };
    let read = OdmRead {
        dataset: declaration.dataset.clone(),
        item: declaration.item.clone(),
        item_oid: declaration.item_oid.clone(),
        path: declaration.path.clone(),
        row: row.map(String::from),
        phase: if declaration.path.starts_with("rows[") {
            AggregatePhase::RowConstruction
        } else {
            AggregatePhase::Derivation
        },
        scope,
        source: if matches!(phase, Phase::Column { .. }) {
            let reference = format!("{}.ItemOID", declaration.dataset);
            let Some(reference_binding::Binding::Dataset { dataset, .. }) =
                catalog.bind(&reference)
            else {
                return Err(BindError::Internal);
            };
            Some(if dataset == 0 {
                crate::dataset::AggregateSource::Primary
            } else {
                crate::dataset::AggregateSource::Secondary(dataset - 1)
            })
        } else {
            None
        },
        selectors: declaration.selectors.clone(),
        filter,
    };
    BoundOdm::new(read)
        .map(|bound| Some(Expression::Odm(std::boxed::Box::new(bound))))
        .map_err(BindError::InvalidPlan)
}

pub(super) fn contains(d: &Document, node: usize, depth: usize) -> Result<bool, PrepareError> {
    if depth > crate::limits::Limits::SEMANTIC_DEPTH {
        return Err(PrepareError::limit(
            "semantic_depth",
            crate::limits::Limits::SEMANTIC_DEPTH,
            Some(depth),
            None,
        ));
    }
    match &d.nodes()[node] {
        N::Mapping(fields) => {
            if d.field(node, "odm").is_some() {
                return Ok(true);
            }
            for &(key, child) in fields {
                if ["literal", "dict"].contains(&text(d, key)?) {
                    continue;
                }
                if contains(d, child, depth + 1)? {
                    return Ok(true);
                }
            }
        }
        N::Sequence(nodes) => {
            for &child in nodes {
                if contains(d, child, depth + 1)? {
                    return Ok(true);
                }
            }
        }
        _ => (),
    }
    Ok(false)
}

/// ODM scope and filter names depend only on the authored specification.
/// Keep them ahead of source capture, including reads under aggregate derive.
pub(super) fn context_findings(
    spec: &SpecificationDocument,
) -> Result<Vec<PreflightFinding>, PrepareError> {
    use crate::diagnostic::{ConditionCode, OdmFinding, OdmLocation};
    struct Site<'a> {
        payload: usize,
        item: &'a str,
        dataset: &'a str,
        path: String,
        location: OdmLocation,
        column: Option<&'a str>,
        row: Option<usize>,
    }
    #[allow(clippy::too_many_arguments)]
    fn visit<'a>(
        d: &'a Document,
        node: usize,
        path: &str,
        location: OdmLocation,
        column: Option<&'a str>,
        row: Option<usize>,
        depth: usize,

        sites: &mut Vec<Site<'a>>,
    ) -> Result<(), PrepareError> {
        if depth > crate::limits::Limits::SEMANTIC_DEPTH {
            return Err(PrepareError::limit(
                "semantic_depth",
                crate::limits::Limits::SEMANTIC_DEPTH,
                Some(depth),
                Some(path),
            ));
        }

        match &d.nodes()[node] {
            N::Mapping(fields) => {
                if let Some(value) = d.field(node, "value").filter(|_| {
                    fields.iter().all(|&(key, _)| {
                        text(d, key).is_ok_and(|key| ["value", "unconvertible"].contains(&key))
                    })
                }) {
                    let suffix = if d.field(node, "unconvertible").is_some() {
                        ".value"
                    } else {
                        ""
                    };
                    return visit(
                        d,
                        value,
                        &format!("{path}{suffix}"),
                        location,
                        column,
                        row,
                        depth + 1,
                        sites,
                    );
                }
                if let Some(payload) = d.field(node, "odm") {
                    let item = text(d, field(d, payload, "item")?)?;
                    let (dataset, _) = item.split_once('.').ok_or(PrepareError::Internal)?;
                    sites.push(Site {
                        payload,
                        item,
                        dataset,
                        path: format!("{path}.odm"),
                        location,
                        column,
                        row,
                    });
                } else {
                    for &(key, child) in fields {
                        let key = text(d, key)?;
                        if ["literal", "dict"].contains(&key) {
                            continue;
                        }
                        let location = if key == "aggregate" {
                            OdmLocation::Derive
                        } else {
                            location
                        };
                        visit(
                            d,
                            child,
                            &format!("{path}.{key}"),
                            location,
                            column,
                            row,
                            depth + 1,
                            sites,
                        )?;
                    }
                }
            }
            N::Sequence(children) => {
                for (index, &child) in children.iter().enumerate() {
                    visit(
                        d,
                        child,
                        &format!("{path}[{index}]"),
                        location,
                        column,
                        row,
                        depth + 1,
                        sites,
                    )?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    let d = spec.document();
    let root = d.root();
    let inputs = mapping(d, field(d, root, "input")?)?
        .iter()
        .map(|&(name, _)| text(d, name))
        .collect::<Result<Vec<_>, _>>()?;
    let rows = d
        .field(root, "rows")
        .filter(|&node| !matches!(d.nodes()[node], N::Null))
        .map(|node| sequence(d, node))
        .transpose()?
        .unwrap_or(&[]);
    let driver = |row| -> Result<Option<&str>, PrepareError> {
        d.field(row, "dataset")
            .filter(|&node| !matches!(d.nodes()[node], N::Null))
            .map(|node| text(d, node))
            .transpose()
            .map(|driver| driver.or_else(|| (inputs.len() == 1).then(|| inputs[0])))
    };
    let mut sites = Vec::new();

    for &node in sequence(d, field(d, root, "columns")?)? {
        if let Some(derivation) = d
            .field(node, "derivation")
            .filter(|&node| !matches!(d.nodes()[node], N::Null))
        {
            let column = text(d, field(d, node, "name")?)?;
            visit(
                d,
                derivation,
                &format!("columns.{column}.derivation"),
                OdmLocation::Column,
                Some(column),
                None,
                0,
                &mut sites,
            )?;
        }
    }
    for (collection, location) in [
        ("rows", OdmLocation::Row),
        ("intermediates", OdmLocation::Intermediate),
    ] {
        if let Some(nodes) = d
            .field(root, collection)
            .filter(|&node| !matches!(d.nodes()[node], N::Null))
        {
            for (index, &node) in sequence(d, nodes)?.iter().enumerate() {
                if let Some(derivations) = d
                    .field(node, "derivations")
                    .filter(|&node| !matches!(d.nodes()[node], N::Null))
                {
                    for &(name, derivation) in mapping(d, derivations)? {
                        visit(
                            d,
                            derivation,
                            &format!("{collection}[{index}].derivations.{}", text(d, name)?),
                            location,
                            None,
                            Some(index),
                            0,
                            &mut sites,
                        )?;
                    }
                }
            }
        }
    }
    let mut findings = Vec::new();
    for site in sites {
        if !inputs.contains(&site.dataset) {
            findings.push(PreflightFinding::Odm(
                source_expressions::SourceFinding::new(
                    ConditionCode::SourceUnknownReference,
                    &format!("{}.item", site.path),
                    &[("identifier", site.item)],
                )
                .diagnostic(),
            ));
            continue;
        }
        let scoped = match site.location {
            OdmLocation::Intermediate | OdmLocation::Derive => false,
            OdmLocation::Row => {
                driver(rows[site.row.ok_or(PrepareError::Internal)?])? == Some(site.dataset)
            }
            OdmLocation::Column if rows.is_empty() => spec.default_driver() == Some(site.dataset),
            OdmLocation::Column => {
                let mut scoped = true;
                for &row in rows {
                    if d.field(
                        field(d, row, "derivations")?,
                        site.column.ok_or(PrepareError::Internal)?,
                    )
                    .is_none()
                        && driver(row)? != Some(site.dataset)
                    {
                        scoped = false;
                    }
                }
                scoped
            }
        };
        if !scoped {
            findings.push(PreflightFinding::Odm(
                OdmFinding::InvalidContext {
                    path: &site.path,
                    dataset: site.dataset,
                    location: site.location,
                }
                .diagnostic(),
            ));
        }
        if let Some(filter) = d
            .field(site.payload, "filter")
            .filter(|&node| !matches!(d.nodes()[node], N::Null))
        {
            let path = format!("{}.filter", site.path);
            let expression = text(d, filter)?;
            match compile(expression, ExpressionContext::Predicate, &path) {
                Ok(plan) => {
                    let mut seen = BTreeSet::new();
                    for identifier in plan.identifiers() {
                        if !seen.insert(identifier) {
                            continue;
                        }
                        let code = match identifier.split_once('.') {
                            Some((dataset, field))
                                if dataset == site.dataset
                                    && crate::odm_source::FIELDS.contains(&field) =>
                            {
                                continue
                            }
                            Some((dataset, _)) if dataset == site.dataset => {
                                ConditionCode::SourceUnknownReference
                            }
                            _ => ConditionCode::SourceFilterReference,
                        };
                        let fields = [("identifier", identifier), ("dataset", site.dataset)];
                        findings.push(PreflightFinding::Odm(
                            source_expressions::SourceFinding::new(code, &path, &fields)
                                .diagnostic(),
                        ));
                    }
                }
                Err(CompileError::Invalid {
                    position, failure, ..
                }) => {
                    findings.push(PreflightFinding::Odm(failure.diagnostic(
                        ExpressionContext::Predicate,
                        &path,
                        expression,
                        position.character,
                    )));
                }
                Err(_) => (), // Existing expression preparation retains unsupported/policy failures.
            }
        }
    }
    Ok(findings)
}

pub(super) fn inputs(d: &Document) -> Result<BTreeSet<&str>, PrepareError> {
    fn visit<'a>(
        d: &'a Document,
        node: usize,
        depth: usize,
        found: &mut BTreeSet<&'a str>,
    ) -> Result<(), PrepareError> {
        if depth > crate::limits::Limits::SEMANTIC_DEPTH {
            return Err(PrepareError::limit(
                "semantic_depth",
                crate::limits::Limits::SEMANTIC_DEPTH,
                None,
                None,
            ));
        }
        match &d.nodes()[node] {
            N::Mapping(fields) => {
                if let Some(payload) = d.field(node, "odm") {
                    let item = text(d, field(d, payload, "item")?)?;
                    if let Some((dataset, _)) = item.split_once('.') {
                        found.insert(dataset);
                    }
                } else {
                    for &(key, child) in fields {
                        if ["literal", "dict"].contains(&text(d, key)?) {
                            continue;
                        }
                        visit(d, child, depth + 1, found)?;
                    }
                }
            }
            N::Sequence(children) => {
                for &child in children {
                    visit(d, child, depth + 1, found)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    let root = d.root();
    let mut found = BTreeSet::new();
    for &column in sequence(d, field(d, root, "columns")?)? {
        if let Some(derivation) = d
            .field(column, "derivation")
            .filter(|&node| !matches!(d.nodes()[node], N::Null))
        {
            visit(d, derivation, 0, &mut found)?;
        }
    }
    for collection in ["rows", "intermediates"] {
        if let Some(nodes) = d
            .field(root, collection)
            .filter(|&node| !matches!(d.nodes()[node], N::Null))
        {
            for &node in sequence(d, nodes)? {
                if let Some(derivations) = d
                    .field(node, "derivations")
                    .filter(|&node| !matches!(d.nodes()[node], N::Null))
                {
                    for &(_, derivation) in mapping(d, derivations)? {
                        visit(d, derivation, 0, &mut found)?;
                    }
                }
            }
        }
    }
    Ok(found)
}

impl Declaration {
    pub(super) fn check_findings(&self, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
        if let Some((expression, compiled)) = &self.filter {
            static_checks::predicate(
                compiled,
                expression,
                &format!("{}.filter", self.path),
                findings,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_document::Tree;

    #[test]
    fn dictionary_data_and_literal_values_cannot_select_odm_source_admission() {
        for expression in [
            Tree::map(vec![(
                "mapping",
                Tree::map(vec![
                    ("source", Tree::text("SRC.X")),
                    (
                        "dict",
                        Tree::map(vec![("odm", Tree::text("SRC.IT.TEST.OID"))]),
                    ),
                ]),
            )]),
            Tree::map(vec![("literal", Tree::text("SRC.IT.TEST.OID"))]),
        ] {
            let tree = Tree::map(vec![
                ("schema_version", Tree::text("1.0")),
                ("domain", Tree::text("TEST")),
                (
                    "input",
                    Tree::map(vec![(
                        "SRC",
                        Tree::map(vec![("path", Tree::text("source.csv"))]),
                    )]),
                ),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "columns",
                    Tree::List(vec![Tree::map(vec![
                        ("name", Tree::text("ID")),
                        ("type", Tree::text("str")),
                        ("derivation", Tree::map(vec![("value", expression)])),
                    ])]),
                ),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("result.csv")),
                        ("columns", Tree::List(vec![Tree::text("ID")])),
                    ]),
                ),
            ]);
            let mut nodes = Vec::new();
            let root = tree.append(&mut nodes);
            let document = Document::new(nodes, root).unwrap();
            let admitted = SpecificationDocument::admit(document).unwrap().unwrap();
            let document = admitted.document();
            let column = sequence(
                document,
                field(document, document.root(), "columns").unwrap(),
            )
            .unwrap()[0];
            let derivation = field(document, column, "derivation").unwrap();
            assert!(!contains(document, derivation, 0).unwrap());
            assert!(inputs(document).unwrap().is_empty());
            assert!(context_findings(&admitted).unwrap().is_empty());
        }
    }
}
