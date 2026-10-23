//! Named intermediates and row drivers: SELF declarations (LOOKUP-7),
//! intermediate admission (LOOKUP-3 to LOOKUP-10), driver admission (ROW-2),
//! the first row template's SELF reads, and the reads a column completed for
//! a driver intermediate's rows cannot address.
use super::*;
use crate::diagnostic::{
    lookup::{self, Owner},
    ConditionCode as C, Diagnostic,
};
use crate::json::Value as JsonValue;

/// SELF is a reserved donor relation and requires row templates.
pub(in crate::specification) fn self_findings(spec: &Spec) -> Vec<PreflightFinding> {
    let mut findings = Vec::new();
    for item in &spec.intermediates {
        if item.dataset.name != "SELF" {
            continue;
        }
        let path = format!("{}.dataset", spec.cite(item.meta));
        let (code, paths, context) = if spec.inputs.iter().any(|input| input.name.name == "SELF") {
            (
                C::IntermediateIdentifierCollision,
                vec![path, "input.SELF".into()],
                vec![("identifier".into(), JsonValue::String("SELF".into()))],
            )
        } else if !has_rows(spec) {
            (
                C::LookupSelectionReference,
                vec![path],
                vec![
                    (
                        "intermediate".into(),
                        JsonValue::String(item.id.name.clone()),
                    ),
                    ("identifier".into(), JsonValue::String("SELF".into())),
                ],
            )
        } else {
            continue;
        };
        findings.push(PreflightFinding::Scalar(Diagnostic {
            code,
            spec_paths: paths,
            context: context.into_iter().collect(),
            source_span: None,
            operand_route: None,
        }));
    }
    findings
}

/// The fields a row driver intermediate writes that it may not declare
/// (ROW-2), in this order. Each counts once written, whatever its value; an
/// absence answer counts even when written null. Binding refuses the same
/// list.
pub(in crate::specification) fn driver_prohibited(
    spec: &Spec,
    item: &ast::Intermediate,
) -> Vec<&'static str> {
    // The absence answer, written `absent`.
    let answer = item
        .absent
        .as_ref()
        .map(|answer| spec.paths.segment(answer.meta.path));
    let mut fields = ["key", "between", "order_by", "keep", "nearest"]
        .into_iter()
        .filter(|&field| item.written.contains(field))
        .collect::<Vec<_>>();
    if let Some(ast::Segment::Field(answer)) = answer {
        fields.push(answer);
    }
    fields.extend(
        ["require_match"]
            .into_iter()
            .filter(|&field| item.written.contains(field)),
    );
    fields
}

/// Without a relation change or `require_match`, an intermediate whose
/// absence answer is missing, by default or written as missing, reads what
/// the implicit join reads (LOOKUP-4): one whose `key` lists its input's
/// declared key (D71). A written field changes the relation even when it is
/// empty, such as `derivations: {}`.
fn rename_only(spec: &Spec, item: &ast::Intermediate) -> bool {
    let joins_as_input = spec.keys_as_input(item);
    let changes_relation = [
        "between",
        "filter",
        "order_by",
        "keep",
        "nearest",
        "columns",
        "derivations",
        "verifications",
    ]
    .iter()
    .any(|field| item.written.contains(field));
    joins_as_input
        && !changes_relation
        && !item.require_match
        && item
            .absent
            .as_ref()
            .is_none_or(|answer| answer.value == ast::Literal::Missing)
}

