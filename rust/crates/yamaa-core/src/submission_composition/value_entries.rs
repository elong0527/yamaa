//! Value-level entries composed per column (SUB-12, SUB-13).
use super::*;

/// Compose every column's value-level entries (SUB-12, SUB-13): the static
/// checks of each column's entries, then one definition per entry carrying
/// what the entry takes from the column (`ValueEntry::describe`).
pub(super) fn compose_values(
    admission: &ColumnAdmission<'_>,
    declarations: &spec::Declarations,
    selected: &mut [Column],
    path: &str,
    findings: &mut Vec<Finding>,
    bound: &mut BTreeSet<String>,
) {
    let doc = admission.doc;
    let columns = &declarations
        .columns()
        .expect("admitted dataset columns")
        .value;
    let names = admission.names;
    let selected_names: Vec<String> = selected
        .iter()
        .map(|column| column.declaration.name.value.clone())
        .collect();
    let positions = Positions::of(selected_names.iter().map(String::as_str));
    for base in columns {
        let name = &base.name.value;
        let Some(metadata) = base.submission.as_ref() else {
            continue;
        };
        if metadata.value_level.is_empty()
            || !names.output.contains(name)
            || (admission.reproduced && reproduced::omits(base))
        {
            continue;
        }
        let entries = values::entries(doc, base.node);
        let written = entries.iter().map(|entry| (entry.id, &entry.clause));
        let mode = if admission.reproduced {
            spec::SubmissionMode::Reproduced
        } else {
            spec::SubmissionMode::Validated
        };
        let static_findings = values::admit(written, name, mode, |where_column| {
            declared_column(
                columns,
                (&names.declared, &names.output),
                admission.catalogue,
                admission.reproduced,
                where_column,
            )
        });
        for finding in &static_findings {
            let text = |value: &str| crate::json::Value::String(value.into());
            let (code, suffix, detail, mut context) = match *finding {
                values::Finding::DuplicateId { id } => (
                    ConditionCode::SubmissionValueDuplicateId,
                    format!("{}.id", values::path(name, id)),
                    id.to_owned(),
                    vec![("entry", text(id)), ("identifier", text(id))],
                ),
                values::Finding::Where {
                    id,
                    column,
                    ref fault,
                } => {
                    // A clause that names no column has no where column (SUB-12).
                    let where_column = column.map_or(crate::json::Value::Null, text);
                    let mut context = vec![
                        ("entry", text(id)),
                        ("reason", text(fault.reason())),
                        ("where_column", where_column),
                    ];
                    // The refused where values, in declared order.
                    context.extend(fault.values().map(|refused| {
                        let refused = refused.iter().map(|value| text(value)).collect();
                        ("values", crate::json::Value::Array(refused))
                    }));
                    // The comparator whose value count is refused.
                    context.extend(
                        fault
                            .comparator()
                            .map(|comparator| ("comparator", text(comparator))),
                    );
                    (
                        ConditionCode::SubmissionValueWhereInvalid,
                        column.map_or_else(
                            || format!("{}.where", values::path(name, id)),
                            |column| format!("{}.where.{column}", values::path(name, id)),
                        ),
                        fault.reason().to_owned(),
                        context,
                    )
                }
                // A finding carries one path: the later clause's, naming the
                // earlier entry beside it.
                values::Finding::Overlap { id, other } => (
                    ConditionCode::SubmissionValueOverlap,
                    format!("{}.where", values::path(name, id)),
                    other.to_owned(),
                    vec![(
                        "entries",
                        crate::json::Value::Array(vec![text(other), text(id)]),
                    )],
                ),
            };
            context.push(("column", text(name)));
            let mut finding = Finding::cause(code, &format!("{path}.{suffix}"), &detail);
            finding
                .context
                .extend(context.into_iter().map(|(key, value)| (key.into(), value)));
            findings.push(finding);
        }
        if !static_findings.is_empty() {
            continue;
        }
        let Some(position) = positions.first(name) else {
            continue;
        };
        for entry in &metadata.value_level {
            let id = &entry.id.value;
            let entry_path = format!("{path}.{}", values::path(name, id));
            let clause = entry
                .clause
                .iter()
                .map(|term| {
                    positions.first(&term.column.value).and_then(|index| {
                        values::where_term(
                            index,
                            term.comparator,
                            term.values
                                .iter()
                                .map(|value| value.value.clone())
                                .collect(),
                            selected[index].runtime_type(),
                        )
                    })
                })
                .collect::<Option<Vec<_>>>();
            let effective = entry.describe(base, mode);
            let first_finding = findings.len();
            if !admission.reproduced {
                reproduced::entry_fields(entry, &entry_path, name, findings);
            }
            if !admission.reproduced
                && admission.keys.contains(name)
                && entry.metadata.mandatory.as_ref().is_some_and(|m| !m.value)
            {
                findings.push(
                    Finding::cause(
                        ConditionCode::SubmissionKeyMandatory,
                        &format!("{entry_path}.mandatory"),
                        name,
                    )
                    .with("value", false),
                );
            }
            // An entry without origins of its own answers with the column's,
            // which the column's own refutation already covers (SUB-13).
            let derivations = match admission.tree_column(name) {
                Some(column) if !entry.metadata.origins.is_empty() => values::levels(column)
                    .iter()
                    .find(|level| level.id.name == *id)
                    .map_or_else(Vec::new, |level| {
                        values::entry_expressions(admission.tree, column, level)
                    }),
                _ => Vec::new(),
            };
            let value = admission.admit(
                &effective,
                &entry_path,
                &derivations,
                Level::Value,
                findings,
                bound,
            );
            for finding in &mut findings[first_finding..] {
                for (key, value) in [("column", name.as_str()), ("entry", id.as_str())] {
                    finding
                        .context
                        .insert(key.into(), crate::json::Value::String(value.into()));
                }
            }
            if let (Some(value), Some(clause)) = (value, clause) {
                let text = |field: &Option<spec::Field<String>>| {
                    field.as_ref().map(|field| field.value.clone())
                };
                selected[position].values.push(ValueDefinition {
                    id: id.clone(),
                    clause,
                    path: entry_path,
                    column: value,
                    received: Received {
                        oid: text(&entry.oid),
                        name: text(&entry.name),
                        where_oid: text(&entry.where_oid),
                    },
                });
            }
        }
    }
}
