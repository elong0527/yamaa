//! The pair relation (LOOKUP-27): a named intermediate that writes
//! `inner_join`. Its records are the matching pairs of its two sides, its
//! `dataset` on the left and its `inner_join` on the right, matched on its
//! `key`, and its fields are its derivations, which read both sides.
//!
//! Preparation reads the sides and the key as written. Binding gives each
//! side the fields of its records, types the key pairs, binds the filter and
//! the derivations over the joined record (the left side's fields, then the
//! right side's), and gives the relation the fields every reader reads
//! through its port, as it reads an input's. A side is an input or an
//! intermediate a row template could drive on (ROW-2); preflight refuses
//! every other.
//!
//! A counted relation (LOOKUP-29), a named intermediate that writes
//! `repeat`, is a pair relation whose right side is generated: each left
//! record pairs with each whole number from 1 to the count it gives. It
//! declares no key; its count is read of the left record as a key mapping's
//! value is, and its right side's one field, the number, is read by the
//! bare name its `index` gives.
use super::*;
use crate::dataset::{
    Intermediate, IntermediateDerivations, IntermediateSource, IntermediateUnique, PairCount,
    PairJoin, PairKey, PairRecords, PairSide, PairValue, RelationPredicate, SecondarySource,
    SourceSchemas,
};
use crate::diagnostic::lookup;
use crate::result_type::ResultType;

/// One side as written: the relation it names and the field that names it.
#[derive(Debug)]
pub(super) struct SideDeclaration {
    pub name: String,
    pub path: String,
}

/// The left value of one key pair, as written.
#[derive(Debug)]
pub(super) enum LeftValue {
    /// A field of the left side: the same-named field of the list form, or
    /// a mapping's value qualified by the left side.
    Field(String),
    /// A mapping's value that names no field through the left side's
    /// qualifier: a bare name or a name qualified by another relation,
    /// written alone.
    Name(String),
    /// Any other mapping value: an expression over the left record, whose
    /// value each left record gives before the pairing (LOOKUP-6).
    Expression(ExprId),
}

/// One key pair as written: the right side's field, the left value it must
/// equal, and the path its refusals are reported at.
#[derive(Debug)]
pub(super) struct KeyDeclaration {
    pub right: String,
    pub left: LeftValue,
    pub path: String,
}

/// A counted relation's `repeat` as written (LOOKUP-29).
#[derive(Debug)]
pub(super) struct CountDeclaration {
    /// The count each left record gives, read as a key mapping's value is.
    pub count: LeftValue,
    /// `intermediates.<id>.repeat.count`, at which the count's refusals are
    /// reported.
    pub path: String,
    /// The name the number of each record is read by.
    pub index: String,
}

/// The right side as written: the relation `inner_join` names, or the whole
/// numbers `repeat` generates for each left record.
#[derive(Debug)]
pub(super) enum RightDeclaration {
    Relation(SideDeclaration),
    Counted(CountDeclaration),
}

/// A pair relation's sides and key as written, and its position among the
/// pair relations the specification declares, which places its records
/// after the inputs (LOOKUP-27). A counted relation declares no key
/// (LOOKUP-29).
#[derive(Debug)]
pub(super) struct PairDeclaration {
    pub left: SideDeclaration,
    pub right: RightDeclaration,
    pub keys: Vec<KeyDeclaration>,
    pub slot: usize,
}