/// Declaration failures that precede source access and key-type binding:
/// identifier collisions, the dataset read, empty and repeated keys, a
/// rename-only lookup, two absence policies, an order or a position beside
/// a nearest selection, and an unpaired selection.
pub(in crate::specification) fn admission_findings(spec: &Spec) -> Vec<PreflightFinding> {
    let mut findings = Vec::new();
    let mut identifiers = BTreeMap::<&str, String>::new();
    let inputs = |name: &str| spec.inputs.iter().any(|input| input.name.name == name);
    for item in &spec.intermediates {
        let path = spec.cite(item.meta);
        let name = item.id.name.as_str();
        let dataset = item.dataset.name.as_str();
        if let Some(previous) = identifiers.get(name).cloned().or_else(|| {
            inputs(name)
                .then(|| format!("input.{name}"))
                .or_else(|| (name == spec.domain.name).then(|| "domain".into()))
        }) {
            findings.push(PreflightFinding::Scalar(Diagnostic {
                code: C::IntermediateIdentifierCollision,
                spec_paths: vec![format!("{path}.id"), previous],
                context: [("identifier".into(), JsonValue::String(name.into()))].into(),
                source_span: None,
                operand_route: None,
            }));
        } else {
            identifiers.insert(name, format!("{path}.id"));
        }
        if !(inputs(dataset) || dataset == "SELF" && has_rows(spec)) {
            let mut finding =
                LookupFinding::new(&format!("{path}.dataset"), C::IntermediateDatasetReference);
            finding.text("identifier", dataset);
            finding.text("intermediate", name);
            findings.push(PreflightFinding::Scalar(finding.diagnostic()));
        }
        // A 1.0 intermediate states its own key, unless it drives a row
        // template, which declares none (LOOKUP-3, ROW-2; D23, D63). The 1.0
        // front end refuses it first; this keeps every route to binding safe.
        if spec.lacks_key(item) {
            findings.push(PreflightFinding::Scalar(lookup::key_required(&format!(
                "{}.key",
                render(spec, item.meta)
            ))));
        }
        let empty = match &item.key {
            Some(JoinKey::Pairs { pairs, .. }) => pairs.is_empty(),
            Some(JoinKey::Columns { columns, .. }) => columns.is_empty(),
            _ => false,
        };
        if empty {
            findings.push(PreflightFinding::Scalar(lookup::empty_key(
                &format!("{path}.key"),
                Owner::Intermediate(name),
                dataset,
            )));
        }
        // A list pairs each column with its same-named value once (LOOKUP-5).
        if let Some(JoinKey::Columns { columns, .. }) = &item.key {
            for (position, column) in columns.iter().enumerate() {
                if columns[..position]
                    .iter()
                    .any(|prior| prior.name == column.name)
                {
                    let mut finding = LookupFinding::new(
                        &format!("{path}.key[{position}]"),
                        C::IntermediateRepeatedKey,
                    );
                    finding.text("intermediate", name);
                    finding.text("dataset", dataset);
                    finding.text("identifier", &column.name);
                    finding.text("expected", "each column once");
                    findings.push(PreflightFinding::Scalar(finding.diagnostic()));
                }
            }
        }
        if rename_only(spec, item) {
            let mut finding =
                LookupFinding::new(&render(spec, item.meta), C::IntermediateRenameOnly);
            finding.text("intermediate", name);
            finding.text("dataset", dataset);
            findings.push(PreflightFinding::Scalar(finding.diagnostic()));
        }
        // Asserting a match and answering its absence are two absence policies.
        // A null answer is the missing value it answers (D18), which the
        // lowering records written.
        let answer = item.absent.as_ref().filter(|answer| {
            matches!(spec.paths.segment(answer.meta.path),
                ast::Segment::Field(field) if item.written.contains(field))
        });
        if let Some(answer) = answer.filter(|_| item.require_match) {
            findings.push(PreflightFinding::Scalar(lookup::absence_conflict(
                render(spec, answer.meta),
                format!("{path}.require_match"),
                Owner::Intermediate(name),
            )));
        }
        // A nearest selection chooses by distance, so neither an order nor a
        // position may choose beside it (LOOKUP-26): each field written
        // beside it, then `nearest`, in specification order. A field so
        // refused is not also unpaired.
        if let Some(nearest) = &item.nearest {
            let mut beside = ["order_by", "keep"]
                .into_iter()
                .filter(|&field| item.written.contains(field))
                .map(|field| format!("{path}.{field}"))
                .collect::<Vec<_>>();
            if !beside.is_empty() {
                beside.push(render(spec, nearest.meta));
                findings.push(PreflightFinding::Scalar(lookup::nearest_beside_selection(
                    beside,
                    Owner::Intermediate(name),
                )));
            }
            continue;
        }
        // A written `order_by` needs a `keep`, even when it has no term; an
        // `order_by` with no term orders nothing, so a `keep` beside it has
        // no order to choose by, as for a source's selection (LOOKUP-10).
        let ordered = !item.order_by.is_empty();
        for (declared, missing, unpaired) in [
            (
                "order_by",
                "keep",
                item.written.contains("order_by") && !item.written.contains("keep"),
            ),
            (
                "keep",
                "order_by",
                item.written.contains("keep") && !ordered,
            ),
        ] {
            if unpaired {
                findings.push(PreflightFinding::Scalar(lookup::selection_unpaired(
                    &path,
                    Owner::Intermediate(name),
                    declared,
                    missing,
                )));
            }
        }
    }
    findings
}

