//! Named stored-input and SELF selections compiled from the admitted document.
use super::*;
use crate::{
    dataset::{Intermediate, IntermediateKey, Keep, OrderTerm, SecondarySource, SourceSelection},
    value::Value,
};

/// One `unique` check of an intermediate (LOOKUP-17).
#[derive(Debug)]
pub(super) struct UniqueDeclaration {
    pub(super) path: String,
    /// Each column as written, with its path.
    pub(super) columns: Vec<(String, String)>,
    pub(super) identity: Option<String>,
    pub(super) severity: crate::dataset::VerificationSeverity,
}
pub(super) type UniqueDeclarations = Vec<UniqueDeclaration>;

/// An intermediate's `unique` check, as its run-report row names it: the
/// intermediate, the dataset it reads and the columns the check lists
/// (LOOKUP-17, CHECK-24).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntermediateCheck<'a> {
    pub intermediate: &'a str,
    pub dataset: &'a str,
    pub columns: Vec<&'a str>,
    pub identity: Option<&'a str>,
    pub severity: crate::dataset::VerificationSeverity,
}

#[derive(Debug)]
pub(super) struct Declaration {
    pub(super) name: String,
    pub(super) path: String,
    /// The intermediate's position among those the specification declares.
    pub(super) declared: usize,
    source: self_relations::Relation,
    keys: Option<Vec<(String, match_values::Declaration)>>,
    pub(super) filter_text: Option<String>,
    pub(super) filter: Option<Result<CompiledExpression, CompileError>>,
    order: Vec<(String, bool, bool)>,
    keep: Option<Keep>,
    nearest: Option<NearestDeclaration>,
    no_match: Option<Value>,
    require_match: bool,
    range: Option<lookup_range::Declaration>,
    pub(super) columns: Option<Vec<String>>,
    pub(super) unique: UniqueDeclarations,
    pub(super) derivations: Vec<intermediate_derivations::Derivation>,
    driver_prohibited: Vec<String>,
    /// The sides and key of a pair relation (LOOKUP-27).
    pub(super) pair: Option<pairs::PairDeclaration>,
    /// The `key` path of a row driver, which declares no key, so a read of it
    /// as a lookup is refused there (LOOKUP-3, ROW-2).
    key_required: Option<String>,
}
impl Declaration {
    /// Whether the intermediate selects one record of several: by `keep`,
    /// or by `nearest` (LOOKUP-10, LOOKUP-26).
    fn selects(&self) -> bool {
        self.keep.is_some() || self.nearest.is_some()
    }
}

/// A nearest selection as written (LOOKUP-26).
#[derive(Debug)]
struct NearestDeclaration {
    /// `intermediates.<id>.nearest`.
    path: String,
    /// The donor field, written bare.
    source: String,
    /// The `to` field's path, at which its findings are reported.
    to_path: String,
    /// The current-row value, read as a key match value is (LOOKUP-5).
    to: match_values::Declaration,
    unit: Option<ast::TimeUnit>,
}

#[derive(Debug, Default)]
/// Each named intermediate, in derivation order; the intermediate whose
/// derivations read, when there is one; and how many inputs the
/// specification declares, after which each pair relation's records take
/// a position (LOOKUP-27).
pub(super) struct Declarations(std::sync::Arc<Vec<Declaration>>, Option<String>, usize);
type ResolveRef<'a> = &'a (dyn Fn(&str, &str, &mut Vec<BindFinding>) -> Option<usize> + 'a);
/// What binding one lookup reads, built for each lookup in the order the
/// lookups are derived ([`Declarations::bind`]) and borrowed down its walk:
/// every lookup's declaration, the captured inputs, the output schema and
/// the columns a SELF lookup reads, the project calls, the typed tree, the
/// secondary sources, and the lookups bound before this one.
#[derive(Clone, Copy)]
pub(super) struct Context<'a> {
    pub named: &'a Declarations,
    pub inputs: binder::Inputs<'a>,
    pub output: &'a TableSchema,
    pub self_columns: Option<&'a [usize]>,
    pub project: Option<&'a crate::project_calls::ProjectCalls>,
    pub tree: &'a types::Tree,
    pub secondary: &'a [SecondarySource],
    pub previous: &'a [Option<Intermediate>],
}