/// The left value written at `value` in the field at `field` (LOOKUP-6): a
/// name written alone names a field of the left side `left` through its
/// qualifier, or nothing; any other value is an expression over the left
/// record, admitted as a lookup key's match value is, and what binding does
/// not support of it is collected in `extra`.
fn left_value(
    spec: &ast::Spec,
    field: ast::Meta,
    value: ExprId,
    left: &str,
    project: Option<&functions::Prepared>,
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<LeftValue, PrepareError> {
    let alone = spec.expr(value).meta.path == field.path;
    Ok(match &spec.expr(value).kind {
        ExprKind::Ref(name) if alone => match name.as_str().split_once('.') {
            Some((owner, field)) if owner == left => LeftValue::Field(field.into()),
            _ => LeftValue::Name(name.as_str().into()),
        },
        _ => {
            match_values::admit_value(spec, field, value, project, extra)?;
            LeftValue::Expression(value)
        }
    })
}

/// The sides and key of the pair relation `item`, the `slot`-th the
/// specification declares, as written. A mapping's expression value is
/// admitted as a lookup key's match value is (LOOKUP-6); what binding does
/// not support of it is collected in `extra`. A counted relation's count is
/// read as such a value is (LOOKUP-29).
pub(super) fn prepare(
    spec: &ast::Spec,
    item: &ast::Intermediate,
    slot: usize,
    project: Option<&functions::Prepared>,
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<PairDeclaration, PrepareError> {
    let path = checks::render(spec, item.meta);
    let left = SideDeclaration {
        name: item.dataset.name.clone(),
        path: checks::render(spec, item.dataset.meta),
    };
    if let Some(repeat) = &item.repeat {
        let count = left_value(
            spec,
            repeat.count_field,
            repeat.count,
            &left.name,
            project,
            extra,
        )?;
        return Ok(PairDeclaration {
            left,
            right: RightDeclaration::Counted(CountDeclaration {
                count,
                path: checks::render(spec, repeat.count_field),
                index: repeat.index.name.clone(),
            }),
            keys: Vec::new(),
            slot,
        });
    }
    let right = item.inner_join.as_ref().ok_or(PrepareError::Internal)?;
    let right = SideDeclaration {
        name: right.name.clone(),
        path: checks::render(spec, right.meta),
    };
    let key = format!("{path}.key");
    let keys = match &item.key {
        Some(ast::JoinKey::Columns { columns, .. }) => columns
            .iter()
            .map(|column| KeyDeclaration {
                right: column.name.clone(),
                left: LeftValue::Field(column.name.clone()),
                path: key.clone(),
            })
            .collect(),
        Some(ast::JoinKey::Pairs { pairs, .. }) => pairs
            .iter()
            .map(|(column, value)| {
                let left = left_value(spec, column.meta, *value, &left.name, project, extra)?;
                // A name written alone is refused at `key`, any other value
                // at its own entry (LOOKUP-5).
                let path = match left {
                    LeftValue::Expression(_) => checks::render(spec, column.meta),
                    LeftValue::Field(_) | LeftValue::Name(_) => key.clone(),
                };
                Ok(KeyDeclaration {
                    right: column.name.clone(),
                    left,
                    path,
                })
            })
            .collect::<Result<_, _>>()?,
        _ => Vec::new(),
    };
    Ok(PairDeclaration {
        left,
        right: RightDeclaration::Relation(right),
        keys,
        slot,
    })
}

/// A field of one side's records: its name, its position in the side's
/// layout and its type, `None` where the input's schema does not decide it.
pub(super) struct Field {
    pub name: String,
    pub column: usize,
    pub kind: Option<ColumnType>,
}

/// One side's records and their visible fields. A counted relation's
/// generated side has no records until its count binds (LOOKUP-29).
struct BoundSide {
    records: Option<PairRecords>,
    fields: Vec<Field>,
}

/// What binding reads besides the pair relation's own declaration.
pub(super) struct Context<'a> {
    pub named: &'a intermediates::Declarations,
    /// The inputs, then a pseudo-input for each pair relation, with their
    /// captured schemas and the driver.
    pub relations: binder::Inputs<'a>,
    /// How many of the `relations` are inputs.
    pub inputs: usize,
    pub secondary: &'a [SecondarySource],
    /// The intermediates bound before this one, in derivation order.
    pub result: &'a [Option<Intermediate>],
    pub project: Option<&'a crate::project_calls::ProjectCalls>,
    pub tree: &'a types::Tree,
}

/// The records of `declared`: an input's, or those of an intermediate a row
/// template could drive on, with each field it exposes typed (ROW-2). None
/// when the side is refused here or its intermediate failed to bind.
fn side(
    declared: &SideDeclaration,
    cx: &Context<'_>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<BoundSide>, BindError> {
    if let Some(index) = cx.relations.sources[..cx.inputs]
        .iter()
        .position(|source| source.name == declared.name)
    {
        let schema = &cx.relations.schemas[index];
        let fields = schema
            .layout()
            .columns()
            .iter()
            .enumerate()
            .map(|(column, field)| Field {
                name: field.name.clone(),
                column,
                kind: schema.kind(column),
            })
            .collect();
        let port = match index.cmp(&cx.relations.driver) {
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Less => index + 1,
            std::cmp::Ordering::Greater => index,
        };
        return Ok(Some(BoundSide {
            records: Some(PairRecords::Source(port)),
            fields,
        }));
    }
    let index = cx
        .named
        .position(&declared.name)
        .ok_or(BindError::Internal)?;
    let Some(item) = cx.result.get(index).and_then(Option::as_ref) else {
        return Ok(None);
    };
    match driver_fields(item, &cx.relations, cx.secondary)? {
        DriverFields::Fields(fields) => Ok(Some(BoundSide {
            records: Some(PairRecords::Intermediate(index)),
            fields,
        })),
        DriverFields::Unbound => Ok(None),
        DriverFields::Current(prohibited) => {
            findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                lookup::pair_side(&declared.path, &declared.name, &prohibited),
            )));
            Ok(None)
        }
        DriverFields::Untyped(untyped) => {
            let untyped = untyped.iter().map(String::as_str).collect::<Vec<_>>();
            findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                lookup::pair_field_type(&declared.path, &declared.name, &untyped),
            )));
            Ok(None)
        }
    }
}

