//! Column admission: the complete inherited column metadata of a base or
//! value-level item, its origins, length and codelist, a key column's
//! `mandatory` (SUB-10), and a declared column as a where clause reads it
//! (SUB-12).
use super::*;

impl ColumnAdmission<'_> {
    /// The column of the tree that `name` declares.
    pub(super) fn tree_column(&self, name: &str) -> Option<&ast::Column> {
        Some(&self.tree.columns[self.names.tree.first(name)?])
    }
    /// SUB-10: the one resolution the domain build also enforces.
    fn mandatory(&self, name: &str, metadata: &spec::ColumnSubmission) -> bool {
        spec::resolve_mandatory(
            self.keys.iter().any(|key| key == name),
            metadata.mandatory.as_ref().map(|field| field.value),
            metadata.core.as_ref().map(|field| field.value),
            self.tabulation,
        )
    }
    pub(super) fn admit(
        &self,
        column: &spec::Column,
        column_path: &str,
        derivations: &[ast::ExprId],
        level: Level,
        findings: &mut Vec<Finding>,
        bound: &mut BTreeSet<String>,
    ) -> Option<Column> {
        if self.reproduced {
            return reproduced::column(self, column, column_path, level, findings, bound);
        }
        let Self {
            section,
            known,
            keys,
            ..
        } = *self;
        let name = &column.name.value;
        let Some(metadata) = &column.submission else {
            findings.push(
                Finding::cause(ConditionCode::SubmissionMetadataMissing, column_path, name)
                    .with("field", "submission"),
            );
            return None;
        };
        reproduced_only_fields(metadata, column_path, name, findings);
        if metadata.origins.is_empty() {
            findings.push(
                Finding::cause(ConditionCode::SubmissionOriginMissing, column_path, name)
                    .with("column", name.as_str()),
            );
            return None;
        }
        if column.label.is_none() {
            findings.push(
                Finding::cause(
                    ConditionCode::SubmissionMetadataMissing,
                    column_path,
                    "label",
                )
                .with("field", "label"),
            );
        }
        // A name and label a regulator's transport cannot carry fail under
        // Submission, and under xpt whatever the context (DEFINE-22, XPT-5).
        if level == Level::Column && transport_limits(section) {
            if !column_name(name) {
                findings.push(name_finding(column_path, name, COLUMN_NAME));
            }
            if let Some(label) = &column.label {
                if let Some(cause) = label_cause(&label.value) {
                    let label_path = column_path
                        .strip_suffix(".submission")
                        .map_or_else(|| format!("{column_path}.label"), |p| format!("{p}.label"));
                    findings.push(label_finding(
                        &label_path,
                        self.dataset,
                        Some(name),
                        &label.value,
                        cause,
                    ));
                }
            }
        }
        let core = metadata.core.as_ref().map(|c| c.value);
        let mandatory = self.mandatory(name, metadata);
        let model = section_model(section.family);
        if core.is_none_or(|core| !core.admitted_by(model))
            || (section.family == Submission::Adam && metadata.mandatory.is_none())
            || (section.family != Submission::Adam
                && core == Some(Core::Req)
                && metadata.mandatory.as_ref().is_some_and(|m| !m.value))
        {
            findings.push(
                Finding::cause(ConditionCode::SubmissionCoreMandatory, column_path, name)
                    .with("column", name.as_str()),
            );
        }
        if let Some(role) = metadata
            .role
            .as_ref()
            .filter(|_| section.family == Submission::Adam)
        {
            findings.push(
                Finding::cause(
                    ConditionCode::SubmissionAdamField,
                    &format!("{column_path}.role"),
                    name,
                )
                .with("column", name.as_str())
                .with("value", role.value.as_str())
                .with("model", model.name()),
            );
        }
        // Each origin is admitted on its own, at its own position,
        // `origins[<i>]` (P-ORIGIN-1).
        let origins = metadata
            .origins
            .iter()
            .enumerate()
            .map(|(index, declared)| {
                let at = format!("{column_path}.origins[{index}]");
                let column = ColumnSite {
                    path: column_path,
                    name,
                    derivations,
                    level,
                };
                self.admit_origin(declared, &at, column, findings)
            })
            .collect();
        // SUB-18: a `Derived` origin anywhere needs the method that derives it.
        if metadata
            .origins
            .iter()
            .any(|declared| declared.kind.value == OriginType::Derived)
            && metadata.method.is_none()
        {
            findings.push(
                Finding::cause(ConditionCode::SubmissionMethodMissing, column_path, name)
                    .with("column", name.as_str()),
            );
        }
        if let Some(spec::Method::Class(class)) = &metadata.method {
            references(
                findings,
                &class.documents.value,
                known,
                &format!("{column_path}.method"),
            );
        }
        comment_references(
            findings,
            metadata.comment.as_ref(),
            known,
            &format!("{column_path}.comment"),
        );
        // SUB-6 and SUB-25: the family decides how a date or datetime travels.
        let adam = section.family == Submission::Adam;
        let column_type = column.column_type.value;
        let data_type = metadata
            .data_type
            .as_ref()
            .map_or(column_type.family_data_type(adam), |d| d.value);
        if !column_type.admits_family_data_type(data_type, adam) {
            findings.push(
                Finding::cause(ConditionCode::SubmissionDataType, column_path, name)
                    .with("column", name.as_str())
                    .with("declared_type", column_type.name())
                    .with("data_type", data_type.name()),
            );
        }
        // SUB-27: an `sdtm` or `send` dataset carries its date and time text
        // in a `str` column, never in a `datetime` or `time` one.
        if !adam
            && level == Level::Column
            && matches!(
                column_type,
                spec::ColumnType::Datetime | spec::ColumnType::Time
            )
        {
            let type_path = column_path
                .strip_suffix(".submission")
                .map_or_else(|| format!("{column_path}.type"), |p| format!("{p}.type"));
            let mut finding =
                Finding::cause(ConditionCode::SubmissionDatetimeColumn, &type_path, name);
            let text = |value: &str| crate::json::Value::String(value.into());
            for (key, value) in [
                ("column", text(name)),
                ("value", text(column_type.name())),
                ("class", text(self.class)),
                (
                    "permitted",
                    crate::json::Value::Array(vec![text("str"), text("date")]),
                ),
            ] {
                finding.context.insert(key.into(), value);
            }
            findings.push(finding);
        }
        let display_format = match column_type.numeric_date_format(adam) {
            Some(fixed) => {
                if let Some(declared) = metadata
                    .display_format
                    .as_ref()
                    .filter(|declared| declared.value != fixed)
                {
                    findings.push(
                        Finding::cause(
                            ConditionCode::SubmissionDisplayFormatConflict,
                            &format!("{column_path}.display_format"),
                            fixed,
                        )
                        .with("column", name.as_str())
                        .with("value", declared.value.as_str())
                        .with("permitted", fixed),
                    );
                }
                Some(fixed.to_string())
            }
            None => metadata.display_format.as_ref().map(|d| d.value.clone()),
        };
        // XPT-9: a numeric variable's declared display format must be a SAS
        // format the transport can carry; only the column's own one is written.
        if level == Level::Column
            && section.transport == Some(ast::Transport::Xpt)
            && matches!(
                column_type,
                spec::ColumnType::Integer | spec::ColumnType::Float
            )
        {
            if let Some(declared) = &metadata.display_format {
                if sas_format(&declared.value).is_none() {
                    let mut finding = Finding::cause(
                        ConditionCode::XptDisplayFormatInvalid,
                        &format!("{column_path}.display_format"),
                        &declared.value,
                    );
                    for (key, value) in [
                        ("dataset", self.dataset),
                        ("column", name.as_str()),
                        ("display_format", declared.value.as_str()),
                    ] {
                        finding
                            .context
                            .insert(key.into(), crate::json::Value::String(value.into()));
                    }
                    findings.push(finding);
                }
            }
        }
        let length = self.admit_length(column, metadata, data_type, column_path, name, findings);
        self.admit_codelist(column, metadata, column_path, level, findings, bound);
        Some(Column {
            declaration: column.clone(),
            data_type,
            length,
            display_format,
            mandatory,
            origins,
            key_sequence: keys.iter().position(|key| key == name).map(|k| k + 1),
            order: None,
            values: Vec::new(),
            transport_length: None,
            has_no_data: false,
            path: column_path.into(),
        })
    }
    /// Admit one declared origin of the column or entry at `column_path`, at
    /// `at`, its own position: its type and source pair (SUB-15), its
    /// description (SUB-16), its annotated case report form reference
    /// (SUB-17) and the derivations its type claims (SUB-19, SUB-20), each
    /// refutation naming the refuted type.
    fn admit_origin(
        &self,
        declared: &spec::OriginMetadata,
        at: &str,
        column: ColumnSite<'_>,
        findings: &mut Vec<Finding>,
    ) -> OriginDefinition {
        let ColumnSite {
            path: column_path,
            name,
            derivations,
            level,
        } = column;
        let Self {
            tree,
            section,
            known,
            acrfs,
            reads,
            ..
        } = *self;
        let kind = declared.kind.value;
        let model = section_model(section.family);
        let source = declared.source.as_ref().map(|s| s.value);
        // D65: a specification takes no `Not Available` origin under SDTM.
        if !kind.admits_source(model, source) {
            let mut finding = Finding::cause(ConditionCode::SubmissionOriginSource, at, name)
                .with("column", name)
                .with("origin_type", kind.name());
            if let Some(source) = source {
                finding = finding.with("origin_source", source.name());
            }
            findings.push(finding.with("model", model.name()));
        }
        let description = declared.description.as_ref().map(|d| d.value.as_str());
        let predecessor = description
            .filter(|_| kind == OriginType::Predecessor)
            .and_then(origin::predecessor);
        if matches!(
            kind,
            OriginType::Other | OriginType::Predecessor | OriginType::NotAvailable
        ) && description.is_none()
        {
            findings.push(
                Finding::cause(ConditionCode::SubmissionOriginDescription, at, name)
                    .with("column", name)
                    .with("origin_type", kind.name()),
            );
        } else if kind == OriginType::Predecessor && predecessor.is_none() {
            // SUB-16: a Predecessor description names `<dataset>.<column>`.
            findings.push(
                Finding::cause(
                    ConditionCode::SubmissionOriginDescription,
                    &format!("{at}.description"),
                    name,
                )
                .with("column", name)
                .with("origin_type", kind.name())
                .with("reason", "predecessor_form"),
            );
        }
        // SUB-19 and SUB-20: the derivations refute the declared type, or are
        // not the copy a Predecessor claims. A Predecessor that is not the
        // named copy is refuted by a bare `source`, which SUB-19 alone admits.
        let refuted = origin::Constructions::of(tree, derivations, reads)
            .refuting(kind)
            .map(|derivation| (derivation, None))
            .or_else(|| {
                predecessor
                    .as_ref()
                    .and_then(|predecessor| copy_failure(tree, derivations, predecessor, reads))
                    .map(|failure| {
                        (
                            origin::classify_expr(tree, derivations[0], reads),
                            Some(failure),
                        )
                    })
            });
        if let Some((derivation, failure)) = refuted {
            let code = match level {
                Level::Column => ConditionCode::SubmissionOriginRefuted,
                Level::Value => ConditionCode::SubmissionValueOriginRefuted,
            };
            let mut finding = Finding::cause(code, column_path, name).with("column", name);
            let text = |value: &str| crate::json::Value::String(value.into());
            finding
                .context
                .insert("origin_type".into(), text(kind.name()));
            if level == Level::Value {
                finding
                    .context
                    .insert("derivation_kind".into(), text(derivation.name()));
            }
            if let Some(failure) = failure {
                finding
                    .context
                    .insert("reason".into(), text(failure.reason()));
            }
            findings.push(finding);
        }
        let mut refs = declared.documents.value.clone();
        if kind == OriginType::Collected
            && matches!(
                source,
                Some(OriginSource::Investigator | OriginSource::Subject)
            )
        {
            // SUB-17: under `Submission` the reference says where in the form
            // the value was collected; only `Other` derives a page-less one.
            let submission = section.context == DefineContext::Submission;
            if acrfs.len() != 1 {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionAnnotatedCrfMissing,
                    at,
                    name,
                ));
            } else if declared.documents.origin.key.is_none() {
                if submission {
                    findings.push(Finding::cause(
                        ConditionCode::SubmissionOriginDocument,
                        at,
                        name,
                    ));
                } else {
                    refs.push(spec::DocumentReference::Plain(spec::Field {
                        value: acrfs[0].id.name.clone(),
                        origin: declared.documents.origin,
                    }));
                }
            } else if refs.is_empty()
                || refs.iter().any(|r| reference_id(r) != acrfs[0].id.name)
                || (submission && !refs.iter().any(paged))
            {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionOriginDocument,
                    &format!("{at}.documents"),
                    name,
                ));
            }
        }
        references(findings, &refs, known, &format!("{at}.documents"));
        OriginDefinition {
            declaration: declared.clone(),
            // SUB-15: an ADaM source is derived, never declared.
            source: if section.family == Submission::Adam && kind != OriginType::Predecessor {
                Some(OriginSource::Sponsor)
            } else {
                source
            },
            documents: refs,
        }
    }
    /// Admit length and significant-digit declarations; return the effective length.
    fn admit_length(
        &self,
        column: &spec::Column,
        metadata: &spec::ColumnSubmission,
        data_type: DataType,
        column_path: &str,
        name: &str,
        findings: &mut Vec<Finding>,
    ) -> Option<String> {
        let maxima: Vec<_> = verification(self.doc, column.node, "max_length")
            .iter()
            .filter_map(|&n| self.doc.field(n, "max"))
            .filter_map(|n| match &self.doc.nodes()[n] {
                N::Integer(value) => Some(value.clone()),
                _ => None,
            })
            .collect();
        // SUB-7: a date or datetime column never declares a length; under
        // `adam` it travels as an eight-byte SAS numeric date (SUB-25).
        let column_type = column.column_type.value;
        // Each finding names the column, and the field it is about, as the
        // specification run does (SUB-7, SUB-8).
        let length_finding = |code, field: &str| {
            let detail = if field == "length" { name } else { field };
            let finding = Finding::cause(code, column_path, detail)
                .with("column", name)
                .with("field", field);
            if field == "length" {
                finding.with("data_type", data_type.name())
            } else {
                finding
            }
        };
        if column_type.temporal() {
            if metadata.length.is_some() {
                findings.push(length_finding(
                    ConditionCode::SubmissionLengthNotApplicable,
                    "length",
                ));
            }
            if metadata.significant_digits.is_some() {
                findings.push(length_finding(
                    ConditionCode::SubmissionLengthNotApplicable,
                    "significant_digits",
                ));
            }
            return column_type
                .numeric_date_format(self.section.family == Submission::Adam)
                .map(|_| "8".into());
        }
        let length = metadata
            .length
            .as_ref()
            .map(|v| v.value.clone())
            .or_else(|| {
                if column_type == spec::ColumnType::String {
                    maxima.first().cloned()
                } else {
                    None
                }
            });
        let takes_length = matches!(
            data_type,
            DataType::Text | DataType::Integer | DataType::Float
        );
        // SUB-7: a text column without `length` takes the published length,
        // which only a transport measures (SUB-24).
        let needs_length =
            takes_length && (data_type != DataType::Text || self.section.transport.is_none());
        if needs_length && length.is_none() {
            findings.push(length_finding(
                ConditionCode::SubmissionLengthMissing,
                "length",
            ));
        }
        if !takes_length && metadata.length.is_some() {
            findings.push(length_finding(
                ConditionCode::SubmissionLengthNotApplicable,
                "length",
            ));
        }
        if length
            .as_ref()
            .is_some_and(|s| s.starts_with('-') || s == "0")
            || metadata
                .significant_digits
                .as_ref()
                .is_some_and(|v| v.value.starts_with('-'))
        {
            findings.push(
                Finding::cause(ConditionCode::SubmissionLengthInvalid, column_path, name)
                    .with("column", name),
            );
        }
        if let Some((declared, max)) = metadata.length.as_ref().and_then(|declared| {
            maxima
                .iter()
                .find(|max| **max != declared.value)
                .map(|max| (declared, max))
        }) {
            findings.push(
                Finding::cause(ConditionCode::SubmissionLengthConflict, column_path, name)
                    .with("column", name)
                    .with("length", integer(&declared.value))
                    .with("max_length", integer(max)),
            );
        }
        if (data_type == DataType::Float) != metadata.significant_digits.is_some() {
            findings.push(length_finding(
                if data_type == DataType::Float {
                    ConditionCode::SubmissionLengthMissing
                } else {
                    ConditionCode::SubmissionLengthNotApplicable
                },
                "significant_digits",
            ));
        }
        length
    }
    /// Bind an admitted codelist declaration and report any binding findings.
    /// A column-level list of items states the column's value set, so an
    /// `allowed_values` verification beside it is redundant (CT-9).
    fn admit_codelist(
        &self,
        column: &spec::Column,
        metadata: &spec::ColumnSubmission,
        column_path: &str,
        level: Level,
        findings: &mut Vec<Finding>,
        bound: &mut BTreeSet<String>,
    ) {
        if let Some(list) = &metadata.codelist {
            bound.insert(list.value.clone());
            let kind = match column.column_type.value {
                spec::ColumnType::String => crate::value::ColumnType::Str,
                spec::ColumnType::Integer => crate::value::ColumnType::Int,
                spec::ColumnType::Float => crate::value::ColumnType::Float,
                spec::ColumnType::Date => crate::value::ColumnType::Date,
                spec::ColumnType::Datetime => crate::value::ColumnType::DateTime,
                spec::ColumnType::Time => crate::value::ColumnType::Time,
            };
            let codelist_path = format!("{column_path}.codelist");
            let allowed = match level {
                Level::Column => verification_paths(
                    self.doc,
                    column.node,
                    "allowed_values",
                    column_path
                        .strip_suffix(".submission")
                        .unwrap_or(column_path),
                ),
                Level::Value => Vec::new(),
            };
            for binding in self.catalogue.bind(&list.value, kind, !allowed.is_empty()) {
                // Each binding finding names the column and the codelist, and
                // a type mismatch both types, as the specification run does
                // (CT-1, CT-8).
                let finding = |code| {
                    Finding::cause(code, &codelist_path, &list.value)
                        .with("column", column.name.value.as_str())
                        .with("codelist", list.value.as_str())
                };
                let finding = match binding {
                    ct::BindingFinding::UnknownCodelist => {
                        finding(ConditionCode::CodelistUnknownBinding)
                    }
                    ct::BindingFinding::TypeMismatch { expected, actual } => {
                        finding(ConditionCode::CodelistBindingTypeMismatch)
                            .with(
                                "codelist_data_type",
                                crate::project_codelist_binding::codelist_data_type(expected),
                            )
                            .with(
                                "column_type",
                                crate::project_codelist_binding::column_type(actual),
                            )
                    }
                    ct::BindingFinding::RedundantAllowedValues => {
                        for path in &allowed {
                            findings.push(redundant(
                                ConditionCode::CodelistAllowedValuesRedundant,
                                path,
                                &column.name.value,
                                &codelist_path,
                            ));
                        }
                        continue;
                    }
                };
                findings.push(finding);
            }
        }
    }
}

