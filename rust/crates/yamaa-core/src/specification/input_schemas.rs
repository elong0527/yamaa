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
//!   output fields, typed by its contract (INPUT-10, INPUT-11), and its own
//!   generated ordinal (`int`, INPUT-3). Any other input stores its `types`
//!   entries, its ordinal and every field the specification reads from it
//!   ([`read_fields`]), and may store more: a delimited input's undeclared
//!   field is `str` (INPUT-5), and any other input's undeclared field has
//!   no type until it is read (EXPR-14).
//! - **Written** ([`InputSchemas::written`]): the declared schemas without
//!   INPUT-5's `str`, the types written declarations decide when no input
//!   is read at all.
//!
//! An open input leaves undecided whether it stores a field no declaration
//! states ([`InputSchema::stores`]). Binding reads a slot as stored, as
//! every read a specification makes of data that matches its declarations
//! binds, and any other field as absent; a finding that turns on the answer
//! is withheld from `check()` ([`BindFinding::Withheld`]), as is every read
//! of a lookup such a finding alone leaves unbound. A kind no source decides
//! is compared with nothing.
use super::*;
use crate::ast::{ExprKind, JoinKey, Scope};
use std::collections::{BTreeMap, BTreeSet};

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
            kinds: schema
                .columns()
                .iter()
                .map(|column| Some(column.kind))
                .collect(),
            stated: vec![true; width],
            closed: true,
        }
    }
    /// The slots `fields` names, each `(name, kind, stated)`, in order.
    fn new(
        fields: Vec<(String, Option<ColumnType>, bool)>,
        closed: bool,
    ) -> Result<Self, BindError> {
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
        Self(
            schemas
                .iter()
                .map(|schema| InputSchema::exact(schema))
                .collect(),
        )
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
                    // The consumer's ordinal is its own, never the
                    // producer's (INPUT-3); preparation refuses one that
                    // names a field of the contract.
                    let stored = contract.fields();
                    let ordinal = source
                        .ordinal
                        .iter()
                        .filter(|ordinal| stored.iter().all(|field| field.name != **ordinal))
                        .map(|ordinal| (ordinal.clone(), Some(ColumnType::Int), true));
                    let fields = stored
                        .iter()
                        .map(|field| (field.name.clone(), Some(field.kind), true))
                        .chain(ordinal)
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

/// The input whose records the relation `relation` holds, through each
/// lookup over a lookup on the way, with the fields those lookups derive:
/// `None` for a pair relation, SELF or a name no input or lookup has.
fn reaches<'s>(spec: &'s ast::Spec, relation: &'s str) -> Option<(&'s str, Vec<&'s str>)> {
    let mut derived = Vec::new();
    let mut at = relation;
    for _ in 0..=spec.intermediates.len() {
        if spec.inputs.iter().any(|input| input.name.name == at) {
            return Some((at, derived));
        }
        let item = spec.intermediates.iter().find(|item| item.id.name == at)?;
        if item.pairs() {
            return None;
        }
        derived.extend(
            item.bindings
                .iter()
                .map(|binding| binding.name.name.as_str()),
        );
        at = &item.dataset.name;
    }
    None
}

/// The donor fields a key names, as written.
fn key_fields(key: Option<&JoinKey>) -> Vec<&str> {
    match key {
        Some(JoinKey::Columns { columns, .. }) => {
            columns.iter().map(|column| column.name.as_str()).collect()
        }
        Some(JoinKey::Pairs { pairs, .. }) => pairs
            .iter()
            .map(|(column, _)| column.name.as_str())
            .collect(),
        Some(JoinKey::FromInput) | None => Vec::new(),
    }
}