/// The fields of the records a row template driving on an intermediate
/// would read (ROW-2), as a pair relation's side or a record relation's
/// reader reads them (LOOKUP-27, LOOKUP-3), or why there are none.
pub(super) enum DriverFields {
    /// Each visible field, stored then derived, in layout order.
    Fields(Vec<Field>),
    /// The intermediate did not bind as declared; its own findings explain
    /// why.
    Unbound,
    /// The intermediate's `filter` or `derivations`, as listed, read a
    /// current row, so its records are not its own.
    Current(Vec<&'static str>),
    /// The derived fields that have no one column type (EXPR-14).
    Untyped(Vec<String>),
}

/// The fields of the bound intermediate `item`'s records as a row driver
/// reads them, whose relations and secondary ports are `relations` and
/// `secondary` ([`DriverFields`]).
pub(super) fn driver_fields(
    item: &Intermediate,
    relations: &binder::Inputs<'_>,
    secondary: &[SecondarySource],
) -> Result<DriverFields, BindError> {
    // A rejected augmentation keeps only its declared schema. Its own
    // findings explain the failure; its derived fields have no bound type
    // for a reader to check (LOOKUP-16, LOOKUP-27).
    if item.pending_schema.is_some() {
        return Ok(DriverFields::Unbound);
    }
    // Neither the filter nor the derivations read a current row: its
    // records are its own (ROW-2).
    let mut prohibited = Vec::new();
    if item
        .filter
        .as_ref()
        .is_some_and(RelationPredicate::correlated)
    {
        prohibited.push("filter");
    }
    if item
        .derivations
        .as_ref()
        .is_some_and(|derived| !derived.current.is_empty())
    {
        prohibited.push("derivations");
    }
    if !prohibited.is_empty() {
        return Ok(DriverFields::Current(prohibited));
    }
    let schema = item.source_schema(secondary).ok_or(BindError::Internal)?;
    let mut fields = Vec::new();
    let mut untyped = Vec::new();
    for (column, field) in schema.columns().iter().enumerate() {
        if !item.admits_column(column) {
            continue;
        }
        // A derived field of no one type is untyped; a stored field has the
        // kind its input's schema gives it, decided or not.
        match item.source_kind(secondary, column) {
            Some(_) => fields.push(Field {
                name: field.name.clone(),
                column,
                kind: relations.source_kind(item, secondary, column),
            }),
            None => untyped.push(field.name.clone()),
        }
    }
    if !untyped.is_empty() {
        return Ok(DriverFields::Untyped(untyped));
    }
    Ok(DriverFields::Fields(fields))
}

/// A field of the joined record by the name that reads it: `L.f` reads the
/// left side's field and `R.g` the right side's, at their positions in the
/// record. A counted relation's right side is generated and has no
/// qualifier: its one field, the number, is read by its bare name
/// (LOOKUP-29).
struct Joined<'a> {
    left: &'a str,
    right: Option<&'a str>,
    left_fields: &'a [Field],
    right_fields: &'a [Field],
}

impl Joined<'_> {
    fn field(&self, name: &str) -> Option<(usize, Option<ColumnType>)> {
        let (fields, offset, field) = match name.split_once('.') {
            Some((owner, field)) if owner == self.left => (self.left_fields, 0, field),
            Some((owner, field)) if Some(owner) == self.right => {
                (self.right_fields, self.left_fields.len(), field)
            }
            None if self.right.is_none() => (self.right_fields, self.left_fields.len(), name),
            _ => return None,
        };
        fields
            .iter()
            .position(|candidate| candidate.name == field)
            .map(|position| (offset + position, fields[position].kind))
    }
    /// The joined record's stored fields: the left side's, then the right
    /// side's. The generated side's field keeps the name that reads it, so
    /// a derivation reads it as it reads a derivation declared before it.
    fn schema(&self) -> Result<TableSchema, BindError> {
        let mut columns = Vec::new();
        for (offset, field) in self.left_fields.iter().enumerate() {
            columns.push(Column {
                name: format!("\0left{offset}"),
                kind: field.kind.unwrap_or(ColumnType::Str),
            });
        }
        for (offset, field) in self.right_fields.iter().enumerate() {
            columns.push(Column {
                name: match self.right {
                    Some(_) => format!("\0right{offset}"),
                    None => field.name.clone(),
                },
                kind: field.kind.unwrap_or(ColumnType::Str),
            });
        }
        TableSchema::new(columns).map_err(|_| BindError::Internal)
    }
}