/// The input whose records build every row template's rows: a template's
/// input, or the input its driver intermediate reads. A driver intermediate
/// declares no relation of its own (ROW-2): each field it writes that it may
/// not declare refuses it, as binding refuses it.
pub(in crate::specification) fn common_driver(spec: &Spec) -> Result<&str, PrepareError> {
    let mut common = None;
    for template in &spec.rows.templates {
        // Preflight has already refused a template without a driver.
        let name = template_driver(spec, template).ok_or(PrepareError::Internal)?;
        let input = |name: &str| spec.inputs.iter().any(|input| input.name.name == name);
        let name = if input(name) {
            name
        } else {
            let item = spec
                .intermediates
                .iter()
                .find(|item| item.id.name == name)
                .ok_or(PrepareError::Internal)?;
            let mut prohibited = driver_prohibited(spec, item);
            if !input(&item.dataset.name) {
                prohibited.push("dataset");
            }
            if !prohibited.is_empty() {
                let mut finding = LookupFinding::new(
                    &format!("{}.dataset", spec.cite(template.meta)),
                    C::IntermediateDriverInvalid,
                );
                finding.text("intermediate", name);
                finding.texts("fields", &prohibited);
                return Err(PrepareError::Invalid(vec![PreflightFinding::Scalar(
                    finding.diagnostic(),
                )]));
            }
            item.dataset.name.as_str()
        };
        common.get_or_insert(name);
    }
    common.ok_or(PrepareError::Internal)
}

/// The first row template's rows exist before any row is completed, so a
/// read of SELF there, directly or through the `between` values of further
/// intermediates, finds no completed row (LOOKUP-7). `rows` are that
/// template's row-phase derivations in construction order, and `filter` its
/// filter when it is ungrouped: a grouped template's filter reads no
/// qualified name at all (ROW-6), which binding reports. Every alternative
/// is checked, selected or not.
pub(in crate::specification) fn first_template(
    spec: &Spec,
    rows: &[ExprId],
    filter: Option<ExprId>,
    calls: bool,
    named: &intermediates::Declarations,
) -> Result<Vec<PreflightFinding>, PrepareError> {
    let mut findings = Vec::new();
    let mut stack = rows
        .iter()
        .rev()
        .map(|&root| Visit::Expression(root))
        .collect::<Vec<_>>();
    while let Some(visit) = stack.pop() {
        let e = match visit {
            Visit::Names(names, path) => {
                for name in names {
                    register(named, &name, &path, &mut findings);
                }
                continue;
            }
            Visit::Expression(e) => e,
        };
        match &spec.expr(e).kind {
            ExprKind::Case(arms) => {
                // Each `when`'s names, then its alternative, in written order.
                let mut visits = Vec::new();
                for arm in arms {
                    if let Some(ExprKind::Formula(formula)) =
                        arm.condition.map(|condition| &spec.expr(condition).kind)
                    {
                        if let Ok(compiled) = &formula.compiled {
                            visits.push(Visit::Names(
                                compiled.identifiers().map(String::from).collect(),
                                render(spec, formula.text.meta),
                            ));
                        }
                    }
                    visits.push(Visit::Expression(arm.result));
                }
                stack.extend(visits.into_iter().rev());
            }
            ExprKind::FirstAvailable { sources, .. } => {
                for &source in sources {
                    if let Some(name) = variable(spec, source) {
                        register(named, name, &path(spec, source), &mut findings);
                    }
                }
            }
            ExprKind::Scalar { op, operands, .. } if op.cites_operands() => {
                for operand in operands {
                    if let Some(name) = name(spec, operand.expr) {
                        register(named, name, &path(spec, operand.expr), &mut findings);
                    }
                }
            }
            kind => {
                let scan = reads::Scan {
                    relation: true,
                    windows: true,
                };
                let names = reads::names(spec, e, scan, calls)?;
                let path = match kind {
                    ExprKind::Formula(formula) => render(spec, formula.text.meta),
                    _ => path(spec, e),
                };
                for name in names {
                    register(named, &name, &path, &mut findings);
                }
            }
        }
    }
    if let Some(ExprKind::Formula(formula)) = filter.map(|filter| &spec.expr(filter).kind) {
        if let Ok(plan) = &formula.compiled {
            let mut seen = std::collections::BTreeSet::new();
            for name in plan.identifiers() {
                if seen.insert(name) {
                    register(named, name, &render(spec, formula.text.meta), &mut findings);
                }
            }
        }
    }
    Ok(findings)
}

