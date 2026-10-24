//! The schemas binding reads its inputs through (#3805): one value, built
//! three ways, so `run()` and `check()` bind through one walk.
//!
//! Binding asks three things of an input: whether it stores a field, the
//! field's type, and the field's slot, which only places the column in the
//! plan. It reads no record. Each input's [`InputSchema`] answers them: its
//! slots in order, an optional kind for each, which slots a declaration
//! states, and whether its slots are every field it stores.
//!
//! - **Captured** ([`InputSchemas::captured`]), what `run()` binds against:
//!   each decoded schema, every slot typed and stated, every input closed.
//! - **Declared** ([`InputSchemas::declared`]), what the declarations decide
//!   before any input is read. A schema link stores exactly its producer's
//!   output fields, typed by its contract (INPUT-10, INPUT-11). Any other
//!   input stores its `types` entries, its generated ordinal (`int`,
//!   INPUT-3) and every field the specification reads from it, and may
//!   store more: a delimited input's undeclared field is `str` (INPUT-5),
//!   and any other input's undeclared field has no type until it is read
//!   (EXPR-14).
//! - **Written** ([`InputSchemas::written`]): the declared schemas without
//!   INPUT-5's `str`, the types written declarations decide when no input
//!   is read at all.
//!
//! An open input answers whether it stores a field it has no stated slot
//! for as undecided ([`InputSchema::stores`]): binding reads such a field
//! as stored, as every read a specification makes of data that matches its
//! declarations binds, but a finding whose wording depends on the answer is
//! withheld from `check()` ([`BindFinding::Withheld`]).
use super::*;
use crate::ast::{ExprKind, JoinKey, Scope};
use std::collections::BTreeSet;

/// One input's schema as binding reads it.
#[derive(Clone, Debug)]
pub(super) struct InputSchema {
    /// Each slot's name, in slot order, with its kind or, where no source
    /// decides one, a layout placeholder that no check reads.
    layout: TableSchema,
    /// Each slot's kind; `None` where the source does not decide it.
    kinds: Vec<Option<ColumnType>>,
    /// Whether a declaration states each slot: a captured field, a linked
    /// field, a `types` entry or the ordinal.
    stated: Vec<bool>,
    /// Whether the slots are every field the input stores.
    closed: bool,
}

impl InputSchema {
    /// A schema every slot of which is typed and stated, and which is
    /// closed: a decoded input's, or the fields a pair relation exposes.
    pub(super) fn exact(schema: &TableSchema) -> Self {
        let width = schema.columns().len();
        Self {
            layout: schema.clone(),
            kinds: schema.columns().iter().map(|column| Some(column.kind)).collect(),
            stated: vec![true; width],
            closed: true,
        }
    }
    /// The slots `fields` names, each `(name, kind, stated)`, in order.
    fn new(fields: Vec<(String, Option<ColumnType>, bool)>, closed: bool) -> Result<Self, BindError> {
        let layout = TableSchema::new(
            fields
                .iter()
                .map(|(name, kind, _)| Column {
                    name: name.clone(),
                    kind: kind.unwrap_or(ColumnType::Str),
                })
                .collect(),
        )
        .map_err(|_| BindError::Internal)?;
        Ok(Self {
            layout,
            kinds: fields.iter().map(|&(_, kind, _)| kind).collect(),
            stated: fields.iter().map(|&(_, _, stated)| stated).collect(),
            closed,
        })
    }
    /// Each slot's name, in slot order, as a plan lays the input out. A
    /// slot's kind is [`Self::kind`]'s, never this layout's.
    pub(super) fn layout(&self) -> &TableSchema {
        &self.layout
    }
    /// The kind of the slot `column`, when the source decides it.
    pub(super) fn kind(&self, column: usize) -> Option<ColumnType> {
        self.kinds.get(column).copied().flatten()
    }
    /// Each slot's kind, in slot order.
    pub(super) fn kinds(&self) -> &[Option<ColumnType>] {
        &self.kinds
    }
    /// The slot of the field `field`.
    pub(super) fn position(&self, field: &str) -> Option<usize> {
        self.layout
            .columns()
            .iter()
            .position(|column| column.name == field)
    }
    /// Whether the input stores `field`, as the source decides it: a slot a
    /// declaration states does, and an absent field of a closed input does
    /// not. `None` for anything else: a slot the specification only reads,
    /// or an absent field of an open input.
    pub(super) fn stores(&self, field: &str) -> Option<bool> {
        match self.position(field) {
            Some(column) if self.stated[column] => Some(true),
            None if self.closed => Some(false),
            _ => None,
        }
    }
    /// The catalog fields of this input, each with its kind.
    pub(super) fn fields(&self) -> Vec<reference_binding::Field<'_>> {
        self.layout
            .columns()
            .iter()
            .zip(&self.kinds)
            .map(|(column, &kind)| reference_binding::Field {
                name: &column.name,
                column_type: kind,
            })
            .collect()
    }
}

/// Every input's schema, in authored order.
#[derive(Clone, Debug)]
pub(super) struct InputSchemas(Vec<InputSchema>);