/// Bind the pair relation `declaration` (LOOKUP-27): its sides, its key
/// pairs, its filter and its derivations over the joined record, its
/// exposed fields and its `unique` checks. Gives the relation's
/// intermediate, which materializes it, none when a finding refuses it, and
/// the fields its port exposes: those it bound, or else those its
/// declaration names ([`declared_fields`]). A counted relation binds its
/// count where a pair relation binds its key (LOOKUP-29).
pub(super) fn bind(
    declaration: &intermediates::Declaration,
    pair: &PairDeclaration,
    cx: &Context<'_>,
    findings: &mut Vec<BindFinding>,
) -> Result<(Option<Intermediate>, InputSchema), BindError> {
    let name = declaration.name.as_str();
    let left = side(&pair.left, cx, findings)?;
    let right = match &pair.right {
        RightDeclaration::Relation(declared) => side(declared, cx, findings)?,
        // The generated side holds one field, each record's number.
        RightDeclaration::Counted(count) => Some(BoundSide {
            records: None,
            fields: vec![Field {
                name: count.index.clone(),
                column: 0,
                kind: Some(ColumnType::Int),
            }],
        }),
    };
    let (Some(left), Some(mut right)) = (left, right) else {
        return Ok((None, declared_fields(declaration)?));
    };
    let before = findings.len();
    let (right_name, keys, values) = match &pair.right {
        RightDeclaration::Relation(declared) => {
            let sides = (
                (pair.left.name.as_str(), &left),
                (declared.name.as_str(), &right),
            );
            let (keys, values) = bind_keys(&pair.keys, name, sides, cx, findings)?;
            (declared.name.as_str(), keys, values)
        }
        RightDeclaration::Counted(count) => {
            let (value, values) =
                bind_count(count, name, &pair.left.name, &left.fields, cx, findings)?;
            right.records = value.map(|value| {
                PairRecords::Counted(PairCount {
                    value,
                    path: count.path.clone(),
                })
            });
            (count.index.as_str(), Vec::new(), values)
        }
    };
    let joined = Joined {
        left: &pair.left.name,
        right: match &pair.right {
            RightDeclaration::Relation(declared) => Some(&declared.name),
            RightDeclaration::Counted(_) => None,
        },
        left_fields: &left.fields,
        right_fields: &right.fields,
    };
    let joined_schema = joined.schema()?;
    let filter = match &declaration.filter {
        Some(Ok(plan)) => bind_filter(plan, name, &joined, cx, findings)?,
        Some(Err(CompileError::Invalid {
            position, failure, ..
        })) => {
            findings.push(BindFinding::Lookup(LookupFinding::grammar(
                &format!("{}.filter", declaration.path),
                declaration.filter_text.as_deref().unwrap_or_default(),
                position.character,
                failure,
            )));
            None
        }
        Some(Err(error)) => return Err(BindError::ExpressionPolicy(error.clone())),
        None => None,
    };
    let derivations = bind_derivations(declaration, &joined, &joined_schema, cx, findings)?;
    let Some((derived, field_types)) = derivations else {
        return Ok((None, declared_fields(declaration)?));
    };
    // The fields the relation exposes: its `columns`, each a derived name,
    // or else every derivation, in declaration order.
    let stored = joined_schema.columns().len();
    let output = derived.plan.output();
    let derived_column = |field: &str| {
        output.columns()[stored..]
            .iter()
            .position(|column| column.name == field)
            .map(|position| stored + position)
    };
    let exposed: Vec<usize> = match &declaration.columns {
        Some(listed) => {
            let mut exposed = Vec::new();
            for field in listed {
                match derived_column(field) {
                    Some(column) if !exposed.contains(&column) => exposed.push(column),
                    Some(_) => {}
                    None => findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                        lookup::pair_reference(
                            &format!("{}.columns", declaration.path),
                            name,
                            field,
                        ),
                    ))),
                }
            }
            exposed
        }
        None => (stored..output.columns().len()).collect(),
    };
    let mut untyped = Vec::new();
    let mut fields = Vec::new();
    for &column in &exposed {
        let field = &output.columns()[column];
        match field_types[column - stored].column_type() {
            Some(kind) => fields.push(Column {
                name: field.name.clone(),
                kind,
            }),
            None => untyped.push(field.name.as_str()),
        }
    }
    if !untyped.is_empty() {
        findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
            lookup::pair_field_type(&format!("{}.derivations", declaration.path), name, &untyped),
        )));
    }
    let mut unique = Vec::new();
    for declared in &declaration.unique {
        let mut columns = Vec::new();
        for (field, path) in &declared.columns {
            match derived_column(field) {
                Some(column) => columns.push(column),
                None => {
                    let mut finding = LookupFinding::new(
                        path,
                        crate::diagnostic::ConditionCode::IntermediateUniqueField,
                    );
                    finding.text("intermediate", name);
                    finding.text("identifier", &format!("{name}.{field}"));
                    findings.push(BindFinding::Lookup(finding));
                }
            }
        }
        unique.push(IntermediateUnique {
            path: declared.path.clone(),
            columns,
            identity: declared.identity.clone(),
            severity: declared.severity,
            declared: declaration.declared,
        });
    }
    if findings.len() != before {
        return Ok((None, declared_fields(declaration)?));
    }
    let fields = TableSchema::new(fields).map_err(|_| BindError::Internal)?;
    // A side without records is the generated side of a count a finding
    // refused, which returned above.
    let side = |bound: BoundSide, name: &str| {
        Ok::<_, BindError>(PairSide {
            name: name.into(),
            records: bound.records.ok_or(BindError::Internal)?,
            columns: bound.fields.iter().map(|field| field.column).collect(),
        })
    };
    let join = PairJoin {
        left: side(left, &pair.left.name)?,
        right: side(right, right_name)?,
        keys,
        schema: joined_schema,
        values,
        filter,
        port: cx.inputs - 1 + pair.slot,
        fields: fields.clone(),
    };
    Ok((
        Some(Intermediate {
            identifier: name.into(),
            path: declaration.path.clone(),
            source: IntermediateSource::Pairs(Box::new(join)),
            keys: Vec::new(),
            filter: None,
            selection: None,
            nearest: None,
            pivot: None,
            range: None,
            columns: Some(exposed),
            no_match: None,
            require_match: false,
            unique,
            derivations: Some(derived),
            pending_schema: None,
            records: None,
        }),
        InputSchema::exact(&fields),
    ))
}