/// The relation a lookup reads, as its filter, range, nearest selection
/// and `unique` checks probe it: the lookup's name, the dataset it reads,
/// that dataset's schema once the lookup's derivations extend it, each
/// field's kind, and whether the lookup may read a field.
#[derive(Clone, Copy)]
pub(super) struct Donor<'a> {
    pub lookup: &'a str,
    pub dataset: &'a str,
    pub schema: &'a TableSchema,
    pub kind: &'a dyn Fn(usize) -> Option<ColumnType>,
    pub admits: &'a dyn Fn(usize) -> bool,
}

/// A lookup's filter over its relation: a read the relation does not
/// answer refuses the lookup (LOOKUP-7), and a grammar failure is reported
/// at the filter.
fn bind_filter(
    item: &Declaration,
    relation: &Donor<'_>,
    inputs: binder::Inputs<'_>,
    pairs: &[IntermediateKey],
    failed: &mut bool,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<crate::dataset::RelationPredicate>, BindError> {
    let filter = match &item.filter {
        Some(Ok(plan)) => {
            let filter = relation_predicates::bind(
                plan,
                relation,
                matches!(item.source, self_relations::Relation::SelfRows),
                inputs,
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
    Ok(filter)
}

/// A lookup's `order_by`, each term a field of its relation whose values
/// all compare (VALUE-22): a field whose type, `field_type`'s, holds two
/// types that do not compare under VALUE-19, such as an untyped derivation
/// choosing between text and a number, is refused before any record is
/// read.
fn bind_order(
    item: &Declaration,
    resolve: ResolveRef<'_>,
    field_type: &dyn Fn(usize) -> crate::result_type::ResultType,
    failed: &mut bool,
    findings: &mut Vec<BindFinding>,
) -> Vec<OrderTerm> {
    let mut order_by = Vec::new();
    for (index, (name, descending, nulls_first)) in item.order.iter().enumerate() {
        let path = format!("{}.order_by[{index}]", item.path);
        if let Some(column) = resolve(name, &path, findings) {
            // The finding refuses the plan; the lookup keeps its fields, so a
            // read of one is not unknown as well.
            let kind = field_type(column);
            if kind.unordered() {
                findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                    crate::diagnostic::lookup::order_term_type(
                        &path,
                        crate::diagnostic::lookup::Owner::Intermediate(&item.name),
                        name,
                        kind,
                    ),
                )));
            }
            order_by.push(OrderTerm {
                column,
                descending: *descending,
                nulls_first: *nulls_first,
            });
        } else {
            *failed = true;
        }
    }
    order_by
}

/// A lookup's `unique` checks (LOOKUP-17): each column a field of its
/// relation, and none beside a filter or a derivation that reads the
/// current row.
fn bind_unique(
    item: &Declaration,
    relation: &Donor<'_>,
    filter: &Option<crate::dataset::RelationPredicate>,
    derivations: &Option<crate::dataset::IntermediateDerivations>,
    findings: &mut Vec<BindFinding>,
) -> Vec<crate::dataset::IntermediateUnique> {
    let Donor {
        dataset: source_name,
        schema,
        admits,
        ..
    } = *relation;
    let mut unique = Vec::new();
    for declared in &item.unique {
        let mut columns = Vec::new();
        for (name, field_path) in &declared.columns {
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
            path: declared.path.clone(),
            columns,
            identity: declared.identity.clone(),
            severity: declared.severity,
            declared: item.declared,
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
    unique
}

/// Bind a nearest selection (LOOKUP-26): its `source`, a `date` or
/// `datetime` field of the lookup's relation; its `unit`, one that type
/// counts in (TIME-4); and its `to`, of the source's type and read as a key
/// match value is. The first finding refuses the selection.
fn bind_nearest(
    declaration: &NearestDeclaration,
    relation: &Donor<'_>,
    catalog: &Catalog,
    contextual: bool,
    context: match_values::Context<'_>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<crate::dataset::Nearest>, BindError> {
    let Donor {
        lookup: intermediate,
        schema,
        kind: donor_kind,
        admits,
        ..
    } = *relation;
    use crate::diagnostic::lookup::{self, Owner};
    use crate::result_type::ResultType;
    let owner = Owner::Intermediate(intermediate);
    let refuse = |diagnostic: crate::diagnostic::Diagnostic,
                  findings: &mut Vec<BindFinding>|
     -> Result<Option<crate::dataset::Nearest>, BindError> {
        findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
            diagnostic,
        )));
        Ok(None)
    };
    let source_path = format!("{}.source", declaration.path);
    let Some(source) = schema
        .columns()
        .iter()
        .position(|column| column.name == declaration.source)
        .filter(|&column| admits(column))
    else {
        return refuse(
            lookup::nearest_source_reference(&source_path, owner, &declaration.source),
            findings,
        );
    };
    let (kind, temporal) = match donor_kind(source) {
        Some(kind @ ColumnType::Date) => (kind, ast::TemporalType::Date),
        Some(kind @ ColumnType::DateTime) => (kind, ast::TemporalType::DateTime),
        other => {
            return refuse(
                lookup::nearest_source_type(
                    &source_path,
                    owner,
                    &declaration.source,
                    ResultType::declared(other),
                ),
                findings,
            )
        }
    };
    let unit = declaration.unit.ok_or(BindError::Internal)?;
    let difference = match crate::scalar_verb::difference(temporal, unit, &declaration.path) {
        Ok(difference) => difference,
        Err(diagnostic) => return refuse(diagnostic, findings),
    };
    let expected = ResultType::column(kind);
    let actual = declaration.to.result_type(context.tree, catalog)?;
    if !actual.comparable(expected) {
        return refuse(
            lookup::nearest_reference_type(&declaration.to_path, owner, expected, actual),
            findings,
        );
    }
    let to = &declaration.to;
    let Some(value) = (if contextual {
        to.bind_deferred(
            &declaration.to_path,
            intermediate,
            catalog,
            findings,
            context,
        )?
    } else {
        to.bind(
            &declaration.to_path,
            intermediate,
            catalog,
            findings,
            context,
        )?
    }) else {
        return Ok(None);
    };
    let difference = crate::dataset::Temporal::new(
        difference,
        vec![declaration.source.clone(), "to".into()],
        declaration.path.clone(),
    )
    .map_err(|_| BindError::Internal)?;
    Ok(Some(crate::dataset::Nearest {
        source,
        reference: IntermediateKey {
            source_column: source,
            path: declaration.to_path.clone(),
            value,
        },
        difference,
    }))
}