/// One step of [`first_template`]'s walk.
enum Visit {
    Expression(ExprId),
    Names(Vec<String>, String),
}

/// A read of a first-template SELF lookup, followed through the `between`
/// value of each intermediate it reads, as the selector runs for the same
/// row. Report the read, then each value in the order followed.
fn register(
    named: &intermediates::Declarations,
    name: &str,
    path: &str,
    findings: &mut Vec<PreflightFinding>,
) {
    let mut paths = vec![path.to_owned()];
    let mut followed = Vec::new();
    let mut reached = name.to_owned();
    loop {
        let Some((index, _)) = named.reference(&reached) else {
            return;
        };
        if named.is_self(index) {
            break;
        }
        // Binding reports a value that reaches back to its own selector.
        let Some((value, between)) = named.range_value(index) else {
            return;
        };
        if followed.contains(&index) {
            return;
        }
        followed.push(index);
        paths.push(between);
        reached = value.into();
    }
    findings.push(PreflightFinding::Scalar(Diagnostic {
        code: C::IntermediateSelfPhaseBoundary,
        source_span: None,
        operand_route: None,
        spec_paths: paths,
        context: [
            ("identifier".into(), JsonValue::String(reached)),
            (
                "available_phase".into(),
                JsonValue::String("completed_prior_row_template".into()),
            ),
            (
                "required_phase".into(),
                JsonValue::String("row_construction".into()),
            ),
        ]
        .into_iter()
        .collect(),
    }));
}

/// Whether a column completed for rows the named intermediate `driver`
/// builds reads a field of it that the column phase cannot address. That
/// phase reads each row's driver record from the input the intermediate
/// reads, at the position of the record that built the row (#2829). A field
/// the intermediate derives has no record there, and a column holding a
/// qualified aggregate keeps the whole-dataset scope in which the
/// intermediate is no driver. Preparation refuses such a read before any
/// data is read, so `check` and `domain` report it alike.
pub(in crate::specification) fn unaddressable(
    spec: &Spec,
    root: ExprId,
    driver: &str,
    aggregate: bool,
    calls: bool,
) -> Result<bool, PrepareError> {
    let Some(item) = spec
        .intermediates
        .iter()
        .find(|item| item.id.name == driver)
    else {
        return Ok(false);
    };
    let scan = reads::Scan {
        relation: false,
        windows: true,
    };
    let mut names = reads::names(spec, root, scan, calls)?;
    for position in positions(spec, root) {
        // The conditional operands the shared reference scan skips, such as
        // an `impute`'s bounds and fallback.
        if let ExprKind::Scalar { op, operands, .. } = &spec.expr(position).kind {
            names.extend(
                operands
                    .iter()
                    .filter(|operand| op.conditional(operand.slot))
                    .filter_map(|operand| name(spec, operand.expr).map(String::from)),
            );
        }
    }
    Ok(names.iter().any(|name| {
        name.split_once('.').is_some_and(|(owner, field)| {
            owner == driver
                && (aggregate
                    || item
                        .bindings
                        .iter()
                        .any(|binding| binding.name.name == field))
        })
    }))
}

/// Whether a qualified aggregate occurs at a position below `root`.
pub(in crate::specification) fn aggregates(spec: &Spec, root: ExprId) -> bool {
    positions(spec, root)
        .into_iter()
        .any(|e| matches!(spec.expr(e).kind, ExprKind::Aggregate(_)))
}