/// Each key pair, its left value a field of the joined record from the left
/// side or a value of the key-values plan, and the plan of each left
/// record's key values when an expression gives one (LOOKUP-6). A right
/// side's field the right side does not have, a left field the left side
/// does not have, a name that reads no left field and a pair whose types
/// cannot compare fail at the pair's path (LOOKUP-27, LOOKUP-9). Each of
/// `sides` is a side's name and its records' fields, the left side first.
fn bind_keys(
    declared: &[KeyDeclaration],
    name: &str,
    sides: ((&str, &BoundSide), (&str, &BoundSide)),
    cx: &Context<'_>,
    findings: &mut Vec<BindFinding>,
) -> Result<(Vec<PairKey>, Option<IntermediateDerivations>), BindError> {
    let ((left_name, left), (right_name, right)) = sides;
    let push = |findings: &mut Vec<BindFinding>, diagnostic| {
        findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
            diagnostic,
        )))
    };
    let expressions = declared
        .iter()
        .filter_map(|key| match key.left {
            LeftValue::Expression(expr) => Some((expr, key.path.as_str())),
            _ => None,
        })
        .collect::<Vec<_>>();
    let values = if expressions.is_empty() {
        None
    } else {
        bind_values(
            LeftRecord {
                name,
                left: left_name,
                fields: &left.fields,
                reference: lookup::pair_reference,
            },
            &expressions,
            cx,
            findings,
        )?
    };
    let mut keys = Vec::new();
    let mut expression = 0;
    for key in declared {
        let right_field = right
            .fields
            .iter()
            .position(|field| field.name == key.right);
        if right_field.is_none() {
            push(
                findings,
                lookup::pair_key_reference(&key.path, name, &format!("{right_name}.{}", key.right)),
            );
        }
        let left_value = match &key.left {
            LeftValue::Field(field) => {
                let position = left
                    .fields
                    .iter()
                    .position(|candidate| candidate.name == *field);
                if position.is_none() {
                    push(
                        findings,
                        lookup::pair_key_reference(
                            &key.path,
                            name,
                            &format!("{left_name}.{field}"),
                        ),
                    );
                }
                position.map(|position| {
                    (
                        PairValue::Field(position),
                        ResultType::declared(left.fields[position].kind),
                    )
                })
            }
            LeftValue::Name(written) => {
                push(
                    findings,
                    lookup::pair_key_reference(&key.path, name, written),
                );
                None
            }
            LeftValue::Expression(_) => {
                let column = left.fields.len() + expression;
                expression += 1;
                values
                    .as_ref()
                    .map(|(_, types)| (PairValue::Value(column), types[column - left.fields.len()]))
            }
        };
        let (Some((left_value, expected)), Some(right_field)) = (left_value, right_field) else {
            continue;
        };
        let actual = ResultType::declared(right.fields[right_field].kind);
        if !expected.comparable(actual) {
            push(
                findings,
                lookup::pair_key_type(
                    &key.path,
                    name,
                    &format!("{right_name}.{}", key.right),
                    expected,
                    actual,
                ),
            );
            continue;
        }
        keys.push(PairKey {
            left: left_value,
            right: left.fields.len() + right_field,
            path: key.path.clone(),
            source: format!("{right_name}.{}", key.right),
        });
    }
    Ok((keys, values.map(|(plan, _)| plan)))
}

/// The count each left record gives a counted relation (LOOKUP-29): a field
/// of the left side, or the value of the plan that evaluates an expression
/// over each left record before any record is built, as it evaluates a
/// key's (LOOKUP-6). A name written alone that reads no field of the left
/// side, and a count whose result type is not `int` alone, fail at
/// `repeat.count`. A count that reads a field whose type no source decides
/// yet is compared with nothing: `run()`, whose captured schemas decide
/// every stored type, refuses it.
fn bind_count(
    count: &CountDeclaration,
    name: &str,
    left: &str,
    fields: &[Field],
    cx: &Context<'_>,
    findings: &mut Vec<BindFinding>,
) -> Result<(Option<PairValue>, Option<IntermediateDerivations>), BindError> {
    let refuse = |findings: &mut Vec<BindFinding>, diagnostic| {
        findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
            diagnostic,
        )));
        Ok((None, None))
    };
    let undecided = |field: &str| {
        fields
            .iter()
            .any(|candidate| candidate.name == field && candidate.kind.is_none())
    };
    let (value, kind, plan, decided) = match &count.count {
        LeftValue::Field(field) => {
            let Some(position) = fields.iter().position(|candidate| candidate.name == *field)
            else {
                return refuse(
                    findings,
                    lookup::repeat_count_reference(&count.path, name, &format!("{left}.{field}")),
                );
            };
            (
                PairValue::Field(position),
                ResultType::declared(fields[position].kind),
                None,
                !undecided(field),
            )
        }
        LeftValue::Name(written) => {
            return refuse(
                findings,
                lookup::repeat_count_reference(&count.path, name, written),
            );
        }
        LeftValue::Expression(expr) => {
            let record = LeftRecord {
                name,
                left,
                fields,
                reference: lookup::repeat_count_reference,
            };
            let Some((plan, types)) =
                bind_values(record, &[(*expr, count.path.as_str())], cx, findings)?
            else {
                return Ok((None, None));
            };
            let decided = !cx
                .tree
                .spec()
                .reads(*expr, cx.tree.donor(name)?)
                .iter()
                .any(|read| read.name.owner() == Some(left) && undecided(read.name.field()));
            (
                PairValue::Value(fields.len()),
                types[0],
                Some(plan),
                decided,
            )
        }
    };
    if decided && kind != ResultType::INT {
        return refuse(findings, lookup::repeat_count_type(&count.path, name, kind));
    }
    Ok((Some(value), plan))
}

