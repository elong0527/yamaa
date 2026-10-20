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
    /// The `key` path of a 1.0 row driver, which declares no key, so a read
    /// of it as a lookup is refused there (LOOKUP-3, ROW-2).
    key_required: Option<String>,
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
    // A 0.5 lookup without `key` infers the applicable keys. A 1.0
    // intermediate without `key` drives rows and is never a keyed lookup
    // (ROW-2, LOOKUP-3); preparation refused every other.
    let default_keys = if item.keys.is_none() && tree.spec().header.surface.infers_join_keys() {
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

    /// Each intermediate's name, in the order the intermediates are derived.
    pub(super) fn names(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|item| item.name.as_str())
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
    /// Where a read of the 1.0 row driver at `index` as a keyed lookup is
    /// refused: its `key`, which it does not declare (LOOKUP-3, ROW-2).
    pub(super) fn key_required(&self, index: usize) -> Option<&str> {
        self.0.get(index)?.key_required.as_deref()
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
    /// The intermediates a derivation may classify as named: all of them,
    /// or inside an intermediate's own derivations those derived before it.
    fn earlier(&self) -> &[Declaration] {
        match &self.1 {
            Some(reader) => {
                let end = self
                    .0
                    .iter()
                    .position(|item| item.name == *reader)
                    .unwrap_or(self.0.len());
                &self.0[..end]
            }
            None => &self.0,
        }
    }
    /// Whether `name` is an intermediate derived before the reading one.
    pub(super) fn earlier_contains(&self, name: &str) -> bool {
        self.earlier().iter().any(|item| item.name == name)
    }
    /// Whether `name` is a `keep`-declared intermediate derived before the
    /// reading one.
    pub(super) fn earlier_kept(&self, name: &str) -> bool {
        self.earlier()
            .iter()
            .any(|item| item.name == name && item.keep.is_some())
    }
    fn unique_checks(
        spec: &ast::Spec,
        item: &ast::Intermediate,
    ) -> Result<UniqueDeclarations, PrepareError> {
        let mut unique = Vec::new();
        let mut identities = std::collections::BTreeSet::new();
        for check in &item.checks {
            if let Some(identity) = &check.id {
                if !identities.insert(identity.text.clone()) {
                    let mut finding = LookupFinding::new(
                        &checks::render(spec, identity.meta),
                        crate::diagnostic::ConditionCode::VerificationIdentifierRepeated,
                    );
                    finding.text("identifier", &identity.text);
                    return Err(PrepareError::Invalid(vec![PreflightFinding::Scalar(
                        finding.diagnostic(),
                    )]));
                }
            }
            let columns = check
                .columns
                .iter()
                .map(|column| (column.name.clone(), checks::render(spec, column.meta)))
                .collect();
            unique.push((
                checks::render(spec, check.meta),
                columns,
                check.id.as_ref().map(|identity| identity.text.clone()),
            ));
        }
        Ok(unique)
    }

    /// Each named intermediate, in the order its derivations need, admitted
    /// before any source is read. A dataset or key outside what binding
    /// supports is collected in `extra`.
    pub fn prepare(
        spec: &ast::Spec,
        sources: &[SourceDeclaration],
        output: &TableSchema,
        keys: &[usize],
        project: Option<&functions::Prepared>,
        extra: &mut Vec<UnsupportedFeature>,
    ) -> Result<Self, PrepareError> {
        let mut result = Self::default();
        if spec.intermediates.is_empty() {
            return Ok(result);
        }
        if spec.intermediates.len() > crate::limits::Limits::COLUMNS {
            return Err(PrepareError::limit(
                "intermediates",
                crate::limits::Limits::COLUMNS,
                Some(spec.intermediates.len()),
                None,
            ));
        }
        let findings = checks::lookups::self_findings(spec);
        if !findings.is_empty() {
            return Err(PrepareError::Invalid(findings));
        }
        let findings = checks::lookups::admission_findings(spec);
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
        let calls = project.is_some();
        for index in derivation_order(spec, calls)? {
            let item = &spec.intermediates[index];
            let path = checks::render(spec, item.meta);
            let name = item.id.name.as_str();
            let row_driver = spec.rows.templates.iter().any(|template| {
                template
                    .driver
                    .as_ref()
                    .is_some_and(|driver| driver.name == name)
            });
            let driver_keys = row_driver.then(|| std::sync::Arc::clone(&driver_key_schema));
            let range = intermediate_range::prepare(spec, item)?;
            let columns = intermediate_range::prepare_columns(item)?;
            let dataset = item.dataset.name.as_str();
            let source = if dataset == "SELF" && !spec.rows.templates.is_empty() {
                self_relations::Relation::SelfRows
            } else if let Some(source) = sources.iter().position(|source| source.name == dataset) {
                self_relations::Relation::Input(source)
            } else {
                reject(
                    extra,
                    "intermediate_dataset",
                    checks::render(spec, item.dataset.meta),
                );
                continue;
            };
            let keys = match &item.key {
                Some(ast::JoinKey::Columns { columns, meta }) => {
                    if columns.len() > crate::limits::Limits::KEYS {
                        return Err(PrepareError::limit(
                            "intermediate_keys",
                            crate::limits::Limits::KEYS,
                            Some(columns.len()),
                            Some(&checks::render(spec, *meta)),
                        ));
                    }
                    Some(
                        columns
                            .iter()
                            .map(|column| {
                                (
                                    column.name.clone(),
                                    match_values::Declaration::Variable(column.name.clone()),
                                )
                            })
                            .collect(),
                    )
                }
                Some(ast::JoinKey::Pairs { pairs, meta }) => {
                    if pairs.len() > crate::limits::Limits::KEYS {
                        return Err(PrepareError::limit(
                            "intermediate_keys",
                            crate::limits::Limits::KEYS,
                            Some(pairs.len()),
                            Some(&checks::render(spec, *meta)),
                        ));
                    }
                    if pairs.is_empty() {
                        reject(extra, "intermediate_key", checks::render(spec, *meta));
                    }
                    let mut declared = Vec::new();
                    for (field, value) in pairs {
                        match_values::admit(
                            spec,
                            field.meta,
                            *value,
                            &checks::render(spec, field.meta),
                            project,
                            extra,
                        )?;
                        declared.push((
                            field.name.clone(),
                            match_values::Declaration::from_tree(spec, field.meta, *value, calls)?,
                        ));
                    }
                    Some(declared)
                }
                _ => None,
            };
            derivations::predicate_limits(spec, item.filter)?;
            let (filter_text, filter) = match item.filter.map(|filter| &spec.expr(filter).kind) {
                Some(ExprKind::Formula(formula)) => (
                    Some(formula.text.text.clone()),
                    Some(formula.compiled.clone()),
                ),
                Some(_) => return Err(PrepareError::Internal),
                None => (None, None),
            };
            let keep = item.keep.map(|keep| match keep {
                ast::Keep::First => Keep::First,
                ast::Keep::Last => Keep::Last,
            });
            if item.order_by.len() > crate::limits::Limits::COLUMNS {
                return Err(PrepareError::limit(
                    "intermediate_order",
                    crate::limits::Limits::COLUMNS,
                    Some(item.order_by.len()),
                    None,
                ));
            }
            let order = item
                .order_by
                .iter()
                .map(|term| tree_order_term(spec, term))
                .collect::<Result<Vec<_>, _>>()?;
            let no_match = checks::handler(spec, item.absent.as_ref())?;
            let unique = Self::unique_checks(spec, item)?;
            let driver_prohibited = checks::lookups::driver_prohibited(spec, item)
                .into_iter()
                .map(String::from)
                .collect();
            let derivations = intermediate_derivations::prepare(
                spec,
                item,
                &derivations::Admission {
                    project,
                    intermediates: &result,
                },
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
                    require_match: item.require_match,
                    range,
                    columns,
                    unique,
                    derivations,
                    driver_prohibited,
                    driver_keys,
                    key_required: (item.key.is_none() && !spec.header.surface.infers_join_keys())
                        .then(|| format!("{}.key", checks::render(spec, item.meta))),
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
fn derivation_order(spec: &ast::Spec, calls: bool) -> Result<Vec<usize>, PrepareError> {
    use crate::dependency_analysis;
    let names = spec
        .intermediates
        .iter()
        .map(|item| item.id.name.as_str())
        .collect::<Vec<_>>();
    let indices = names
        .iter()
        .enumerate()
        .map(|(index, &name)| (name, index))
        .collect::<BTreeMap<_, _>>();
    let mut graph = vec![Vec::new(); names.len()];
    let mut sites = vec![Vec::new(); names.len()];
    // A donor window cannot read a named selector (LOOKUP-16), so its fields
    // remain window findings rather than dependency edges.
    let scan = reads::Scan {
        relation: true,
        windows: false,
    };
    for (index, item) in spec.intermediates.iter().enumerate() {
        for binding in &item.bindings {
            let expr = binding.derivation.expr;
            let path = checks::path(spec, expr);
            for reference in reads::names(spec, expr, scan, calls)? {
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