/// SUB-10 for one column at composition: a key column declares no
/// `mandatory: false`. Preparation reports a `not_missing` verification that
/// restates a resolution (SUB-11) before composition.
pub(super) fn key_column_mandatory(
    column: &spec::Column,
    keys: &[String],
    path: &str,
    findings: &mut Vec<Finding>,
) {
    let name = &column.name.value;
    let declared = column
        .submission
        .as_ref()
        .and_then(|metadata| metadata.mandatory.as_ref());
    if keys.contains(name) && declared.is_some_and(|mandatory| !mandatory.value) {
        findings.push(
            Finding::cause(
                ConditionCode::SubmissionKeyMandatory,
                &format!("{path}.columns.{name}.submission.mandatory"),
                name,
            )
            .with("column", name.as_str())
            .with("value", false),
        );
    }
}

/// The declared column `name` as a where clause reads it (SUB-12): whether
/// `output` carries it, whether metadata that is `reproduced` omits it from
/// the document (DEFINE-30), its type, and the values its own codelist and
/// its entries' codelists admit. A value-level entry's clause and an analysis
/// dataset's read a where column through this one description.
pub(super) fn declared_column<'c>(
    columns: &[spec::Column],
    (declared, output): (&Positions<'_>, &Positions<'_>),
    catalogue: &'c ct::Catalogue,
    reproduced: bool,
    name: &str,
) -> Option<values::Declared<'c>> {
    let declared = &columns[declared.first(name)?];
    Some(values::Declared {
        output: output.contains(name),
        omitted: reproduced && reproduced::omits(declared),
        kind: runtime_type(declared.column_type.value),
        codelist: declared.submission.as_ref().and_then(|metadata| {
            values::admitted(
                catalogue,
                metadata.codelist.as_ref().map(|list| list.value.as_str()),
                metadata
                    .value_level
                    .iter()
                    .filter_map(|entry| entry.metadata.codelist.as_ref())
                    .map(|list| list.value.as_str()),
            )
        }),
    })
}