impl InputSchemas {
    /// The decoded schemas `run()` binds against, in authored order.
    pub(super) fn captured(schemas: &[&TableSchema]) -> Self {
        Self(schemas.iter().map(|schema| InputSchema::exact(schema)).collect())
    }
    /// What the declarations of `prepared` decide of each input.
    pub(super) fn declared(prepared: &PreparedSpecification) -> Result<Self, BindError> {
        Self::from_declarations(prepared, true)
    }
    /// What written declarations decide of each input: the declared schemas
    /// without the `str` that INPUT-5 gives an undeclared field of a
    /// delimited input once it is read.
    pub(super) fn written(prepared: &PreparedSpecification) -> Result<Self, BindError> {
        Self::from_declarations(prepared, false)
    }
    fn from_declarations(prepared: &PreparedSpecification, read: bool) -> Result<Self, BindError> {
        let spec = prepared.tree.spec();
        let odm_items = odm_items(spec);
        prepared
            .sources
            .iter()
            .enumerate()
            .map(|(index, source)| {
                if let Some(contract) = prepared.source_contract(index) {
                    let fields = contract
                        .fields()
                        .iter()
                        .map(|field| (field.name.clone(), Some(field.kind), true))
                        .collect();
                    return InputSchema::new(fields, true);
                }
                let input = spec
                    .inputs
                    .iter()
                    .find(|input| input.name.name == source.name)
                    .ok_or(BindError::Internal)?;
                // INPUT-5 types an undeclared field of a delimited input
                // `str`; any other undeclared field has no type until read.
                let undeclared = (read && input.source.profile == Some(ast::Profile::Csv))
                    .then_some(ColumnType::Str);
                let mut fields: Vec<(String, Option<ColumnType>, bool)> = source
                    .types
                    .iter()
                    .map(|(field, kind)| (field.clone(), Some(*kind), true))
                    .collect();
                let mut add = |field: &str, kind: Option<ColumnType>, stated: bool| {
                    if !field.is_empty() && !fields.iter().any(|(name, ..)| name == field) {
                        fields.push((field.into(), kind, stated));
                    }
                };
                if let Some(ordinal) = &source.ordinal {
                    add(ordinal, Some(ColumnType::Int), true);
                }
                if prepared.source_interpretation(index) == Some(SourceInterpretation::Odm) {
                    for field in crate::odm_source::FIELDS {
                        add(field, undeclared, false);
                    }
                }
                for field in read_fields(spec, &source.name, &odm_items) {
                    add(&field, undeclared, false);
                }
                InputSchema::new(fields, false)
            })
            .collect::<Result<_, _>>()
            .map(Self)
    }
    /// Every input's schema, in authored order.
    pub(super) fn schemas(&self) -> &[InputSchema] {
        &self.0
    }
}

/// The item reads of every `odm` read, whose `DATASET.ItemOID` names an
/// item, not a stored field.
fn odm_items(spec: &ast::Spec) -> BTreeSet<ExprId> {
    let mut items = BTreeSet::new();
    for site in spec.sites() {
        spec.walk(site.root, site.scope, |at, _| {
            if let ExprKind::Odm(read) = &spec.expr(at).kind {
                items.insert(read.item);
            }
            ast::Step::Continue
        });
    }
    items
}

/// Every field the specification reads from the input `input`, in the
/// order it first reads each: a name it qualifies by the input, wherever it
/// is read; a bare name a lookup over the input reads of its donor record,
/// other than one the lookup derives; the fields a lookup over the input
/// names as its key, `unique` columns, `columns` and nearest `source`; the
/// input's declared `key`, which an implicit join of it pairs; and a pair
/// relation's right key fields when the input is its right side.
fn read_fields(spec: &ast::Spec, input: &str, odm_items: &BTreeSet<ExprId>) -> Vec<String> {
    let mut fields = Vec::new();
    let mut add = |field: &str| {
        if field != "*" && !fields.iter().any(|known| known == field) {
            fields.push(field.to_owned());
        }
    };
    let over = |index: usize| {
        let item = &spec.intermediates[index];
        !item.pairs() && item.dataset.name == input
    };
    for column in spec.input_key(input).unwrap_or_default() {
        add(&column.name);
    }
    for site in spec.sites() {
        for read in spec.reads(site.root, site.scope) {
            if odm_items.contains(&read.at) {
                continue;
            }
            match read.name.owner() {
                Some(owner) if owner == input => add(read.name.field()),
                Some(_) => {}
                None => {
                    if let Scope::Donor(index) = read.scope {
                        let item = &spec.intermediates[index];
                        let derived = item
                            .bindings
                            .iter()
                            .any(|binding| binding.name.name == read.name.as_str());
                        if over(index) && !derived {
                            add(read.name.as_str());
                        }
                    }
                }
            }
        }
    }
    for (index, item) in spec.intermediates.iter().enumerate() {
        if item.pairs() {
            let right = item.inner_join.as_ref().is_some_and(|right| right.name == input);
            let left = item.dataset.name == input;
            match &item.key {
                Some(JoinKey::Columns { columns, .. }) if left || right => {
                    for column in columns {
                        add(&column.name);
                    }
                }
                Some(JoinKey::Pairs { pairs, .. }) if right => {
                    for (column, _) in pairs {
                        add(&column.name);
                    }
                }
                _ => {}
            }
            continue;
        }
        if !over(index) {
            continue;
        }
        match &item.key {
            Some(JoinKey::Columns { columns, .. }) => {
                for column in columns {
                    add(&column.name);
                }
            }
            Some(JoinKey::Pairs { pairs, .. }) => {
                for (column, _) in pairs {
                    add(&column.name);
                }
            }
            _ => {}
        }
        let derived = |name: &str| item.bindings.iter().any(|binding| binding.name.name == name);
        for check in &item.checks {
            for column in check.columns.iter().filter(|column| !derived(&column.name)) {
                add(&column.name);
            }
        }
        for column in item.columns.iter().flatten().filter(|column| !derived(&column.name)) {
            add(&column.name);
        }
        if let Some(nearest) = item.nearest.as_ref().filter(|nearest| !derived(&nearest.source.name)) {
            add(&nearest.source.name);
        }
    }
    fields
}