/// The field of the lookup's relation that `name` selects in its order:
/// one the relation admits, qualified by its dataset, or written bare for
/// SELF (`self_rows`). Any other name refuses the lookup (LOOKUP-7), with
/// the qualified field a bare name names as its suggestion, which is
/// withheld unless the donor's schema decides whether it stores the name,
/// or the lookup derives it.
fn select_field(
    relation: &Donor<'_>,
    self_rows: bool,
    inputs: binder::Inputs<'_>,
    name: &str,
    path: &str,
    findings: &mut Vec<BindFinding>,
) -> Option<usize> {
    let Donor {
        lookup,
        dataset,
        schema,
        admits,
        ..
    } = *relation;
    let bare = name
        .strip_prefix(&format!("{dataset}."))
        .or_else(|| self_rows.then_some(name));
    let field = bare
        .and_then(|name| {
            schema
                .columns()
                .iter()
                .position(|column| column.name == name)
        })
        .filter(|&column| admits(column));
    if field.is_none() {
        let suggestion = (!name.contains('.')
            && schema.columns().iter().any(|column| column.name == name))
        .then(|| format!("{dataset}.{name}"));
        let derived = suggestion.is_some()
            && inputs
                .schema(dataset)
                .is_some_and(|stored| stored.position(name).is_none());
        let finding = BindFinding::Lookup(LookupFinding::reference(
            path,
            name,
            Some(lookup),
            ReferenceCause::Selection,
            suggestion,
        ));
        findings.push(
            if name.contains('.') || derived || inputs.decides(dataset, name) {
                finding
            } else {
                BindFinding::Withheld(std::boxed::Box::new(finding))
            },
        );
    }
    field
}