/// Each side's fields as a joined record's catalog reads them: `(field,
/// position, type)`, the positions counted from `offset`.
fn catalog_fields(fields: &[Field], offset: usize) -> Vec<(&str, usize, Option<ColumnType>)> {
    fields
        .iter()
        .enumerate()
        .map(|(position, field)| (field.name.as_str(), offset + position, field.kind))
        .collect()
}

/// The catalog a donor plan over a joined record binds in: its fields as
/// `output`, each of `record_types` its field's result type, and each side's
/// name reading its own fields of the one record.
fn joined_catalog(
    output: &TableSchema,
    record_types: &[ResultType],
    sides: &[reference_binding::JoinedSide<'_>],
) -> Result<Catalog, BindError> {
    let outputs = output
        .columns()
        .iter()
        .map(|column| reference_binding::Field {
            name: &column.name,
            column_type: Some(column.kind),
        })
        .collect::<Vec<_>>();
    Ok(Catalog::compile_joined(&outputs, sides)
        .map_err(BindError::Catalog)?
        .with_record_derivation()
        .with_record_types(record_types.to_vec()))
}

/// The finding of a name that binds nothing, from its path, the relation
/// and the name: a pair relation's, or a counted relation's count's.
type Reference = fn(&str, &str, &str) -> crate::diagnostic::Diagnostic;

/// What the entries of one donor plan over a joined record bind in: the
/// plan's catalog and fields, the side whose records drive it, the pair
/// relation whose findings they are, and the finding of a name that binds
/// nothing.
struct Entries<'e> {
    catalog: &'e Catalog,
    output: &'e TableSchema,
    driver: &'e str,
    name: &'e str,
    reference: Reference,
}

impl Entries<'_> {
    /// One entry, bound at `column` as an intermediate's derivation binds
    /// (`intermediate_derivations::bind`), the record's fields before it
    /// `available`. A name nothing binds is the pair relation's finding at
    /// the operand's path ([`reference_findings`]).
    fn bind(
        &self,
        expr: ExprId,
        column: usize,
        available: &[usize],
        cx: &Context<'_>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<Option<Expression>, BindError> {
        let empty = intermediates::Declarations::default();
        let mut edges = Vec::new();
        let start = findings.len();
        let expression = BindContext {
            scope: binder::Scope {
                template: binder::Template::Any,
                phase: crate::reference_scope::Phase::Row { group_by: None },
                driver: self.driver,
                available: Some(available),
            },
            donors: binder::Donors {
                named: &empty,
                secondary: &[],
                items: &[],
                inputs: cx.relations,
            },
            catalog: self.catalog,
            output: self.output,
            project: cx.project,
            tree: cx.tree,
            edges: &mut edges,
            findings: &mut *findings,
            reported: false,
        }
        .bind(expr, column)?;
        reference_findings(&mut findings[start..], self.name, self.reference);
        Ok(expression)
    }
}

/// A bound donor plan and the result type of each value it derives.
type Plan = (IntermediateDerivations, Vec<ResultType>);

/// A donor plan over a joined record whose first `width` fields are stored:
/// each copied, then `assignments` evaluated in declaration order, the
/// `typed_columns` converting their results.
fn donor_plan(
    width: usize,
    output: TableSchema,
    record_types: Vec<ResultType>,
    assignments: Vec<Assignment>,
    handlers: Vec<crate::dataset::ConversionHandler>,
    typed_columns: Vec<usize>,
) -> IntermediateDerivations {
    let rows = vec![RowTemplate {
        mode: RowMode::Records,
        filter: None,
        assignments: (0..width)
            .map(|column| Assignment {
                column,
                path: format!("donor.{column}"),
                expression: Expression::Source(column),
            })
            .collect(),
    }];
    let plan = DatasetPlan::new_donor(
        SourceSchemas {
            primary: output.clone(),
            secondary: Vec::new(),
        },
        Vec::new(),
        output,
        rows,
        assignments,
        record_types,
    )
    .with_conversion_handlers(handlers);
    IntermediateDerivations {
        plan: std::sync::Arc::new(plan),
        current: Vec::new(),
        stored_columns: width,
        typed_columns,
    }
}

/// The left records a plan of values is evaluated over: the relation whose
/// values they are, the left side's name and visible fields, and the
/// finding of a name a value reads that binds nothing.
#[derive(Clone, Copy)]
struct LeftRecord<'a> {
    name: &'a str,
    left: &'a str,
    fields: &'a [Field],
    reference: Reference,
}