/// Every field the specification reads from the input `input`, in the
/// order it first reads each:
///
/// - the input's declared `key`, which an implicit join of it pairs;
/// - each field it reads of a relation that holds the input's records, the
///   input itself or a lookup over it, directly or over another lookup
///   ([`reaches`]), that no lookup on the way derives: a qualified name,
///   wherever it is read, but not one a lookup's own sites qualify by the
///   lookup, nor a field a lookup over the input derives, which a read
///   qualified by the input names in a nested key; a bare name a lookup
///   reads of its donor record in its derivations or range bounds (its
///   filter and order read a bare name of SELF alone); a bare name an
///   aggregate or structured source reads of its relation; a lookup's key,
///   `unique` columns, `columns` and nearest `source`; an aggregate's key;
///   and a pair relation's key fields of either side;
/// - when the input's records drive rows, directly or through a lookup,
///   the declared `key` of every other input, whose implicit join pairs it
///   with the driver's same-named field, and likewise for a lookup whose
///   sites read another keyed input.
fn read_fields(spec: &ast::Spec, input: &str, odm_items: &BTreeSet<ExprId>) -> Vec<String> {
    let mut fields = Vec::new();
    let mut add = |field: &str| {
        if field != "*" && !fields.iter().any(|known| known == field) {
            fields.push(field.to_owned());
        }
    };
    let stored = |relation: &str, field: &str| {
        reaches(spec, relation)
            .is_some_and(|(reached, derived)| reached == input && !derived.contains(&field))
    };
    // The fields lookups over the input derive, which a read qualified by
    // the input names in a lookup's nested key (LOOKUP-16).
    let derived = spec
        .intermediates
        .iter()
        .filter(|item| !item.pairs() && item.dataset.name == input)
        .flat_map(|item| &item.bindings)
        .map(|binding| binding.name.name.as_str())
        .collect::<BTreeSet<_>>();
    let keyed = |relation: &str| {
        spec.inputs
            .iter()
            .filter(|other| other.name.name != relation)
            .filter_map(|other| spec.input_key(&other.name.name))
            .flatten()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>()
    };
    for column in spec.input_key(input).unwrap_or_default() {
        add(&column.name);
    }
    let drivers = spec.rows.base.iter().chain(
        spec.rows
            .templates
            .iter()
            .filter_map(|template| template.driver.as_ref()),
    );
    for driver in drivers {
        for field in keyed(&driver.name) {
            if stored(&driver.name, field) {
                add(field);
            }
        }
    }
    let sites = spec.sites();
    let reads = sites
        .iter()
        .map(|site| (site, spec.reads(site.root, site.scope)))
        .collect::<Vec<_>>();
    // The relations each aggregate or structured source reads, by the names
    // that qualify its reads.
    let mut relations = BTreeMap::<ExprId, Vec<&str>>::new();
    for (_, reads) in &reads {
        for read in reads {
            if let (Scope::Relation(owner), Some(relation)) = (read.scope, read.name.owner()) {
                relations.entry(owner).or_default().push(relation);
            }
        }
    }
    let mut readers = BTreeSet::new();
    for (site, reads) in &reads {
        let donor_record = matches!(
            site.owner,
            ast::Owner::Binding { .. } | ast::Owner::Range { .. }
        );
        for read in reads {
            if odm_items.contains(&read.at) {
                continue;
            }
            let lookup = match read.scope {
                Scope::Donor(index) => spec.intermediates.get(index),
                _ => None,
            };
            match read.name.owner() {
                Some(owner) => {
                    let field = read.name.field();
                    if let Some(item) = lookup {
                        if spec.input_key(owner).is_some() && owner != item.dataset.name {
                            readers.insert(item.id.name.as_str());
                        }
                    }
                    let own = lookup.is_some_and(|item| owner == item.id.name);
                    let nested = owner == input && derived.contains(field);
                    if !own && !nested && stored(owner, field) {
                        add(field);
                    }
                }
                None => {
                    let name = read.name.as_str();
                    match (read.scope, lookup) {
                        (Scope::Donor(_), Some(item)) if donor_record => {
                            if stored(&item.id.name, name) {
                                add(name);
                            }
                        }
                        (Scope::Relation(owner), _) => {
                            let read_of = relations.get(&owner).into_iter().flatten();
                            if read_of.clone().any(|relation| stored(relation, name)) {
                                add(name);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    for reader in readers {
        for field in keyed(reader) {
            if stored(reader, field) {
                add(field);
            }
        }
    }
    for site in &sites {
        spec.walk(site.root, site.scope, |at, _| {
            if let ExprKind::Aggregate(aggregate) = &spec.expr(at).kind {
                for field in key_fields(aggregate.key.as_ref()) {
                    let read_of = relations.get(&at).into_iter().flatten();
                    if read_of.clone().any(|relation| stored(relation, field)) {
                        add(field);
                    }
                }
            }
            ast::Step::Continue
        });
    }
    for item in &spec.intermediates {
        if let Some(right) = &item.inner_join {
            let sides: &[&str] = match &item.key {
                Some(JoinKey::Columns { .. }) => &[&item.dataset.name, &right.name],
                _ => &[&right.name],
            };
            for field in key_fields(item.key.as_ref()) {
                if sides.iter().any(|side| stored(side, field)) {
                    add(field);
                }
            }
            continue;
        }
        let relation = &item.id.name;
        let named = key_fields(item.key.as_ref())
            .into_iter()
            .chain(
                item.checks
                    .iter()
                    .flat_map(|check| &check.columns)
                    .map(|column| column.name.as_str()),
            )
            .chain(
                item.columns
                    .iter()
                    .flatten()
                    .map(|column| column.name.as_str()),
            )
            .chain(
                item.nearest
                    .as_ref()
                    .map(|nearest| nearest.source.name.as_str()),
            );
        for field in named {
            if stored(relation, field) {
                add(field);
            }
        }
    }
    fields
}
