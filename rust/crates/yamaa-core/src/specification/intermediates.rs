//! Named stored-input and SELF selections compiled from the admitted document.
use super::*;
use crate::{
    dataset::{Intermediate, IntermediateKey, Keep, OrderTerm, SecondarySource, SourceSelection},
    value::Value,
};

type UniqueDeclarations = Vec<(String, Vec<(String, String)>, Option<String>)>;

#[derive(Debug)]
struct Declaration {
    name: String,
    path: String,
    source: self_relations::Relation,
    keys: Option<Vec<(String, match_values::Declaration)>>,
    filter_text: Option<String>,
    filter: Option<Result<CompiledExpression, CompileError>>,
    order: Vec<(String, bool, bool)>,
    keep: Option<Keep>,
    no_match: Option<Value>,
    require_match: bool,
    range: Option<lookup_range::Declaration>,
    columns: Option<Vec<String>>,
    unique: UniqueDeclarations,
    derivations: Vec<intermediate_derivations::Derivation>,
    driver_prohibited: Vec<String>,
    driver_keys: Option<std::sync::Arc<TableSchema>>,
}
#[derive(Debug, Default)]
pub(super) struct Declarations(std::sync::Arc<Vec<Declaration>>, Option<String>);
type ResolveRef<'a> = &'a (dyn Fn(&str, &str, &mut Vec<BindFinding>) -> Option<usize> + 'a);
type FinishedItem = (
    Option<crate::dataset::RelationPredicate>,
    Vec<OrderTerm>,
    Option<Vec<usize>>,
    Option<crate::dataset::RelationRange>,
    Vec<crate::dataset::IntermediateUnique>,
);

#[allow(clippy::too_many_arguments)]
fn finish_item(
    item: &Declaration,
    source_name: &str,
    schema: &TableSchema,
    donor_kind: &(dyn Fn(usize) -> Option<ColumnType> + '_),
    admits: &(dyn Fn(usize) -> bool + '_),
    resolve: ResolveRef<'_>,
    catalog: &Catalog,
    sources: &[SourceDeclaration],
    schemas: &[&TableSchema],
    driver: usize,
    result: &[Option<Intermediate>],
    secondary: &[SecondarySource],
    pairs: &[IntermediateKey],
    failed: &mut bool,
    derivations: &Option<crate::dataset::IntermediateDerivations>,
    findings: &mut Vec<BindFinding>,
) -> Result<FinishedItem, BindError> {
    let filter = match &item.filter {
        Some(Ok(plan)) => {
            let filter = relation_predicates::bind(
                plan,
                source_name,
                schema,
                &donor_kind,
                matches!(item.source, self_relations::Relation::SelfRows),
                &admits,
                &item.name,
                sources,
                schemas,
                driver,
                // Current source-key cardinality is resolved before
                // selecting donor records. Its failure must precede
                // the filter's operand types.
                !(pairs.iter().any(|key| key.value.is_expression())
                    && pairs.iter().flat_map(|key| key.value.reads()).any(|read| {
                        matches!(
                            read,
                            crate::dataset::MatchRead::Record { .. }
                                | crate::dataset::MatchRead::SecondaryRecord { .. }
                        )
                    })),
                findings,
            )?;
            if filter.is_none() {
                *failed = true;
            }
            filter
        }
        Some(Err(CompileError::Invalid {
            position, failure, ..
        })) => {
            findings.push(BindFinding::Lookup(LookupFinding::grammar(
                &format!("{}.filter", item.path),
                item.filter_text.as_deref().expect("written predicate"),
                position.character,
                failure,
            )));
            // A syntax rejection does not remove the declaration's schema.
            None
        }
        Some(Err(error)) => return Err(BindError::ExpressionPolicy(error.clone())),
        None => None,
    };
    let mut order_by = Vec::new();
    for (index, (name, descending, nulls_first)) in item.order.iter().enumerate() {
        if let Some(column) = resolve(name, &format!("{}.order_by[{index}]", item.path), findings) {
            order_by.push(OrderTerm {
                column,
                descending: *descending,
                nulls_first: *nulls_first,
            });
        } else {
            *failed = true;
        }
    }
    let before = findings.len();
    let columns = intermediate_range::bind_columns(
        &item.columns,
        source_name,
        schema,
        &admits,
        &item.path,
        &item.name,
        findings,
    );
    // A rejected projection retains donor names for subsequent checkpoints.
    // Its owned finding still prevents final plan admission.
    let columns = (findings.len() == before).then_some(columns).flatten();
    let range = item
        .range
        .as_ref()
        .map(|declaration| {
            intermediate_range::bind(
                declaration,
                &item.name,
                schema,
                &donor_kind,
                &admits,
                catalog,
                schemas[driver],
                result,
                secondary,
                findings,
            )
        })
        .transpose()?
        .flatten();
    let mut unique = Vec::new();
    for (path, names, identity) in &item.unique {
        let mut columns = Vec::new();
        for (name, field_path) in names {
            if let Some(column) = schema
                .columns()
                .iter()
                .position(|column| column.name == *name)
                .filter(|&column| admits(column))
            {
                columns.push(column);
            } else {
                let mut finding = LookupFinding::new(
                    field_path,
                    crate::diagnostic::ConditionCode::IntermediateUniqueField,
                );
                finding.text("intermediate", &item.name);
                finding.text("identifier", &format!("{source_name}.{name}"));
                findings.push(BindFinding::Lookup(finding));
            }
        }
        unique.push(crate::dataset::IntermediateUnique {
            path: path.clone(),
            columns,
            identity: identity.clone(),
        });
    }
    for check in &unique {
        if filter
            .as_ref()
            .is_some_and(crate::dataset::RelationPredicate::correlated)
        {
            let mut finding = LookupFinding::new(
                &check.path,
                crate::diagnostic::ConditionCode::IntermediateUniqueCorrelatedFilter,
            );
            finding.text("intermediate", &item.name);
            findings.push(BindFinding::Lookup(finding));
        }
        if derivations
            .as_ref()
            .is_some_and(|derived| !derived.current.is_empty())
        {
            let mut finding = LookupFinding::new(
                &check.path,
                crate::diagnostic::ConditionCode::IntermediateUniqueCorrelatedDerivation,
            );
            finding.text("intermediate", &item.name);
            findings.push(BindFinding::Lookup(finding));
        }
    }
    Ok((filter, order_by, columns, range, unique))
}

#[allow(clippy::too_many_arguments)]
fn bind_one(
    named: &Declarations,
    item: &Declaration,
    sources: &[SourceDeclaration],
    driver: usize,
    schemas: &[&TableSchema],
    output: &TableSchema,
    keys: &[usize],
    self_columns: Option<&[usize]>,
    project: Option<&crate::project_calls::ProjectCalls>,
    tree: &types::Tree,
    secondary: &[SecondarySource],
    result: &[Option<Intermediate>],
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Intermediate>, BindError> {
    let mut failed = false;
    let (schema, source_name) = match item.source {
        self_relations::Relation::Input(index) => (schemas[index], sources[index].name.as_str()),
        self_relations::Relation::SelfRows => (output, "SELF"),
    };
    let mut pending_schema = None;
    let derivations = intermediate_derivations::bind(
        &item.derivations,
        source_name,
        &item.name,
        &mut pending_schema,
        schema,
        matches!(item.source, self_relations::Relation::SelfRows)
            .then_some(self_columns.unwrap_or(&[])),
        sources,
        schemas,
        driver,
        output,
        named,
        result,
        project,
        tree,
        findings,
    )?;
    let schema = pending_schema.as_ref().unwrap_or_else(|| {
        derivations
            .as_ref()
            .map_or(schema, |derived| derived.plan.output())
    });
    let donor_kind = |column| {
        if let Some(derived) = &derivations {
            derived.column_kind(column)
        } else {
            match item.source {
                self_relations::Relation::Input(index) => schemas[index]
                    .columns()
                    .get(column)
                    .map(|column| column.kind),
                self_relations::Relation::SelfRows => {
                    output.columns().get(column).map(|column| column.kind)
                }
            }
        }
    };
    let admits = |column: usize| {
        !matches!(item.source, self_relations::Relation::SelfRows)
            || column >= output.columns().len()
            || self_columns.is_some_and(|columns| columns.contains(&column))
    };
    let resolve = |name: &str, path: &str, findings: &mut Vec<BindFinding>| {
        let bare = name
            .strip_prefix(&format!("{source_name}."))
            .or_else(|| matches!(item.source, self_relations::Relation::SelfRows).then_some(name));
        let field = bare.and_then(|name| {
            schema
                .columns()
                .iter()
                .position(|column| column.name == name)
        });
        let field = field.filter(|&column| admits(column));
        if field.is_none() {
            let suggestion = (!name.contains('.')
                && schema.columns().iter().any(|c| c.name == name))
            .then(|| format!("{source_name}.{name}"));
            findings.push(BindFinding::Lookup(LookupFinding::reference(
                path,
                name,
                Some(&item.name),
                ReferenceCause::Selection,
                suggestion,
            )));
        }
        field
    };
    let output_fields = output
        .columns()
        .iter()
        .map(|column| reference_binding::Field {
            name: &column.name,
            column_type: column.kind,
        })
        .collect::<Vec<_>>();
    let current_sources = std::iter::once(driver)
        .chain((0..sources.len()).filter(|&index| index != driver))
        .collect::<Vec<_>>();
    let source_fields = current_sources
        .iter()
        .map(|&index| {
            schemas[index]
                .columns()
                .iter()
                .map(|column| reference_binding::Field {
                    name: &column.name,
                    column_type: column.kind,
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let datasets = current_sources
        .iter()
        .zip(&source_fields)
        .map(|(&index, fields)| reference_binding::Dataset {
            name: &sources[index].name,
            fields,
        })
        .collect::<Vec<_>>();
    let catalog = Catalog::compile(&output_fields, &datasets).map_err(BindError::Catalog)?;
    let default_keys = if item.keys.is_none() {
        let before_keys = findings.len();
        match intermediate_admission::inferred_keys(
            &catalog,
            output,
            keys,
            schema,
            &donor_kind,
            &admits,
            &item.path,
            &item.name,
            source_name,
            findings,
        )? {
            Some(keys) => keys,
            None if item.driver_keys.is_some() => {
                // A driver traverses donor records, without selecting by output keys.
                findings.truncate(before_keys);
                Vec::new()
            }
            None => {
                return Ok(None);
            }
        }
    } else {
        Vec::new()
    };
    let pairs_declared = item.keys.as_ref().unwrap_or(&default_keys);
    // Result types and donor fields precede expression declarations and reads.
    for (field, declaration) in pairs_declared {
        if let Some(donor) = schema
            .columns()
            .iter()
            .position(|column| column.name == *field)
            .and_then(donor_kind)
        {
            let expected = declaration.result_type(tree, &catalog)?;
            if !expected.comparable(donor.into()) {
                findings.push(BindFinding::Lookup(LookupFinding::key_type(
                    &format!("{}.key", item.path),
                    &item.name,
                    &declaration.display_name(field),
                    expected,
                    donor,
                )));
                failed = true;
            }
        }
    }
    for (field, _) in pairs_declared {
        if !schema
            .columns()
            .iter()
            .enumerate()
            .any(|(index, column)| column.name == *field && admits(index))
        {
            findings.push(BindFinding::Lookup(LookupFinding::reference(
                &format!("{}.key", item.path),
                &format!("{source_name}.{field}"),
                Some(&item.name),
                ReferenceCause::DonorKey,
                None,
            )));
            failed = true;
        }
    }
    let contextual_reader = named.0.iter().any(|reader| {
        reader.derivations.iter().any(|derive| {
            derive.references.iter().any(|reference| {
                reference
                    .split_once('.')
                    .is_some_and(|(owner, _)| owner == item.name)
            })
        })
    });
    let mut pairs = Vec::new();
    for (field, declaration) in pairs_declared {
        let path = if matches!(declaration, match_values::Declaration::Variable(_)) {
            format!("{}.key", item.path)
        } else {
            format!("{}.key.{field}", item.path)
        };
        let context = match_values::Context {
            output,
            driver: &sources[driver].name,
            row: None,
            phase: crate::reference_scope::Phase::Row { group_by: None },
            project,
            tree,
            named,
            secondary,
            intermediates: result,
        };
        let value = if contextual_reader {
            declaration.bind_deferred(&path, &item.name, &catalog, findings, context)?
        } else {
            declaration.bind(&path, &item.name, &catalog, findings, context)?
        };
        match value {
            Some(value) => {
                if let Some(source_column) = schema
                    .columns()
                    .iter()
                    .position(|column| column.name == *field)
                {
                    pairs.push(IntermediateKey {
                        source_column,
                        path,
                        value,
                    });
                }
            }
            None => failed = true,
        }
    }
    let (filter, order_by, columns, range, unique) = finish_item(
        item,
        source_name,
        schema,
        &donor_kind,
        &admits,
        &resolve,
        &catalog,
        sources,
        schemas,
        driver,
        result,
        secondary,
        &pairs,
        &mut failed,
        &derivations,
        findings,
    )?;
    // Range rejection owns its original cause without turning an existing
    // donor field into a second unknown-reference diagnostic.
    Ok((!failed).then(|| Intermediate {
        identifier: item.name.clone(),
        path: item.path.clone(),
        source: match item.source {
            self_relations::Relation::Input(index) if index == driver => {
                crate::dataset::IntermediateSource::Primary {
                    schema: schemas[index].clone(),
                    name: sources[index].name.clone(),
                }
            }
            self_relations::Relation::Input(index) => {
                crate::dataset::IntermediateSource::Input(if index > driver {
                    index - 1
                } else {
                    index
                })
            }
            self_relations::Relation::SelfRows => crate::dataset::IntermediateSource::SelfRows {
                schema: output.clone(),
                columns: self_columns.unwrap_or(&[]).to_vec(),
            },
        },
        keys: pairs,
        filter,
        selection: item.keep.map(|keep| SourceSelection { order_by, keep }),
        no_match: item.no_match.clone(),
        require_match: item.require_match,
        unique,
        derivations,
        pending_schema,

        range,
        columns,
    }))
}

impl Declarations {
    pub(super) fn owns_filter_type_finding(&self, finding: &crate::diagnostic::Diagnostic) -> bool {
        use crate::diagnostic::ConditionCode as C;
        if !matches!(
            finding.code,
            C::PredicateInputType
                | C::PredicateIncompatiblePair
                | C::PredicateExpectedText
                | C::PredicateContainsExpectedText
        ) {
            return false;
        }
        let [path] = finding.spec_paths.as_slice() else {
            return false;
        };
        let Some(path) = path.strip_suffix(".filter") else {
            return false;
        };
        self.0.iter().any(|declaration| declaration.path == path)
    }

    /// Each intermediate's name and path with its derivations.
    pub(super) fn audit_entries(
        &self,
    ) -> impl Iterator<Item = (&str, &str, &[intermediate_derivations::Derivation])> {
        self.0.iter().map(|item| {
            (
                item.name.as_str(),
                item.path.as_str(),
                &item.derivations[..],
            )
        })
    }
    pub(super) fn for_reader(&self, reader: &str) -> Self {
        Self(std::sync::Arc::clone(&self.0), Some(reader.into()))
    }
    pub(super) fn reader(&self) -> Option<&str> {
        self.1.as_deref()
    }
    /// Where the selector at `index` reads `identifier` in its derivations.
    pub(super) fn derivation_path(&self, index: usize, identifier: &str) -> Option<&str> {
        intermediate_derivations::reading_path(&self.0.get(index)?.derivations, identifier)
    }
    /// The `between.value` of the selector at `index`, with its authored path.
    pub(super) fn range_value(&self, index: usize) -> Option<(&str, String)> {
        let item = self.0.get(index)?;
        let range = item.range.as_ref()?;
        Some((range.value.as_str(), format!("{}.between.value", item.path)))
    }
    /// The range of the selector at `index`, with the selector's id.
    pub(super) fn range(&self, index: usize) -> Option<(&str, &lookup_range::Declaration)> {
        let item = self.0.get(index)?;
        Some((item.name.as_str(), item.range.as_ref()?))
    }

    pub fn is_self(&self, index: usize) -> bool {
        self.0
            .get(index)
            .is_some_and(|item| matches!(item.source, self_relations::Relation::SelfRows))
    }
    pub fn self_donor_fields(&self) -> impl Iterator<Item = &str> {
        self.0
            .iter()
            .filter(|item| matches!(item.source, self_relations::Relation::SelfRows))
            .flat_map(|item| {
                item.keys
                    .as_ref()
                    .into_iter()
                    .flatten()
                    .map(|(field, _)| field.as_str())
                    .chain(
                        item.order
                            .iter()
                            .map(|(field, _, _)| field.strip_prefix("SELF.").unwrap_or(field)),
                    )
                    .chain(
                        item.filter
                            .as_ref()
                            .and_then(|filter| filter.as_ref().ok())
                            .into_iter()
                            .flat_map(|plan| plan.identifiers())
                            .filter(|field| !field.contains('.') || field.starts_with("SELF."))
                            .map(|field| field.strip_prefix("SELF.").unwrap_or(field)),
                    )
                    .chain(
                        item.derivations
                            .iter()
                            .flat_map(|derive| &derive.references)
                            .filter(|name| !name.contains('.') || name.starts_with("SELF."))
                            .map(|name| name.strip_prefix("SELF.").unwrap_or(name)),
                    )
                    .chain(item.range.iter().flat_map(|range| range.donor_fields()))
                    .chain(item.columns.iter().flatten().map(String::as_str))
                    .chain(
                        item.unique.iter().flat_map(|(_, columns, _)| {
                            columns.iter().map(|(name, _)| name.as_str())
                        }),
                    )
            })
    }
    pub fn contains(&self, name: &str) -> bool {
        self.0.iter().any(|item| item.name == name)
    }
    /// The input a named intermediate reads; a row it drives reads that input's records.
    pub(super) fn input(&self, name: &str) -> Option<usize> {
        match self.0.iter().find(|item| item.name == name)?.source {
            self_relations::Relation::Input(index) => Some(index),
            self_relations::Relation::SelfRows => None,
        }
    }
    pub(super) fn kept(&self, name: &str) -> bool {
        self.0
            .iter()
            .any(|item| item.name == name && item.keep.is_some())
    }
    pub(super) fn driver_keys(&self, index: usize) -> Option<&TableSchema> {
        self.0.get(index)?.driver_keys.as_deref()
    }
    pub(super) fn driver_findings(
        &self,
        name: &str,
        path: &str,
        item: &Intermediate,
        secondary: &[SecondarySource],
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let declaration = self
            .0
            .iter()
            .find(|declaration| declaration.name == name)
            .ok_or(BindError::Internal)?;
        let mut prohibited = declaration.driver_prohibited.clone();
        if item.source.is_self() {
            prohibited.push("dataset".into());
        }
        if item
            .filter
            .as_ref()
            .is_some_and(crate::dataset::RelationPredicate::correlated)
        {
            prohibited.push("filter".into());
        }
        if item
            .derivations
            .as_ref()
            .is_some_and(|derived| !derived.current.is_empty())
        {
            prohibited.push("derivations".into());
        }
        if !prohibited.is_empty() {
            let mut finding = LookupFinding::new(
                path,
                crate::diagnostic::ConditionCode::IntermediateDriverInvalid,
            );
            finding.text("intermediate", name);
            let fields = prohibited.iter().map(String::as_str).collect::<Vec<_>>();
            finding.texts("fields", &fields);
            findings.push(BindFinding::Lookup(finding));
            return Ok(());
        }
        // A driver is admitted whenever each field it exposes has one result
        // type (EXPR-14): its rows then carry the declared layout.
        if let Some(derived) = &item.derivations {
            let schema = item.source_schema(secondary).ok_or(BindError::Internal)?;
            let unknown = schema
                .columns()
                .iter()
                .enumerate()
                .filter(|(column, _)| {
                    item.admits_column(*column)
                        && *column >= derived.stored_columns
                        && derived.column_kind(*column).is_none()
                })
                .map(|(_, column)| column.name.as_str())
                .collect::<Vec<_>>();
            if !unknown.is_empty() {
                let mut finding = LookupFinding::new(
                    path,
                    crate::diagnostic::ConditionCode::IntermediateDriverType,
                );
                finding.text("intermediate", name);
                finding.texts("columns", &unknown);
                findings.push(BindFinding::Lookup(finding));
            }
        }
        Ok(())
    }
    pub fn reference<'a>(&self, name: &'a str) -> Option<(usize, &'a str)> {
        let (relation, field) = name.split_once('.')?;
        self.0
            .iter()
            .position(|item| item.name == relation)
            .map(|index| (index, field))
    }
    pub(super) fn match_declaration(
        &self,
        name: &str,
        field: &str,
    ) -> Option<&match_values::Declaration> {
        self.0
            .iter()
            .find(|item| item.name == name)?
            .keys
            .as_ref()?
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, declaration)| declaration)
    }
    fn prepare_unique_checks(
        d: &Document,
        id: usize,
        path: &str,
    ) -> Result<UniqueDeclarations, PrepareError> {
        let mut unique = Vec::new();
        if let Some(checks) = d
            .field(id, "verifications")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            let mut identities = std::collections::BTreeSet::new();
            for (check_index, &check) in sequence(d, checks)?.iter().enumerate() {
                let payload = field(d, check, "unique")?;
                let check_path = format!("{path}.verifications[{check_index}].unique");
                let identity = d
                    .field(payload, "id")
                    .map(|id| text(d, id).map(String::from))
                    .transpose()?;
                if let Some(identity) = &identity {
                    if !identities.insert(identity.clone()) {
                        let mut finding = LookupFinding::new(
                            &format!("{check_path}.id"),
                            crate::diagnostic::ConditionCode::VerificationIdentifierRepeated,
                        );
                        finding.text("identifier", identity);
                        return Err(PrepareError::Invalid(vec![PreflightFinding::Scalar(
                            finding.diagnostic(),
                        )]));
                    }
                }
                let columns = sequence(d, field(d, payload, "columns")?)?
                    .iter()
                    .enumerate()
                    .map(|(index, &id)| {
                        text(d, id)
                            .map(|name| (name.into(), format!("{check_path}.columns[{index}]")))
                    })
                    .collect::<Result<_, _>>()?;
                unique.push((check_path, columns, identity));
            }
        }
        Ok(unique)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        d: &Document,
        tree: &crate::ast::Spec,
        sources: &[SourceDeclaration],
        driver: usize,
        output: &TableSchema,
        keys: &[usize],
        project: Option<&functions::Prepared>,
        extraction: &str_extract::Prepared,
        templates: &str_template::Prepared,
        extra: &mut Vec<UnsupportedFeature>,
    ) -> Result<Self, PrepareError> {
        let mut result = Self::default();
        let Some(id) = d
            .field(d.root(), "intermediates")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        else {
            return Ok(result);
        };
        let items = sequence(d, id)?;
        if items.len() > crate::limits::Limits::COLUMNS {
            return Err(PrepareError::limit(
                "intermediates",
                crate::limits::Limits::COLUMNS,
                Some(items.len()),
                None,
            ));
        }
        let findings = checks::lookups::self_findings(tree);
        if !findings.is_empty() {
            return Err(PrepareError::Invalid(findings));
        }
        let findings = checks::lookups::admission_findings(tree);
        if !findings.is_empty() {
            return Err(PrepareError::Invalid(findings));
        }
        let driver_key_schema = std::sync::Arc::new(
            TableSchema::new(
                keys.iter()
                    .map(|&key| output.columns()[key].clone())
                    .collect(),
            )
            .map_err(|_| PrepareError::Internal)?,
        );
        let order = derivation_order(d, items, project, templates)?;
        for index in order {
            let id = items[index];
            let path = format!("intermediates[{index}]");
            let name = text(d, field(d, id, "id")?)?;
            let row_driver = d
                .field(d.root(), "rows")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
                .map(|rows| sequence(d, rows))
                .transpose()?
                .is_some_and(|rows| {
                    rows.iter().any(|&row| {
                        d.field(row, "dataset")
                            .is_some_and(|node| text(d, node).is_ok_and(|driver| driver == name))
                    })
                });
            let driver_keys = row_driver.then(|| std::sync::Arc::clone(&driver_key_schema));
            let range = intermediate_range::prepare(d, id, &path, name)?;
            let columns = intermediate_range::prepare_columns(d, id)?;
            let dataset = text(d, field(d, id, "dataset")?)?;
            let source = if dataset == "SELF" && present(d, d.root(), "rows") {
                self_relations::Relation::SelfRows
            } else if let Some(source) = sources.iter().position(|source| source.name == dataset) {
                self_relations::Relation::Input(source)
            } else {
                reject(extra, "intermediate_dataset", format!("{path}.dataset"));
                continue;
            };
            let keys = if let Some(key) = d
                .field(id, "key")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                if !matches!(d.nodes()[key], N::Mapping(_) | N::Sequence(_)) {
                    reject(extra, "intermediate_key", format!("{path}.key"));
                    continue;
                }
                if matches!(d.nodes()[key], N::Sequence(_)) {
                    let fields = sequence(d, key)?;
                    if fields.len() > crate::limits::Limits::KEYS {
                        return Err(PrepareError::limit(
                            "intermediate_keys",
                            crate::limits::Limits::KEYS,
                            Some(fields.len()),
                            Some(&format!("{path}.key")),
                        ));
                    }
                    Some(
                        fields
                            .iter()
                            .map(|&id| {
                                text(d, id).map(|name| {
                                    (
                                        name.into(),
                                        match_values::Declaration::Variable(name.into()),
                                    )
                                })
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    )
                } else {
                    let fields = mapping(d, key)?;
                    if fields.len() > crate::limits::Limits::KEYS {
                        return Err(PrepareError::limit(
                            "intermediate_keys",
                            crate::limits::Limits::KEYS,
                            Some(fields.len()),
                            Some(&format!("{path}.key")),
                        ));
                    }
                    if fields.is_empty() {
                        reject(extra, "intermediate_key", format!("{path}.key"));
                    }
                    Some(
                        fields
                            .iter()
                            .map(|&(a, b)| {
                                let name = text(d, a)?;
                                Ok((
                                    name.into(),
                                    match_values::Declaration::prepare(
                                        d,
                                        b,
                                        &format!("{path}.key.{name}"),
                                        sources,
                                        driver,
                                        project,
                                        extraction,
                                        templates,
                                        extra,
                                    )?,
                                ))
                            })
                            .collect::<Result<Vec<_>, PrepareError>>()?,
                    )
                }
            } else {
                None
            };
            let mut filter_text = None;
            let filter = if let Some(filter) = d
                .field(id, "filter")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                let expression = text(d, filter)?;
                filter_text = Some(expression.into());
                let compiled = compile(
                    expression,
                    ExpressionContext::Predicate,
                    &format!("{path}.filter"),
                );
                match &compiled {
                    Err(CompileError::Invalid { .. }) | Ok(_) => {}
                    Err(error) => {
                        return Err(PrepareError::expression(
                            error.clone(),
                            Some(&format!("{path}.filter")),
                        ))
                    }
                }
                Some(compiled)
            } else {
                None
            };
            let keep = match d
                .field(id, "keep")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
                .map(|id| text(d, id))
                .transpose()?
            {
                Some("first") => Some(Keep::First),
                Some("last") => Some(Keep::Last),
                None => None,
                _ => return Err(PrepareError::Internal),
            };
            let mut order = Vec::new();
            if let Some(order_id) = d
                .field(id, "order_by")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                let terms = sequence(d, order_id)?;
                if terms.len() > crate::limits::Limits::COLUMNS {
                    return Err(PrepareError::limit(
                        "intermediate_order",
                        crate::limits::Limits::COLUMNS,
                        Some(terms.len()),
                        None,
                    ));
                }
                for &term in terms {
                    order.push(order_term(d, term)?);
                }
            }
            let no_match = d
                .field(id, "no_match")
                .map(|value| literal(d, value, &format!("{path}.no_match")))
                .transpose()?;
            let require_match = requires_match(d, id);
            let unique = Self::prepare_unique_checks(d, id, &path)?;
            let driver_prohibited = [
                "key",
                "between",
                "order_by",
                "keep",
                "no_match",
                "require_match",
            ]
            .into_iter()
            .filter(|name| {
                if *name == "no_match" {
                    d.field(id, name).is_some()
                } else {
                    present(d, id, name)
                }
            })
            .map(String::from)
            .collect();
            let derivations = intermediate_derivations::prepare(
                d, id, &path, dataset, project, extraction, templates, &result,
            )?;
            std::sync::Arc::get_mut(&mut result.0)
                .ok_or(PrepareError::Internal)?
                .push(Declaration {
                    name: name.into(),
                    path,
                    source,
                    keys,
                    filter,
                    filter_text,
                    order,
                    keep,
                    no_match,
                    require_match,
                    range,
                    columns,
                    unique,
                    derivations,
                    driver_prohibited,
                    driver_keys,
                });
        }
        Ok(result)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn bind(
        &self,
        sources: &[SourceDeclaration],
        driver: usize,
        schemas: &[&TableSchema],
        output: &TableSchema,
        keys: &[usize],
        self_columns: Option<&[usize]>,
        project: Option<&crate::project_calls::ProjectCalls>,
        tree: &types::Tree,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(Vec<SecondarySource>, Vec<Option<Intermediate>>), BindError> {
        let secondary = sources
            .iter()
            .zip(schemas)
            .enumerate()
            .filter(|(index, _)| *index != driver)
            .map(|(_, (declaration, schema))| SecondarySource {
                name: declaration.name.clone(),
                schema: (**schema).clone(),
            })
            .collect::<Vec<_>>();
        let mut result = Vec::new();
        for item in self.0.iter() {
            let bound = bind_one(
                self,
                item,
                sources,
                driver,
                schemas,
                output,
                keys,
                self_columns,
                project,
                tree,
                &secondary,
                &result,
                findings,
            )?;
            result.push(bound);
        }
        Ok((secondary, result))
    }
}

/// Donor derivations may read another declared selector in either source order.
/// Analyze their shared extracted references before any selector is bound.
fn derivation_order(
    d: &Document,
    items: &[usize],
    project: Option<&functions::Prepared>,
    templates: &str_template::Prepared,
) -> Result<Vec<usize>, PrepareError> {
    use crate::dependency_analysis;
    let names = items
        .iter()
        .map(|&item| text(d, field(d, item, "id")?))
        .collect::<Result<Vec<_>, _>>()?;
    let indices = names
        .iter()
        .enumerate()
        .map(|(index, &name)| (name, index))
        .collect::<BTreeMap<_, _>>();
    let mut graph = vec![Vec::new(); items.len()];
    let mut sites = vec![Vec::new(); items.len()];
    for (index, &item) in items.iter().enumerate() {
        let Some(derivations) = d
            .field(item, "derivations")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        else {
            continue;
        };
        for &(name, handled) in mapping(d, derivations)? {
            let path = format!("intermediates[{index}].derivations.{}", text(d, name)?);
            let (value, expression_path) = intermediate_derivations::value(d, handled, &path)?;
            let &[(operation, _)] = mapping(d, value)? else {
                return Err(PrepareError::Internal);
            };
            let path = format!("{expression_path}.{}", text(d, operation)?);
            let mut references = Vec::new();
            derivations::donor_references(d, value, project, &mut references, 0)?;
            self_relations::template_references(d, value, templates, &mut references, 0)?;
            for reference in references {
                let Some(dependency) = reference
                    .split_once('.')
                    .and_then(|(name, _)| indices.get(name))
                    .copied()
                else {
                    continue;
                };
                graph[index].push(dependency);
                sites[index].push((dependency, path.clone()));
            }
        }
    }
    let analysis = dependency_analysis::analyze(&graph).map_err(|_| PrepareError::Internal)?;
    if let Some(cycle) = analysis.cycle {
        let mut paths = Vec::new();
        for pair in cycle.windows(2) {
            for (dependency, path) in &sites[pair[0]] {
                if *dependency == pair[1] && !paths.contains(path) {
                    paths.push(path.clone());
                }
            }
        }
        let mut finding = LookupFinding::new(
            "",
            crate::diagnostic::ConditionCode::IntermediateDerivationCycle,
        );
        let names = cycle.iter().map(|&index| names[index]).collect::<Vec<_>>();
        finding.texts("cycle", &names);
        let mut diagnostic = finding.diagnostic();
        diagnostic.spec_paths = paths;
        return Err(PrepareError::Invalid(vec![PreflightFinding::Scalar(
            diagnostic,
        )]));
    }
    Ok(analysis.order)
}