/// The plan of each left record's key values (LOOKUP-6): a donor plan over
/// the left side's visible fields, one untyped value for each expression of
/// the key's mapping, each reading the left side's fields by its qualifier,
/// and each value's result type (EXPR-14). None when a finding refuses one.
/// A counted relation's count is one such value (LOOKUP-29).
fn bind_values(
    record: LeftRecord<'_>,
    expressions: &[(ExprId, &str)],
    cx: &Context<'_>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Plan>, BindError> {
    let LeftRecord {
        name,
        left,
        fields,
        reference,
    } = record;
    let before = findings.len();
    let scope = cx.tree.donor(name)?;
    let width = fields.len();
    let typing = |read: &str| -> ResultType {
        read.split_once('.')
            .filter(|(owner, _)| *owner == left)
            .and_then(|(_, field)| fields.iter().find(|candidate| candidate.name == field))
            .map_or(ResultType::UNKNOWN, |field| {
                ResultType::declared(field.kind)
            })
    };
    let types = expressions
        .iter()
        .map(|&(expr, _)| cx.tree.result_type(expr, scope, &typing))
        .collect::<Vec<_>>();
    let mut columns = fields
        .iter()
        .enumerate()
        .map(|(position, field)| Column {
            name: format!("\0left{position}"),
            kind: field.kind.unwrap_or(ColumnType::Str),
        })
        .collect::<Vec<_>>();
    columns.extend(types.iter().enumerate().map(|(position, kind)| Column {
        name: format!("\0value{position}"),
        kind: kind.layout(),
    }));
    let output = TableSchema::new(columns).map_err(|_| BindError::Internal)?;
    let record_types: Vec<ResultType> = fields
        .iter()
        .map(|field| ResultType::declared(field.kind))
        .chain(types.iter().copied())
        .collect();
    let side = catalog_fields(fields, 0);
    let catalog = joined_catalog(&output, &record_types, &[(left, &side)])?;
    let entries = Entries {
        catalog: &catalog,
        output: &output,
        driver: left,
        name,
        reference,
    };
    let available: Vec<usize> = (0..width).collect();
    let mut assignments = Vec::new();
    for (position, &(expr, path)) in expressions.iter().enumerate() {
        let column = width + position;
        if let Some(expression) = entries.bind(expr, column, &available, cx, findings)? {
            assignments.push(Assignment {
                column,
                path: path.into(),
                expression,
            });
        }
    }
    if findings.len() != before {
        return Ok(None);
    }
    Ok(Some((
        donor_plan(
            width,
            output,
            record_types,
            assignments,
            Vec::new(),
            Vec::new(),
        ),
        types,
    )))
}

/// The pair filter over the joined record: each name it reads is a field of
/// a side, qualified by the side (LOOKUP-27), typed by the side; a counted
/// relation's reads the number by its bare name (LOOKUP-29).
fn bind_filter(
    plan: &CompiledExpression,
    name: &str,
    joined: &Joined<'_>,
    cx: &Context<'_>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<RelationPredicate>, BindError> {
    let before = findings.len();
    let mut bindings = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for identifier in plan.identifiers() {
        if !seen.insert(identifier) {
            continue;
        }
        match joined.field(identifier) {
            Some((column, _)) => bindings.push(Binding {
                name: identifier.into(),
                read: Read::Source(column),
            }),
            None => findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                lookup::pair_reference(plan.spec_path(), name, identifier),
            ))),
        }
    }
    let kind = |name: &str| {
        joined
            .field(name)
            .and_then(|(_, kind)| kind.map(Into::into))
    };
    types::predicate_faults(
        plan,
        kind,
        types::FieldOrigins {
            spec: cx.tree.spec(),
            inputs: cx.relations,
            derived: None,
        },
        findings,
    );
    if findings.len() != before {
        return Ok(None);
    }
    let typed = plan.typed(|name| ResultType::known(kind(name)));
    Ok(Some(
        RelationPredicate::new(
            crate::bound_expression::BoundPredicate::new(typed, bindings)
                .map_err(BindError::InvalidPredicateBinding)?,
            Vec::new(),
        )
        .map_err(BindError::InvalidPredicateBinding)?,
    ))
}