/// Bind one lookup over its relation: its derivations, its keys' types and
/// fields, its key values and nearest reference in the current row, then
/// its filter, order, projection, range and `unique` checks.
fn bind_one(
    cx: Context<'_>,
    item: &Declaration,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Intermediate>, BindError> {
    let Context {
        named,
        inputs,
        output,
        self_columns,
        project,
        tree,
        secondary,
        previous: result,
    } = cx;
    let binder::Inputs {
        sources,
        schemas,
        driver,
    } = inputs;
    let mut failed = false;
    let (schema, source_name) = match item.source {
        self_relations::Relation::Input(index) => {
            (schemas[index].layout(), sources[index].name.as_str())
        }
        self_relations::Relation::SelfRows => (output, "SELF"),
        // A pair relation binds its two sides ([`pairs::bind`]).
        self_relations::Relation::Pair => return Err(BindError::Internal),
    };
    let output_kinds;
    let stored_kinds = match item.source {
        self_relations::Relation::Input(index) => schemas[index].kinds(),
        _ => {
            output_kinds = output
                .columns()
                .iter()
                .map(|column| Some(column.kind))
                .collect::<Vec<_>>();
            &output_kinds
        }
    };
    let mut pending_schema = None;
    let derivations = intermediate_derivations::bind(
        cx,
        intermediate_derivations::Lookup {
            reader: &item.name,
            declarations: &item.derivations,
            dataset: source_name,
            stored: schema,
            stored_kinds,
            stored_available: matches!(item.source, self_relations::Relation::SelfRows)
                .then_some(self_columns.unwrap_or(&[])),
        },
        &mut pending_schema,
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
                self_relations::Relation::Input(index) => schemas[index].kind(column),
                self_relations::Relation::SelfRows => {
                    output.columns().get(column).map(|column| column.kind)
                }
                self_relations::Relation::Pair => None,
            }
        }
    };
    let admits = |column: usize| {
        !matches!(item.source, self_relations::Relation::SelfRows)
            || column >= output.columns().len()
            || self_columns.is_some_and(|columns| columns.contains(&column))
    };
    let donor = Donor {
        lookup: &item.name,
        dataset: source_name,
        schema,
        kind: &donor_kind,
        admits: &admits,
    };
    let self_rows = matches!(item.source, self_relations::Relation::SelfRows);
    let resolve = |name: &str, path: &str, findings: &mut Vec<BindFinding>| {
        select_field(&donor, self_rows, inputs, name, path, findings)
    };
    let catalog = current_catalog(output, inputs)?;
    // An intermediate without `key` drives rows and is never a keyed lookup
    // (ROW-2, LOOKUP-3); preparation refused every other.
    let no_keys = Vec::new();
    let pairs_declared = item.keys.as_ref().unwrap_or(&no_keys);
    // Result types and donor fields precede expression declarations and reads.
    failed |= check_keys(item, &donor, pairs_declared, tree, &catalog, findings)?;
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
            donors: binder::Donors {
                named,
                secondary,
                items: result,
                inputs,
            },
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
    // The reference a nearest selection measures to is read with the match
    // values, after them (LOOKUP-26).
    let nearest = match &item.nearest {
        Some(declaration) => {
            let nearest = bind_nearest(
                declaration,
                &donor,
                &catalog,
                contextual_reader,
                match_values::Context {
                    output,
                    driver: &sources[driver].name,
                    row: None,
                    phase: crate::reference_scope::Phase::Row { group_by: None },
                    project,
                    tree,
                    donors: binder::Donors {
                        named,
                        secondary,
                        items: result,
                        inputs,
                    },
                },
                findings,
            )?;
            failed |= nearest.is_none();
            nearest
        }
        None => None,
    };
    let filter = bind_filter(item, &donor, inputs, &pairs, &mut failed, findings)?;
    // A derived field's type may be decided per value (EXPR-14).
    let field_type = |column| match &derivations {
        Some(derived) => derived.column_type(column),
        None => crate::result_type::ResultType::declared(donor_kind(column)),
    };
    let order_by = bind_order(item, &resolve, &field_type, &mut failed, findings);
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
                &donor,
                &catalog,
                contextual_reader,
                binder::Donors {
                    named,
                    secondary,
                    items: result,
                    inputs,
                },
                findings,
            )
        })
        .transpose()?
        .flatten();
    let unique = bind_unique(item, &donor, &filter, &derivations, findings);
    // Range rejection owns its original cause without turning an existing
    // donor field into a second unknown-reference diagnostic.
    let source = match item.source {
        self_relations::Relation::Input(index) if index == driver => {
            crate::dataset::IntermediateSource::Primary {
                schema: schemas[index].layout().clone(),
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
        self_relations::Relation::Pair => return Err(BindError::Internal),
    };
    Ok((!failed).then(|| Intermediate {
        identifier: item.name.clone(),
        path: item.path.clone(),
        source,
        keys: pairs,
        filter,
        selection: item.keep.map(|keep| SourceSelection { order_by, keep }),
        nearest,
        no_match: item.no_match.clone(),
        require_match: item.require_match,
        unique,
        derivations,
        pending_schema,

        range,
        columns,
    }))
}

/// The current row's catalog that a lookup's key values, nearest reference
/// and range value read: the output columns, and each input's fields, the
/// driver's first.
fn current_catalog(output: &TableSchema, inputs: binder::Inputs<'_>) -> Result<Catalog, BindError> {
    let binder::Inputs {
        sources,
        schemas,
        driver,
    } = inputs;
    let output_fields = output
        .columns()
        .iter()
        .map(|column| reference_binding::Field {
            name: &column.name,
            column_type: Some(column.kind),
        })
        .collect::<Vec<_>>();
    let current_sources = std::iter::once(driver)
        .chain((0..sources.len()).filter(|&index| index != driver))
        .collect::<Vec<_>>();
    let source_fields = current_sources
        .iter()
        .map(|&index| schemas[index].fields())
        .collect::<Vec<_>>();
    let datasets = current_sources
        .iter()
        .zip(&source_fields)
        .map(|(&index, fields)| reference_binding::Dataset {
            name: &sources[index].name,
            fields,
        })
        .collect::<Vec<_>>();
    Catalog::compile(&output_fields, &datasets).map_err(BindError::Catalog)
}

/// Each key's types and donor field, before any key value is read: a match
/// value whose type cannot compare with its donor field's (LOOKUP-6), and a
/// key field the relation lacks or the lookup may not read. Whether either
/// refuses the lookup.
fn check_keys(
    item: &Declaration,
    relation: &Donor<'_>,
    pairs_declared: &[(String, match_values::Declaration)],
    tree: &types::Tree,
    catalog: &Catalog,
    findings: &mut Vec<BindFinding>,
) -> Result<bool, BindError> {
    let Donor {
        dataset: source_name,
        schema,
        kind: donor_kind,
        admits,
        ..
    } = *relation;
    let mut failed = false;
    for (field, declaration) in pairs_declared {
        if let Some(donor) = schema
            .columns()
            .iter()
            .position(|column| column.name == *field)
            .and_then(donor_kind)
        {
            let expected = declaration.result_type(tree, catalog)?;
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
    Ok(failed)
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

    pub(super) fn for_reader(&self, reader: &str) -> Self {
        Self(std::sync::Arc::clone(&self.0), Some(reader.into()), self.2)
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

    pub fn is_self(&self, index: usize) -> bool {
        self.0
            .get(index)
            .is_some_and(|item| matches!(item.source, self_relations::Relation::SelfRows))
    }
    /// Whether `name` is a named intermediate read as a lookup: every one but
    /// a pair relation, which is read as an input is (LOOKUP-27).
    pub fn contains(&self, name: &str) -> bool {
        self.0
            .iter()
            .any(|item| item.name == name && item.pair.is_none())
    }
    /// The input a named intermediate reads; a row it drives reads that input's records.
    pub(super) fn input(&self, name: &str) -> Option<usize> {
        match self.0.iter().find(|item| item.name == name)?.source {
            self_relations::Relation::Input(index) if index < self.2 => Some(index),
            _ => None,
        }
    }
    /// Whether the named intermediate `name` derives the field `field`.
    pub(super) fn derives(&self, name: &str, field: &str) -> bool {
        self.0.iter().any(|item| {
            item.name == name
                && item
                    .derivations
                    .iter()
                    .any(|derivation| derivation.name == field)
        })
    }
    /// Whether `name` selects one record for each row: it declares `keep`
    /// or `nearest` (LOOKUP-22).
    pub(super) fn kept(&self, name: &str) -> bool {
        self.0
            .iter()
            .any(|item| item.name == name && item.selects())
    }
    /// Where a read of the row driver at `index` as a keyed lookup is
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
    /// The lookup a qualified `name` reads and its field. A pair relation is
    /// read as an input is, never as a lookup (LOOKUP-27).
    pub fn reference<'a>(&self, name: &'a str) -> Option<(usize, &'a str)> {
        let (relation, field) = name.split_once('.')?;
        self.0
            .iter()
            .position(|item| item.name == relation && item.pair.is_none())
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
    /// The `to` of the intermediate `name`'s nearest selection, a match
    /// value as a key's is (LOOKUP-26).
    pub(super) fn reference_declaration(&self, name: &str) -> Option<&match_values::Declaration> {
        let item = self.0.iter().find(|item| item.name == name)?;
        item.nearest.as_ref().map(|nearest| &nearest.to)
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
        self.earlier()
            .iter()
            .any(|item| item.name == name && item.pair.is_none())
    }
    /// Whether `name` is an intermediate derived before the reading one that
    /// selects one record: it declares `keep` or `nearest` (LOOKUP-22).
    pub(super) fn earlier_kept(&self, name: &str) -> bool {
        self.earlier()
            .iter()
            .any(|item| item.name == name && item.selects())
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
            unique.push(UniqueDeclaration {
                path: checks::render(spec, check.meta),
                columns,
                identity: check.id.as_ref().map(|identity| identity.text.clone()),
                severity: match check.severity {
                    ast::Severity::Error => crate::dataset::VerificationSeverity::Error,
                    ast::Severity::Warning => crate::dataset::VerificationSeverity::Warning,
                },
            });
        }
        Ok(unique)
    }

    /// The `unique` check at `path` of an intermediate that reads `sources`
    /// or `SELF` (LOOKUP-17).
    pub(super) fn unique_check<'a>(
        &'a self,
        path: &str,
        sources: &'a [SourceDeclaration],
    ) -> Option<IntermediateCheck<'a>> {
        self.0.iter().find_map(|item| {
            let check = item.unique.iter().find(|check| check.path == path)?;
            Some(IntermediateCheck {
                intermediate: &item.name,
                dataset: match item.source {
                    self_relations::Relation::Input(index) => match sources.get(index) {
                        Some(source) => &source.name,
                        // An intermediate over a pair relation, whose
                        // records are the relation's (LOOKUP-27).
                        None => self
                            .0
                            .iter()
                            .find(|pair| {
                                pair.pair
                                    .as_ref()
                                    .is_some_and(|pair| pair.slot == index - sources.len())
                            })?
                            .name
                            .as_str(),
                    },
                    self_relations::Relation::SelfRows => "SELF",
                    self_relations::Relation::Pair => &item.name,
                },
                columns: check
                    .columns
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect(),
                identity: check.identity.as_deref(),
                severity: check.severity,
            })
        })
    }

    /// Each named intermediate, in the order its derivations need, admitted
    /// before any source is read. A dataset or key outside what binding
    /// supports is collected in `extra`.
    pub fn prepare(
        spec: &ast::Spec,
        sources: &[SourceDeclaration],
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
        let calls = project.is_some();
        result.2 = sources.len();
        // Each pair relation takes the next position after the inputs, in the
        // order the intermediates are derived (LOOKUP-27).
        let mut slots: Vec<&str> = Vec::new();
        for index in derivation_order(spec, calls)? {
            let item = &spec.intermediates[index];
            let path = checks::render(spec, item.meta);
            let name = item.id.name.as_str();
            let range = intermediate_range::prepare(spec, item)?;
            let columns = intermediate_range::prepare_columns(item)?;
            let dataset = item.dataset.name.as_str();
            let mut pair = None;
            let source = if item.pairs() {
                pair = Some(pairs::prepare(spec, item, slots.len(), project, extra)?);
                slots.push(name);
                self_relations::Relation::Pair
            } else if dataset == "SELF" && !spec.rows.templates.is_empty() {
                self_relations::Relation::SelfRows
            } else if let Some(source) = sources.iter().position(|source| source.name == dataset) {
                self_relations::Relation::Input(source)
            } else if let Some(slot) = slots.iter().position(|&slot| slot == dataset) {
                self_relations::Relation::Input(sources.len() + slot)
            } else {
                reject(
                    extra,
                    "intermediate_dataset",
                    checks::render(spec, item.dataset.meta),
                );
                continue;
            };
            // Every key names at most `Limits::KEYS` pairs, a pair relation's
            // among them, in either form.
            let written = match &item.key {
                Some(ast::JoinKey::Columns { columns, meta }) => Some((columns.len(), meta)),
                Some(ast::JoinKey::Pairs { pairs, meta }) => Some((pairs.len(), meta)),
                _ => None,
            };
            if let Some((count, meta)) =
                written.filter(|&(count, _)| count > crate::limits::Limits::KEYS)
            {
                return Err(PrepareError::limit(
                    "intermediate_keys",
                    crate::limits::Limits::KEYS,
                    Some(count),
                    Some(&checks::render(spec, *meta)),
                ));
            }
            // A pair relation's key pairs its sides, not a current row's values
            // ([`pairs::prepare`]).
            let keys = match &item.key {
                _ if pair.is_some() => None,
                Some(ast::JoinKey::Columns { columns, .. }) => Some(
                    columns
                        .iter()
                        .map(|column| {
                            (
                                column.name.clone(),
                                match_values::Declaration::Variable(column.name.clone()),
                            )
                        })
                        .collect(),
                ),
                Some(ast::JoinKey::Pairs { pairs, meta }) => {
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
            // A nearest selection's `to` is admitted as a key's match value
            // is, at its own field (LOOKUP-26, LOOKUP-5).
            let nearest = match &item.nearest {
                Some(nearest) => {
                    let to_path = checks::render(spec, nearest.to_field);
                    match_values::admit(
                        spec,
                        nearest.to_field,
                        nearest.to,
                        &to_path,
                        project,
                        extra,
                    )?;
                    Some(NearestDeclaration {
                        path: checks::render(spec, nearest.meta),
                        source: nearest.source.name.clone(),
                        to: match_values::Declaration::from_tree(
                            spec,
                            nearest.to_field,
                            nearest.to,
                            calls,
                        )?,
                        to_path,
                        unit: nearest.unit,
                    })
                }
                None => None,
            };
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
                    declared: index,
                    source,
                    keys,
                    filter,
                    filter_text,
                    order,
                    keep,
                    nearest,
                    no_match,
                    require_match: item.require_match,
                    range,
                    columns,
                    unique,
                    derivations,
                    driver_prohibited,
                    pair,
                    key_required: item
                        .key
                        .is_none()
                        .then(|| format!("{}.key", checks::render(spec, item.meta))),
                });
        }
        Ok(result)
    }
    /// Bind each named intermediate over the inputs, in the order the
    /// intermediates are derived, each reading those bound before it.
    pub fn bind(
        &self,
        inputs: binder::Inputs<'_>,
        output: &TableSchema,
        self_columns: Option<&[usize]>,
        project: Option<&crate::project_calls::ProjectCalls>,
        tree: &types::Tree,
        findings: &mut Vec<BindFinding>,
    ) -> Result<Bound, BindError> {
        let binder::Inputs {
            sources,
            schemas,
            driver,
        } = inputs;
        let mut secondary = inputs.secondary();
        let mut withheld = Vec::new();
        // Each pair relation's records follow the inputs once it binds, read
        // as an input's by every intermediate derived after it (LOOKUP-27).
        // The inputs are borrowed until the first pair relation binds.
        let mut relations = std::borrow::Cow::Borrowed(sources);
        let mut relation_schemas = std::borrow::Cow::Borrowed(schemas);
        let mut result = Vec::new();
        for item in self.0.iter() {
            let schemas = &*relation_schemas;
            if let Some(pair) = &item.pair {
                let (bound, fields) = pairs::bind(
                    item,
                    pair,
                    &pairs::Context {
                        named: self,
                        relations: binder::Inputs {
                            sources: &relations,
                            schemas,
                            driver,
                        },
                        inputs: sources.len(),
                        secondary: &secondary,
                        result: &result,
                        project,
                        tree,
                    },
                    findings,
                )?;
                // A relation that does not bind keeps its position, with no
                // field; its findings are reported.
                let fields = match fields {
                    Some(fields) => fields,
                    None => TableSchema::new(Vec::new()).map_err(|_| BindError::Internal)?,
                };
                relations.to_mut().push(pairs::pseudo_input(&item.name));
                relation_schemas.to_mut().push(InputSchema::exact(&fields));
                secondary.push(SecondarySource {
                    name: item.name.clone(),
                    schema: fields,
                });
                result.push(bound);
                continue;
            }
            // An intermediate over a pair relation that did not bind reads
            // nothing; the relation's findings are reported.
            if let self_relations::Relation::Input(index) = item.source {
                if index >= sources.len()
                    && self.0.iter().zip(&result).any(|(other, bound)| {
                        other.name == relations[index].name && bound.is_none()
                    })
                {
                    result.push(None);
                    continue;
                }
            }
            let before = findings.len();
            let bound = bind_one(
                Context {
                    named: self,
                    inputs: binder::Inputs {
                        sources: &relations,
                        schemas,
                        driver,
                    },
                    output,
                    self_columns,
                    project,
                    tree,
                    secondary: &secondary,
                    previous: &result,
                },
                item,
                findings,
            )?;
            withhold_reads(&mut findings[before..], &withheld);
            let failed = &findings[before..];
            if bound.is_none()
                && !failed.is_empty()
                && failed
                    .iter()
                    .all(|finding| matches!(finding, BindFinding::Withheld(_)))
            {
                withheld.push(item.name.clone());
            }
            result.push(bound);
        }
        Ok(Bound {
            secondary,
            items: result,
            withheld,
        })
    }
    /// The position of the intermediate `name`, a pair relation included.
    pub(super) fn position(&self, name: &str) -> Option<usize> {
        self.0.iter().position(|item| item.name == name)
    }
}

/// The named intermediates as binding bound them ([`Declarations::bind`]):
/// the secondary ports, with each pair relation's records after the inputs;
/// each intermediate, or `None` where it did not bind; and the names of those
/// that did not bind only because of findings `check()` withholds.
pub(super) struct Bound {
    pub secondary: Vec<SecondarySource>,
    pub items: Vec<Option<Intermediate>>,
    pub withheld: Vec<String>,
}

/// Withhold each read of an intermediate in `withheld` that binding refuses
/// as unbound: the failure follows from an answer the schemas do not decide,
/// so `check()` withholds it with the finding that caused it (#3805).
pub(super) fn withhold_reads(findings: &mut [BindFinding], withheld: &[String]) {
    for finding in findings {
        let caused = matches!(finding, BindFinding::UnknownReference { name, .. }
            if name
                .split_once('.')
                .is_some_and(|(owner, _)| withheld.iter().any(|name| name == owner)));
        if caused {
            let read = finding.take();
            *finding = BindFinding::Withheld(std::boxed::Box::new(read));
        }
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
        // A pair relation reads the records of the intermediates its sides
        // name, and an intermediate over a pair relation reads the
        // relation's records (LOOKUP-27).
        let relations = match &item.inner_join {
            Some(right) => vec![&item.dataset, right],
            None if spec.pair_relation(&item.dataset.name).is_some() => vec![&item.dataset],
            None => Vec::new(),
        };
        for relation in relations {
            if let Some(&dependency) = indices.get(relation.name.as_str()) {
                if dependency != index {
                    graph[index].push(dependency);
                    sites[index].push((dependency, checks::render(spec, relation.meta)));
                }
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
