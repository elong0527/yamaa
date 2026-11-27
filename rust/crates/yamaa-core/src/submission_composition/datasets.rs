//! The section's datasets composed in order: each listed specification's
//! dataset declarations, keys and transport presentation, and its columns.
use super::*;

/// Compose every captured dataset in order, collecting codelist bindings.
pub(super) fn compose_datasets(
    composition: &Composition<'_>,
    inputs: &[Input<'_>],
    findings: &mut Vec<Finding>,
    bound: &mut BTreeSet<String>,
) -> Vec<Dataset> {
    let Composition {
        section,
        guides,
        catalogue,
        study,
        known,
        acrfs,
    } = *composition;
    let family = family_name(section.family);
    let mut datasets = Vec::new();
    let mut ids = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    for (index, input) in inputs.iter().enumerate() {
        let doc = input.document;
        let path = format!("{family}.specs[{index}]");
        // A section lists only specifications that declare its model (SUB-3,
        // D25). A missing `model` or another model fails at the listed
        // specification's `model`, and its dataset composes no further,
        // since every rule after this one reads the model.
        if let Some(finding) = listed_model(input.tree, section.family, &path) {
            findings.push(finding);
            continue;
        }
        reserved_metadata(doc, doc.root(), None, &path, findings);
        let reads = Reads::of_spec(input.tree);
        let reproduced = reproduced::listed(input);
        let id = text(doc, doc.root(), "domain").unwrap_or_default();
        if !ids.insert(id.clone()) || known.contains(id.as_str()) {
            findings.push(
                Finding::cause(ConditionCode::SubmissionDuplicateIdentifier, &path, &id)
                    .with("identifier", id.as_str()),
            );
        }
        if !sources.insert(input.source) || (!reproduced && !artifacts.insert(input.artifact)) {
            findings.push(Finding::cause(
                ConditionCode::SubmissionDuplicateDataset,
                &path,
                &id,
            ));
        }
        transport_presentation(section, doc, index, &path, &id, findings);
        let declarations = match spec::decode(doc, doc.root()) {
            Ok(value) => value,
            Err(_) => {
                findings.push(
                    Finding::cause(
                        ConditionCode::SubmissionMetadataMissing,
                        &path,
                        "invalid closed submission declarations",
                    )
                    .with("field", "submission"),
                );
                continue;
            }
        };
        let Some(dataset) = declarations.dataset() else {
            findings.push(
                Finding::cause(ConditionCode::SubmissionMetadataMissing, &path, &id)
                    .with("field", "submission"),
            );
            continue;
        };
        reproduced::dataset(section, dataset, &path, findings);
        reproduced::codelists(dataset, catalogue, &path, findings, bound);
        let Some(columns) = declarations.columns() else {
            findings.push(
                Finding::cause(ConditionCode::SubmissionMetadataMissing, &path, "columns")
                    .with("field", "columns"),
            );
            continue;
        };
        // A non-standard dataset binds no guide (DEFINE-6); one that also
        // selects a guide fails in `admit_dataset` instead (SUB-4).
        let standard = (!dataset.non_standard.value).then(|| {
            bound_standard(
                section.family,
                guides,
                dataset.standard.as_ref(),
                &format!("{path}.submission.standard"),
                findings,
            )
        });
        admit_dataset(section, known, dataset, &path, &id, findings);
        let tabulation = dataset.class.value.tabulation();
        let output = doc.field(doc.root(), "output");
        let output_columns = output.map_or_else(Vec::new, |out| strings(doc, out, "columns"));
        let keys = strings(doc, doc.root(), "keys");
        let mut names = BTreeSet::new();
        for column in &columns.value {
            if !names.insert(&column.name.value) {
                findings.push(
                    Finding::cause(
                        ConditionCode::SubmissionDuplicateIdentifier,
                        &format!("{path}.columns"),
                        &column.name.value,
                    )
                    .with("identifier", column.name.value.as_str()),
                );
            }
        }
        let names = Names {
            tree: Positions::of(input.tree.columns.iter().map(|c| c.name.name.as_str())),
            declared: Positions::of(columns.value.iter().map(|c| c.name.value.as_str())),
            output: Positions::of(output_columns.iter().map(String::as_str)),
        };
        admit_keys(doc, &names, &keys, &path, findings);
        let mut selected = Vec::new();
        for column in &columns.value {
            reserved_metadata(
                doc,
                column.node,
                Some(&column.name.value),
                &format!("{path}.columns.{}", column.name.value),
                findings,
            );
            if column.submission.is_some() && !names.output.contains(&column.name.value) {
                findings.push(
                    Finding::cause(
                        ConditionCode::SubmissionColumnNotOutput,
                        &format!("{path}.columns.{}.submission", column.name.value),
                        &column.name.value,
                    )
                    .with("column", column.name.value.as_str()),
                );
            }
        }
        let admission = ColumnAdmission {
            reproduced,
            doc,
            tree: input.tree,
            dataset: &id,
            section,
            catalogue,
            known,
            acrfs,
            keys: &keys,
            names: &names,
            reads: &reads,
            tabulation,
            class: dataset.class.value.name(),
        };
        // The first column reproduced metadata omits, and whether the
        // document describes any other output column (DEFINE-30).
        let mut omitted = None;
        let mut described = false;
        for (position, name) in output_columns.iter().enumerate() {
            // OUTPUT-1: an entry that repeats an earlier one, or else names no
            // declared column, fails as checking the specification refuses
            // it, at the entry under the listing, and DEFINE-7 reads no
            // metadata for it.
            let repeated = names.output.repeats(position, name);
            let column = match names.declared.first(name).map(|at| &columns.value[at]) {
                Some(column) if !repeated => column,
                _ => {
                    let name = name.clone();
                    let refusal = if repeated {
                        OutputFinding::DuplicateColumn { position, name }
                    } else {
                        OutputFinding::UndeclaredColumn { position, name }
                    };
                    findings.push(Finding::listed(&path, refusal.diagnostic()));
                    continue;
                }
            };
            let column_path = format!("{path}.columns.{name}.submission");
            if reproduced && reproduced::omitted(column, &column_path, findings) {
                omitted.get_or_insert(column_path);
                continue;
            }
            described = true;
            if !reproduced {
                key_column_mandatory(column, admission.keys, &path, findings);
            }
            // Every origin the column and its entries declare.
            let origins: Vec<_> = column
                .submission
                .iter()
                .flat_map(|metadata| {
                    metadata.origins.iter().chain(
                        metadata
                            .value_level
                            .iter()
                            .flat_map(|entry| &entry.metadata.origins),
                    )
                })
                .collect();
            if !reproduced {
                predecessor_labels(study, column, &origins, &path, findings);
            }
            // The column's origin answers for every construction whose rows an
            // entry declaring its own origin does not all select (SUB-19).
            let derivations = admission.tree_column(name).map_or_else(Vec::new, |column| {
                values::column_expressions(input.tree, column)
            });
            if let Some(column) = admission.admit(
                column,
                &column_path,
                &derivations,
                Level::Column,
                findings,
                bound,
            ) {
                selected.push(column);
            }
        }
        if let Some(first) = omitted.filter(|_| !described) {
            findings.push(reproduced::no_item_ref(&first));
        }
        if reproduced {
            reproduced::orders(&selected, &path, findings);
        }
        redundant_unique(doc, &keys, &path, findings);
        compose_values(
            &admission,
            &declarations,
            &mut selected,
            &path,
            findings,
            bound,
        );
        datasets.push(Dataset {
            id: id.clone(),
            domain: dataset.domain.as_ref().map_or(id, |d| d.value.clone()),
            standard,
            metadata: dataset.clone(),
            columns: selected,
            artifact: input.artifact.into(),
            path: path.clone(),
            href: if reproduced {
                dataset.artifact.as_ref().map(|link| link.value.clone())
            } else {
                input.href.map(String::from)
            },
        });
    }
    datasets
}

/// JSON-5: a transport re-reads the published artifact, so display digits
/// the csv profile writes would become the submitted values, and a declared
/// missing text would be read as a value (CSV-17).
fn transport_presentation(
    section: &ast::Section,
    doc: &Document,
    index: usize,
    path: &str,
    id: &str,
    findings: &mut Vec<Finding>,
) {
    if section.transport.is_none() {
        return;
    }
    let output = doc.field(doc.root(), "output");
    for (field, code, detail) in [
        (
            "decimals",
            ConditionCode::SubmissionTransportDecimals,
            "display precision would become the submitted values",
        ),
        (
            "missing",
            ConditionCode::SubmissionTransportMissingText,
            "the missing text would become a submitted value",
        ),
    ] {
        let Some(declared) = output.and_then(|output| doc.field(output, field)) else {
            continue;
        };
        let mut finding = Finding::cause(code, &format!("{path}.output.{field}"), detail);
        finding
            .context
            .insert("dataset".into(), crate::json::Value::String(id.into()));
        // The artifact the declaration would spell, as `output.path` writes
        // it (CSV-6, CSV-17).
        let written = output.and_then(|output| text(doc, output, "path"));
        finding.context.insert(
            "path".into(),
            written.map_or(crate::json::Value::Null, crate::json::Value::String),
        );
        if let Some(written) = section.specs.get(index) {
            finding.context.insert(
                "specification".into(),
                crate::json::Value::String(written.text.clone()),
            );
        }
        match &doc.nodes()[declared] {
            N::Integer(value) => {
                if let Ok(number) = value.parse() {
                    finding
                        .context
                        .insert("decimals".into(), crate::json::Value::Number(number));
                }
            }
            N::Text(value) => {
                finding
                    .context
                    .insert("missing".into(), crate::json::Value::String(value.clone()));
            }
            _ => {}
        }
        findings.push(finding);
    }
}

/// The declarations of a listed specification's dataset the section reads:
/// its name and label in a regulator's transport (DEFINE-22, XPT-5), reference
/// data that does not repeat, no ADaM `domain`, a class of the section's
/// family, and a comment for a dataset without data, whose references name
/// known documents.
fn admit_dataset(
    section: &ast::Section,
    known: &BTreeSet<&str>,
    dataset: &spec::Dataset,
    path: &str,
    id: &str,
    findings: &mut Vec<Finding>,
) {
    if transport_limits(section) {
        if !dataset_name(id) {
            findings.push(name_finding(path, id, DATASET_NAME));
        }
        if let Some(cause) = label_cause(&dataset.label.value) {
            findings.push(label_finding(
                &format!("{path}.submission.label"),
                id,
                None,
                &dataset.label.value,
                cause,
            ));
        }
    }
    if dataset.reference_data.value && dataset.repeating.value {
        findings.push(
            Finding::cause(
                ConditionCode::SubmissionReferenceRepeating,
                &format!("{path}.submission"),
                id,
            )
            .with("dataset", id),
        );
    }
    if let Some(standard) = dataset
        .standard
        .as_ref()
        .filter(|_| dataset.non_standard.value)
    {
        findings.push(
            Finding::cause(
                ConditionCode::SubmissionNonStandardBound,
                &format!("{path}.submission.standard"),
                id,
            )
            .with("value", standard.value.as_str())
            .with("non_standard", true),
        );
    }
    let model = section_model(section.family).name();
    if let Some(domain) = dataset
        .domain
        .as_ref()
        .filter(|_| section.family == Submission::Adam)
    {
        findings.push(
            Finding::cause(
                ConditionCode::SubmissionAdamField,
                &format!("{path}.submission.domain"),
                id,
            )
            .with("value", domain.value.as_str())
            .with("model", model),
        );
    }
    if dataset.non_standard.value && section.family == Submission::Adam {
        findings.push(
            Finding::cause(
                ConditionCode::SubmissionAdamField,
                &format!("{path}.submission.non_standard"),
                id,
            )
            .with("value", true)
            .with("model", model),
        );
    }
    // The domain build reads the family group from the class alone, so the
    // class must belong to the family this document binds the dataset to.
    let tabulation = dataset.class.value.tabulation();
    if tabulation == (section.family == Submission::Adam) {
        let class = dataset.class.value.name();
        findings.push(
            Finding::cause(
                ConditionCode::SubmissionClassFamily,
                &format!("{path}.submission.class"),
                class,
            )
            .with("value", class)
            .with("model", model),
        );
    }
    if dataset.has_no_data.value && dataset.comment.is_none() {
        findings.push(
            Finding::cause(
                ConditionCode::SubmissionNoDataComment,
                &format!("{path}.submission.comment"),
                id,
            )
            .with("field", "comment"),
        );
    }
    comment_references(
        findings,
        dataset.comment.as_ref(),
        known,
        &format!("{path}.submission.comment"),
    );
}

/// OUTPUT-2: `keys` fails as checking the specification refuses it, with
/// the specification's own refusal at its spec path under the listing:
/// an empty list, and each key an earlier key repeats, no column
/// declares, or `output.columns` leaves out. A missing list is the
/// schema's to refuse.
fn admit_keys(
    doc: &Document,
    names: &Names<'_>,
    keys: &[String],
    path: &str,
    findings: &mut Vec<Finding>,
) {
    let written = doc.field(doc.root(), "keys").map(|node| &doc.nodes()[node]);
    if matches!(written, Some(N::Sequence(items)) if items.is_empty()) {
        findings.push(Finding::listed(
            path,
            PreflightFinding::EmptyKeys.diagnostic(),
        ));
    }
    for (position, key) in keys.iter().enumerate() {
        if keys[..position].contains(key) {
            let refusal = PreflightFinding::DuplicateKey {
                position,
                identifier: key.clone(),
            };
            findings.push(Finding::listed(path, refusal.diagnostic()));
        }
        let refusal = if !names.declared.contains(key) {
            PreflightFinding::UndeclaredKey {
                position,
                column: key.clone(),
            }
            .diagnostic()
        } else if !names.output.contains(key) {
            OutputFinding::InternalKey {
                position,
                name: key.clone(),
            }
            .diagnostic()
        } else {
            continue;
        };
        findings.push(Finding::listed(path, refusal));
    }
}

/// A `unique` over exactly the keys restates their asserted identity.
fn redundant_unique(doc: &Document, keys: &[String], path: &str, findings: &mut Vec<Finding>) {
    let key_set: BTreeSet<_> = keys.iter().collect();
    if let Some(N::Sequence(entries)) = doc
        .field(doc.root(), "verifications")
        .map(|v| &doc.nodes()[v])
    {
        for (index, &entry) in entries.iter().enumerate() {
            let Some(payload) = doc.field(entry, "unique") else {
                continue;
            };
            let columns = strings(doc, payload, "columns");
            if !columns.is_empty() && columns.iter().collect::<BTreeSet<_>>() == key_set {
                findings.push(redundant(
                    ConditionCode::VerificationUniqueRedundant,
                    &format!("{path}.verifications[{index}].unique"),
                    &columns.join(", "),
                    &format!("{path}.keys"),
                ));
            }
        }
    }
}