/// The derivations over the joined record, as a donor plan whose stored
/// fields are the joined record's: each reads a side's field by its
/// qualified name, or a derivation declared before it by its bare name, and
/// a typed entry converts its result (LOOKUP-15). A counted relation's read
/// the number by its bare name too (LOOKUP-29). Gives the plan and each
/// derivation's field type, none when a finding refuses one.
fn bind_derivations(
    declaration: &intermediates::Declaration,
    joined: &Joined<'_>,
    stored: &TableSchema,
    cx: &Context<'_>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Plan>, BindError> {
    let name = declaration.name.as_str();
    let before = findings.len();
    let scope = cx.tree.donor(name)?;
    let width = stored.columns().len();
    // Each derivation's field type, in declaration order.
    let mut field_types: Vec<ResultType> = Vec::new();
    let mut operation_types = Vec::new();
    let mut fields = stored.columns().to_vec();
    for derivation in &declaration.derivations {
        let types = |read: &str| -> ResultType {
            if let Some((_, kind)) = joined.field(read) {
                return ResultType::declared(kind);
            }
            if read.contains('.') {
                return ResultType::UNKNOWN;
            }
            fields[width..]
                .iter()
                .position(|field| field.name == read)
                .map_or(ResultType::UNKNOWN, |position| field_types[position])
        };
        let operation = cx.tree.result_type(derivation.expr, scope, &types);
        let field = derivation.field_type(operation);
        fields.push(Column {
            name: derivation.name.clone(),
            kind: field.layout(),
        });
        field_types.push(field);
        operation_types.push(operation);
    }
    let output = TableSchema::new(fields).map_err(|_| BindError::Internal)?;
    let record_types: Vec<ResultType> = joined
        .left_fields
        .iter()
        .chain(joined.right_fields)
        .map(|field| ResultType::declared(field.kind))
        .chain(field_types.iter().copied())
        .collect();
    let left = catalog_fields(joined.left_fields, 0);
    let right = catalog_fields(joined.right_fields, joined.left_fields.len());
    // The generated side has no qualifier: the record's own field, named as
    // the index names it, reads the number.
    let mut sides = vec![(joined.left, left.as_slice())];
    sides.extend(joined.right.map(|name| (name, right.as_slice())));
    let catalog = joined_catalog(&output, &record_types, &sides)?;
    let entries = Entries {
        catalog: &catalog,
        output: &output,
        driver: joined.left,
        name,
        reference: lookup::pair_reference,
    };
    let mut assignments = Vec::new();
    let mut available: Vec<usize> = (0..width).collect();
    for (index, derivation) in declaration.derivations.iter().enumerate() {
        let column = width + index;
        let expression = entries.bind(derivation.expr, column, &available, cx, findings)?;
        if let Some(finding) = derivation
            .typed()
            .filter(|_| expression.is_some() && derivation.handler().is_none())
            .and_then(|typed| {
                types::conversion_finding(derivation.path(), operation_types[index], typed)
            })
        {
            findings.push(BindFinding::Scalar(finding));
        }
        if let Some(expression) = expression {
            assignments.push(Assignment {
                column,
                path: derivation.assignment_path().into(),
                expression,
            });
        }
        available.push(column);
    }
    if findings.len() != before {
        return Ok(None);
    }
    let handlers = declaration
        .derivations
        .iter()
        .filter_map(|derivation| {
            derivation
                .handler()
                .map(|handler| crate::dataset::ConversionHandler {
                    assignment_path: derivation.assignment_path().into(),
                    handler: handler.clone(),
                })
        })
        .collect();
    let typed_columns = declaration
        .derivations
        .iter()
        .enumerate()
        .filter_map(|(index, derivation)| derivation.typed().map(|_| width + index))
        .collect();
    Ok(Some((
        donor_plan(
            width,
            output,
            record_types,
            assignments,
            handlers,
            typed_columns,
        ),
        field_types,
    )))
}

/// A name a pair relation's key value or derivation reads that binds
/// nothing: no field of a side by its qualifier and no derivation declared
/// before it (LOOKUP-27). The shared binders report it at the operand's
/// path; `reference` gives the finding of the relation that reads it.
fn reference_findings(findings: &mut [BindFinding], intermediate: &str, reference: Reference) {
    for finding in findings {
        // The pair relation's own cause does not keep a withheld wording.
        if matches!(finding, BindFinding::Withheld(inner) if matches!(
            **inner,
            BindFinding::UnknownReference { .. }
                | BindFinding::OutputReference { .. }
                | BindFinding::QualifiedReference { .. }
        )) {
            finding.release();
        }
        let read = match finding {
            BindFinding::UnknownReference { path, name }
            | BindFinding::OutputReference { path, name, .. }
            | BindFinding::QualifiedReference { path, name, .. } => {
                Some((path.clone(), name.clone()))
            }
            _ => None,
        };
        if let Some((path, name)) = read {
            *finding = BindFinding::Lookup(LookupFinding::from_diagnostic(reference(
                &path,
                intermediate,
                &name,
            )));
        }
    }
}

/// The declaration a pair relation's records stand under after the inputs,
/// so that binding reads them as it reads an input's (LOOKUP-27). No run
/// captures it: the run fills its port.
pub(super) fn pseudo_input(name: &str) -> SourceDeclaration {
    SourceDeclaration {
        name: name.into(),
        path: String::new(),
        types: Vec::new(),
        profile: SourceProfile::Csv,
        encoding: None,
        byte_order_mark: false,
        fixed_width: None,
        empty_string_present: false,
        ordinal: None,
    }
}

/// The fields a pair relation's port exposes as its declaration names them:
/// each exposed derivation, of its declared type or else of none the
/// declaration decides. Its readers, a row template driving on it and an
/// intermediate over it, bind against them when the relation does not
/// bind, so that only the relation's own findings and theirs are reported
/// (#4965).
fn declared_fields(declaration: &intermediates::Declaration) -> Result<InputSchema, BindError> {
    let mut fields = Vec::new();
    let exposed = |name: &str| {
        declaration
            .columns
            .as_ref()
            .is_none_or(|listed| listed.iter().any(|field| field == name))
    };
    for derivation in &declaration.derivations {
        if exposed(&derivation.name) {
            fields.push((derivation.name.clone(), derivation.typed(), true));
        }
    }
    InputSchema::new(fields, true)
}
