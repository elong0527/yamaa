//! Expression lowering: every expression to its node.
//!
//! An operation lowers at its written site, `<container>.<operation>`; a
//! normalized `value` wrapper adds no segment. Each operation reads its own
//! fields before it lowers any child, so a payload outside the language
//! becomes one [`ExprKind::Unsupported`] node without orphaned children.
//!
//! An expression field holds text in the closed grammar, a YAML scalar or
//! one operation (D18, D31). Text and scalars lower at the expression field
//! itself, except that a reference in a derivation keeps the canonical
//! `source` binding and its path (RFC 2.4).

use super::{column_type, keyword, Lower};
use crate::ast::{
    Aggregate, Arm, Between, Binding, Constant, Derivation, Dictionary, Direction, ExprId,
    ExprKind, Formula, Handlers, JoinKey, Keep, Literal, Meta, Name, Named, Nearest, Nulls,
    OdmRead, Operand, OrderTerm, Role, ScalarOp, Segment, Slot, SourceRead, Text, TimeUnit, Window,
    WindowFunction,
};
use crate::expression::{compile, CompiledExpression, Context, NodeKind, TemporalKind};
use crate::expression_table::{Closed, Form, OperandForm, Verb, Written};
use crate::schema::DocumentNode as N;

/// The kind of expression field a 1.0 reference is written in, which
/// decides its path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Placement {
    /// A derivation, a `derivations` entry, a handled wrapper's `value` or a
    /// case result: a bare reference is the canonical `source` binding, at
    /// `<field>.source`.
    Derivation,
    /// A key match value or a function argument: a reference is read at
    /// the field itself.
    Value,
}

impl<'d> Lower<'d> {
    /// Lower the expression written at `at`: text in the closed grammar, a
    /// YAML scalar or an operation mapping.
    pub(super) fn expression(&mut self, value: usize, at: Meta) -> ExprId {
        self.placed(value, at, Placement::Derivation)
    }

    /// [`Lower::expression`], with the reference path `placement` gives.
    pub(super) fn placed(&mut self, value: usize, at: Meta, placement: Placement) -> ExprId {
        match self.node(value) {
            N::Text(_) => self.text_expression(value, at, placement),
            N::Null | N::Boolean(_) | N::Integer(_) | N::Float(_) => {
                let here = self.written_at(at, value);
                self.push(ExprKind::Literal(self.literal(value)), here)
            }
            N::Mapping(entries) if entries.len() == 1 => {
                let (key, payload) = entries[0];
                let Some(operation) = self.text(key) else {
                    return self.push(ExprKind::Unsupported(String::new()), at);
                };
                let site = self.meta(at, keyword(operation), Some(value));
                match self.operation(operation, payload, site) {
                    Some(id) => id,
                    None => self.push(ExprKind::Unsupported(operation.into()), site),
                }
            }
            _ => self.push(ExprKind::Unsupported(String::new()), at),
        }
    }

    /// The operation `op` names, by the form its declaration gives it.
    fn operation(&mut self, op: &str, p: usize, site: Meta) -> Option<ExprId> {
        let verb = Verb::from_name(op)?;
        match verb.form() {
            Form::Scalar => self.scalar(verb, p, site),
            Form::Window => self.window(verb, p, site),
            Form::Structure => self.structure(verb, p, site),
        }
    }

    /// A verb whose node is a structure of its own.
    fn structure(&mut self, verb: Verb, p: usize, site: Meta) -> Option<ExprId> {
        match verb {
            Verb::Source => self.source(p, site),
            Verb::FirstAvailable => self.first_available(p, site),
            Verb::Case => self.case(p, site),
            Verb::Flag => self.flag(p, site),
            Verb::Aggregate => self.aggregate(p, site),
            Verb::Odm => self.odm(p, site),
            Verb::Function => self.call(p, site),
            _ => None,
        }
    }

    /// `first_available`: each source binding in order, then `missing`.
    fn first_available(&mut self, p: usize, site: Meta) -> Option<ExprId> {
        let sources = self.field(p, "sources")?;
        let id = self.reserve(site);
        let list = self.meta(site, Segment::Field("sources"), Some(sources));
        let mut lowered = Vec::new();
        for (index, &source) in self.items(sources).iter().enumerate() {
            let meta = self.meta(list, Segment::Index(index as u32), Some(source));
            lowered.push(self.source(source, meta).unwrap_or_else(|| {
                self.push(ExprKind::Unsupported(Verb::Source.name().into()), meta)
            }));
        }
        let missing = self.constant(site, p, "missing");
        Some(self.fill(
            id,
            ExprKind::FirstAvailable {
                sources: lowered,
                missing,
            },
        ))
    }

    /// A name read at `meta`.
    fn reference(&mut self, node: usize, meta: Meta) -> ExprId {
        match self.text(node) {
            Some(name) => self.push(ExprKind::Ref(Name(name.into())), meta),
            None => self.push(ExprKind::Unsupported(String::new()), meta),
        }
    }

    /// A list of names, such as `group_by`.
    pub(super) fn references(&mut self, list: usize, meta: Meta) -> Vec<ExprId> {
        let mut references = Vec::new();
        for (index, &item) in self.items(list).iter().enumerate() {
            let meta = self.meta(meta, Segment::Index(index as u32), Some(item));
            references.push(self.reference(item, meta));
        }
        references
    }

    /// A predicate field: grammar text whose site is its own field.
    pub(super) fn predicate(&mut self, node: usize, meta: Meta) -> ExprId {
        if self.text(node).is_none() {
            return self.push(ExprKind::Unsupported(String::new()), meta);
        }
        let path = self.render(meta);
        self.formula(Context::Predicate, node, (meta, meta), &path)
    }

    /// Text in an expression field (D18). Text holding only a name is a
    /// reference and text holding only a literal is that literal (FORMULA-9,
    /// EXPR-4); other text is a formula in the derivation context
    /// (FORMULA-27). The text is compiled once, at the expression field.
    fn text_expression(&mut self, node: usize, at: Meta, placement: Placement) -> ExprId {
        let text = self.text(node).unwrap_or_default();
        let here = self.written_at(at, node);
        let compiled = compile(text, Context::Derivation, &self.render(at));
        if let Ok(compiled) = &compiled {
            let syntax = compiled.syntax();
            let root = syntax.root();
            let leaf = match &syntax.nodes()[root].kind {
                NodeKind::Identifier => Some(ExprKind::Ref(Name(syntax.text(root).into()))),
                NodeKind::Number {
                    fractional: false,
                    text,
                } => Some(ExprKind::Literal(Literal::Int(text.clone()))),
                NodeKind::Number {
                    fractional: true,
                    text,
                } => text
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .map(|value| ExprKind::Literal(Literal::Float(value))),
                NodeKind::String(text) => Some(ExprKind::Literal(Literal::Text(text.clone()))),
                NodeKind::Temporal { kind, value } => Some(ExprKind::Literal(match kind {
                    TemporalKind::Date => Literal::Date(value.clone()),
                    TemporalKind::DateTime => Literal::DateTime(value.clone()),
                })),
                NodeKind::Null => Some(ExprKind::Literal(Literal::Missing)),
                _ => None,
            };
            match leaf {
                Some(kind @ ExprKind::Ref(_)) if placement == Placement::Derivation => {
                    let meta = self.meta(at, keyword(Verb::Source.name()), Some(node));
                    return self.push(kind, meta);
                }
                Some(kind) => return self.push(kind, here),
                None => {}
            }
        }
        self.compiled_formula(Context::Derivation, node, (here, here), compiled)
    }

    /// Grammar text compiled once, with each name it reads as a child.
    fn formula(
        &mut self,
        context: Context,
        node: usize,
        metas: (Meta, Meta),
        spec_path: &str,
    ) -> ExprId {
        let text = self.text(node).unwrap_or_default();
        let compiled = compile(text, context, spec_path);
        self.compiled_formula(context, node, metas, compiled)
    }

    /// The formula node of text `node` compiled as `compiled`.
    fn compiled_formula(
        &mut self,
        context: Context,
        node: usize,
        (site, text_meta): (Meta, Meta),
        compiled: Result<CompiledExpression, crate::expression::CompileError>,
    ) -> ExprId {
        let text = self.text(node).unwrap_or_default();
        let id = self.reserve(site);
        let mut names = Vec::new();
        if let Ok(compiled) = &compiled {
            for (role, name) in formula_names(compiled) {
                names.push((role, self.push(ExprKind::Ref(Name(name)), text_meta)));
            }
        }
        self.fill(
            id,
            ExprKind::Formula(Formula {
                context,
                text: Text {
                    text: text.into(),
                    meta: text_meta,
                },
                compiled,
                names,
            }),
        )
    }

    /// A source binding: a bare name, or a variable with its record
    /// selection. A lone variable is the canonical `source` binding of a
    /// bare name and lowers to a reference.
    fn source(&mut self, p: usize, site: Meta) -> Option<ExprId> {
        if let Some(name) = self.text(p) {
            return Some(self.push(ExprKind::Ref(Name(name.into())), site));
        }
        let variable = self
            .field(p, "variable")
            .filter(|&n| self.text(n).is_some())?;
        if self.entries(p).len() == 1 {
            let name = self.text(variable).unwrap_or_default();
            return Some(self.push(ExprKind::Ref(Name(name.into())), site));
        }
        let id = self.reserve(site);
        let variable_meta = self.meta(site, Segment::Field("variable"), Some(variable));
        let variable = self.reference(variable, variable_meta);
        let between = self.field(p, "between").map(|node| {
            let meta = self.meta(site, Segment::Field("between"), Some(node));
            self.between(node, meta)
        });
        let filter = self.field(p, "filter").map(|node| {
            let meta = self.meta(site, Segment::Field("filter"), Some(node));
            self.predicate(node, meta)
        });
        let order_by = self.order_by(p, "order_by", site);
        let keep = self.keep(p);
        let absent = self.constant(site, p, "absent");
        let require_match = self.boolean(p, "require_match", false);
        Some(self.fill(
            id,
            ExprKind::Source(SourceRead {
                variable,
                between,
                filter,
                order_by,
                keep,
                absent,
                require_match,
                written: self.written(
                    p,
                    &[
                        "between",
                        "filter",
                        "order_by",
                        "keep",
                        "absent",
                        "require_match",
                    ],
                ),
            }),
        ))
    }

    /// A record range: `{value, lower, upper, closed}`.
    pub(super) fn between(&mut self, node: usize, meta: Meta) -> Between {
        let part = |lower: &mut Self, name: &'static str| {
            lower.field(node, name).map(|value| {
                let meta = lower.meta(meta, Segment::Field(name), Some(value));
                lower.reference(value, meta)
            })
        };
        let value = part(self, "value")
            .unwrap_or_else(|| self.push(ExprKind::Unsupported("between".into()), meta));
        let lower = part(self, "lower");
        let upper = part(self, "upper");
        let closed = self
            .field(node, "closed")
            .and_then(|value| self.text(value))
            .and_then(Closed::from_name)
            .unwrap_or(Closed::Both);
        Between {
            value,
            lower,
            upper,
            closed,
            meta,
        }
    }

    /// A selection by distance: `{source, to, unit}` (LOOKUP-26). `to` is
    /// read as a key match value is: a name written alone is read at the
    /// field itself, and an operation at its keyword.
    pub(super) fn nearest(&mut self, node: usize, meta: Meta) -> Nearest {
        let source = self.named(meta, node, "source").unwrap_or(Named {
            name: String::new(),
            meta,
        });
        let (to, to_field) = match self.field(node, "to") {
            Some(value) => {
                let at = self.field_meta(meta, "to", value);
                (self.placed(value, at, Placement::Value), at)
            }
            None => (
                self.push(ExprKind::Unsupported("nearest".into()), meta),
                meta,
            ),
        };
        Nearest {
            source,
            to,
            to_field,
            unit: self.text_of(node, "unit").and_then(TimeUnit::from_name),
            meta,
        }
    }

    /// The order terms of `owner.name`: bare names or `{variable, direction, nulls}`.
    pub(super) fn order_by(
        &mut self,
        owner: usize,
        name: &'static str,
        parent: Meta,
    ) -> Vec<OrderTerm> {
        let Some(list) = self.field(owner, name) else {
            return Vec::new();
        };
        let meta = self.meta(parent, Segment::Field(name), Some(list));
        self.terms(list, meta)
    }

    fn terms(&mut self, list: usize, meta: Meta) -> Vec<OrderTerm> {
        self.terms_after(list, meta, 0)
    }

    /// The order terms of `list` from position `first`, each at its own
    /// position.
    fn terms_after(&mut self, list: usize, meta: Meta, first: usize) -> Vec<OrderTerm> {
        let mut terms = Vec::new();
        for (index, &term) in self.items(list).iter().enumerate().skip(first) {
            let meta = self.meta(meta, Segment::Index(index as u32), Some(term));
            let variable = match self.text(term) {
                Some(_) => term,
                None => self.field(term, "variable").unwrap_or(term),
            };
            terms.push(OrderTerm {
                variable: self.reference(variable, meta),
                direction: match self.text_of(term, "direction") {
                    Some("desc") => Direction::Desc,
                    _ => Direction::Asc,
                },
                nulls: match self.text_of(term, "nulls") {
                    Some("first") => Some(Nulls::First),
                    Some("last") => Some(Nulls::Last),
                    _ => None,
                },
                meta,
            });
        }
        terms
    }

    pub(super) fn keep(&self, owner: usize) -> Option<Keep> {
        match self.text_of(owner, "keep") {
            Some("first") => Some(Keep::First),
            Some("last") => Some(Keep::Last),
            _ => None,
        }
    }

    /// A written lookup key: donor columns, or donor columns with values.
    pub(super) fn join_key(&mut self, node: usize, meta: Meta) -> JoinKey {
        if let N::Mapping(entries) = self.node(node) {
            let mut pairs = Vec::new();
            for &(key, value) in entries {
                let Some(name) = self.text(key) else { continue };
                let pair = self.meta(meta, Segment::Key(name.into()), Some(value));
                let value = self.placed(value, pair, Placement::Value);
                pairs.push((self.named_at(name, pair), value));
            }
            return JoinKey::Pairs { pairs, meta };
        }
        let mut columns = Vec::new();
        for (index, &item) in self.items(node).iter().enumerate() {
            if let Some(name) = self.text(item) {
                let meta = self.meta(meta, Segment::Index(index as u32), Some(item));
                columns.push(self.named_at(name, meta));
            }
        }
        JoinKey::Columns { columns, meta }
    }

    fn case(&mut self, p: usize, site: Meta) -> Option<ExprId> {
        if !matches!(self.node(p), N::Sequence(_)) {
            return None;
        }
        let id = self.reserve(site);
        let mut arms = Vec::new();
        for (index, &item) in self.items(p).iter().enumerate() {
            let meta = self.meta(site, Segment::Index(index as u32), Some(item));
            let (condition, result) = match self.d.field(item, "otherwise") {
                Some(otherwise) => {
                    let at = self.meta(meta, Segment::Field("otherwise"), Some(otherwise));
                    (None, self.expression(otherwise, at))
                }
                None => {
                    let condition = self.field(item, "when").map(|when| {
                        let at = self.meta(meta, Segment::Field("when"), Some(when));
                        self.predicate(when, at)
                    });
                    let result = match self.d.field(item, "then") {
                        Some(then) => {
                            let at = self.meta(meta, Segment::Field("then"), Some(then));
                            self.expression(then, at)
                        }
                        None => self.push(ExprKind::Unsupported(Verb::Case.name().into()), meta),
                    };
                    (condition, result)
                }
            };
            arms.push(Arm {
                condition,
                result,
                meta,
            });
        }
        Some(self.fill(id, ExprKind::Case(arms)))
    }

    /// `flag`: a bare predicate, or `{condition, true_value, false_value, missing}`.
    fn flag(&mut self, p: usize, site: Meta) -> Option<ExprId> {
        let condition = self
            .payload_field(Verb::Flag, p, "condition")
            .filter(|&n| self.text(n).is_some())?;
        let id = self.reserve(site);
        let at = self.meta(site, Segment::Field("condition"), Some(condition));
        let condition = self.predicate(condition, at);
        let true_value = self.constant(site, p, "true_value").unwrap_or_else(|| {
            let meta = self.meta(site, Segment::Field("true_value"), None);
            let value = Verb::Flag.default_of("true_value");
            Constant {
                value: value.map_or(Literal::Missing, Literal::from),
                meta,
            }
        });
        let false_value = self.constant(site, p, "false_value");
        let missing = self.constant(site, p, "missing");
        Some(self.fill(
            id,
            ExprKind::Flag {
                condition,
                true_value,
                false_value,
                missing,
            },
        ))
    }

    /// A scalar verb: its parameters as its declaration types them, then
    /// each operand field it writes, in declared order, then its handlers.
    fn scalar(&mut self, verb: Verb, p: usize, site: Meta) -> Option<ExprId> {
        let op = ScalarOp::read(verb, &mut self.payload(verb, p, site))?;
        let fields = verb.operands();
        if fields
            .iter()
            .any(|field| field.required && self.payload_field(verb, p, field.field).is_none())
        {
            return None;
        }
        let id = self.reserve(site);
        let mut operands = Vec::new();
        for field in fields {
            let Some(node) = self.payload_field(verb, p, field.field) else {
                continue;
            };
            let (name, slot) = (field.field, field.slot);
            match field.form {
                OperandForm::Name => {
                    let meta = self.meta(site, Segment::Field(name), Some(node));
                    let expr = self.operand(node, meta);
                    operands.push(Operand { slot, expr });
                }
                OperandForm::Names => {
                    let list = self.meta(site, Segment::Field(name), Some(node));
                    for expr in self.references(node, list) {
                        operands.push(Operand { slot, expr });
                    }
                }
                OperandForm::Binding => {
                    let meta = self.meta(site, Segment::Field(name), Some(node));
                    let expr = match self.source(node, meta) {
                        Some(expr) => expr,
                        None => self.push(ExprKind::Unsupported(Verb::Source.name().into()), meta),
                    };
                    operands.push(Operand { slot, expr });
                }
                // A template written bare, or alone in its long form, which
                // is the expanded shorthand (SCHEMA-15), reads its
                // placeholders at the verb.
                OperandForm::Placeholders => {
                    let meta = if node == p || self.entries(p).len() == 1 {
                        site
                    } else {
                        self.meta(site, Segment::Field(name), Some(node))
                    };
                    let template = self.text(node).unwrap_or_default();
                    let parsed = crate::text_template::StringTemplate::new(
                        template.into(),
                        self.render(site),
                        None,
                    );
                    for name in parsed.iter().flat_map(|parsed| parsed.identifiers()) {
                        let expr = self.push(ExprKind::Ref(Name(name.clone())), meta);
                        operands.push(Operand { slot, expr });
                    }
                }
            }
        }
        let handlers = Handlers::read(verb, &mut self.payload(verb, p, site));
        Some(self.fill(
            id,
            ExprKind::Scalar {
                op,
                operands,
                handlers,
            },
        ))
    }

    /// The payload `p` of `verb`, as its generated readers read it.
    fn payload<'a>(&'a mut self, verb: Verb, p: usize, site: Meta) -> Payload<'a, 'd> {
        Payload {
            lower: self,
            verb,
            payload: p,
            site,
        }
    }

    /// The node of the field `name` in `verb`'s payload `p`, written and not
    /// null; a bare payload is the field it fills (SCHEMA-10).
    fn payload_field(&self, verb: Verb, p: usize, name: &str) -> Option<usize> {
        match self.node(p) {
            N::Mapping(_) => self.field(p, name),
            _ => (verb.shorthand() == Some(name)).then_some(p),
        }
    }

    /// An operand: a name, or a written number (`shift.days`, `shift.amount`).
    fn operand(&mut self, node: usize, meta: Meta) -> ExprId {
        match self.node(node) {
            N::Integer(_) | N::Float(_) => self.push(ExprKind::Literal(self.literal(node)), meta),
            _ => self.reference(node, meta),
        }
    }

    /// A mapping dictionary written at `node`, the field `name`: inline, or
    /// the path of a file holding it.
    fn dictionary(&mut self, node: usize, name: &'static str, site: Meta) -> Dictionary {
        let meta = self.meta(site, Segment::Field(name), Some(node));
        if let Some(path) = self.text(node) {
            return Dictionary::File(Text {
                text: path.into(),
                meta,
            });
        }
        let mut entries = Vec::new();
        for &(key, value) in self.entries(node) {
            let Some(key) = self.text(key) else { continue };
            let meta = self.meta(meta, Segment::Key(key.into()), Some(value));
            entries.push((
                Text {
                    text: key.into(),
                    meta,
                },
                Constant {
                    value: self.literal(value),
                    meta,
                },
            ));
        }
        Dictionary::Inline(entries)
    }

    /// A window verb: its parameters as its declaration types them, its
    /// operands, and its partitions, order and filter.
    fn window(&mut self, verb: Verb, p: usize, site: Meta) -> Option<ExprId> {
        let function = WindowFunction::read(verb, &mut self.payload(verb, p, site))?;
        // Each operand field written as a name, the analysis value `source`
        // first and then the others in declared order; a baseline's `date`
        // heads its order rather than being an operand (P-OP-2, P-OP-3).
        let mut fields = Vec::new();
        for field in verb.operands() {
            match self
                .field(p, field.field)
                .filter(|&n| self.text(n).is_some())
            {
                Some(node) => fields.push((field.field, field.slot, node)),
                None if field.required => return None,
                None => {}
            }
        }
        fields.sort_by_key(|&(_, slot, _)| slot != Slot::Source);
        let id = self.reserve(site);
        let mut operands = Vec::new();
        let mut date = None;
        for (name, slot, node) in fields {
            let meta = self.meta(site, Segment::Field(name), Some(node));
            if slot == Slot::Date {
                date = Some((node, meta));
            } else {
                let expr = self.reference(node, meta);
                operands.push(Operand { slot, expr });
            }
        }
        let window = match self.field(p, "window") {
            Some(node) if matches!(self.node(node), N::Mapping(_)) => {
                Some((node, self.meta(site, Segment::Field("window"), Some(node))))
            }
            _ => None,
        };
        let mut group_by = Vec::new();
        if let Some((node, meta)) = window {
            if let Some(list) = self.field(node, "group_by") {
                let meta = self.meta(meta, Segment::Field("group_by"), Some(list));
                group_by = self.references(list, meta);
            }
        }
        // The baseline orders eligible rows by the date and then the
        // tie-break terms (P-ORD-1); every other window by its own terms.
        // The tie-break terms follow the date in `window.order_by`, which
        // defaults to the date alone; a declared order that does not start
        // with the date ascending stays written, for its refusal (WINDOW-5).
        let declared = match (date, window) {
            (Some((date, _)), Some((node, _))) => self.field(node, "order_by").map(|list| {
                let head = self.items(list).first().is_some_and(|&term| {
                    let variable = match self.text(term) {
                        Some(_) => Some(term),
                        None => self.field(term, "variable"),
                    };
                    variable.and_then(|n| self.text(n)) == self.text(date)
                        && !matches!(self.text_of(term, "direction"), Some("desc"))
                });
                (list, head)
            }),
            _ => None,
        };
        let order_by = match date {
            Some((node, meta)) => {
                let mut order_by = vec![OrderTerm {
                    variable: self.reference(node, meta),
                    direction: Direction::Asc,
                    nulls: None,
                    meta,
                }];
                if let (Some((list, true)), Some((_, window_meta))) = (declared, window) {
                    let list_meta = self.meta(window_meta, Segment::Field("order_by"), Some(list));
                    order_by.extend(self.terms_after(list, list_meta, 1));
                }
                order_by
            }
            None => match window {
                Some((node, meta)) => self.order_by(node, "order_by", meta),
                None => Vec::new(),
            },
        };
        let mut filter = None;
        if let Some((node, meta)) = window {
            if let Some(text) = self.field(node, "filter") {
                let meta = self.meta(meta, Segment::Field("filter"), Some(text));
                filter = Some(self.predicate(text, meta));
            }
        }
        let mut window_order_by = Vec::new();
        if let (WindowFunction::BaselineFlag { .. }, Some((node, meta))) = (&function, window) {
            if !matches!(declared, Some((_, true))) {
                window_order_by = self.order_by(node, "order_by", meta);
            }
        }
        Some(self.fill(
            id,
            ExprKind::Window(Window {
                function,
                operands,
                group_by,
                order_by,
                filter,
                window_order_by,
            }),
        ))
    }

    fn aggregate(&mut self, p: usize, site: Meta) -> Option<ExprId> {
        let text = self
            .payload_field(Verb::Aggregate, p, "expr")
            .filter(|&n| self.text(n).is_some())?;
        let shorthand = text == p || self.entries(p).len() == 1;
        let id = self.reserve(site);
        let key = self.field(p, "key").map(|node| {
            let meta = self.meta(site, Segment::Field("key"), Some(node));
            self.join_key(node, meta)
        });
        let between = self.field(p, "between").map(|node| {
            let meta = self.meta(site, Segment::Field("between"), Some(node));
            self.between(node, meta)
        });
        let filter = self.field(p, "filter").map(|node| {
            let meta = self.meta(site, Segment::Field("filter"), Some(node));
            self.predicate(node, meta)
        });
        let group_by = match self.field(p, "group_by") {
            Some(list) => {
                let meta = self.meta(site, Segment::Field("group_by"), Some(list));
                self.references(list, meta)
            }
            None => Vec::new(),
        };
        // Per-record bindings: the `derivations` map keyed by name
        // (P-BIND-1).
        let derive = self.field(p, "derivations");
        let mut bindings = Vec::new();
        if let Some(list) = derive {
            let map_meta = self.meta(site, Segment::Field("derivations"), Some(list));
            for &(key, entry) in self.entries(list) {
                if let Some(name) = self.text(key) {
                    bindings.push(self.aggregate_derivation(name, entry, map_meta));
                }
            }
        }
        // The fields `STR_JOIN`, `COUNT(x)` and `QUANTILE` read besides
        // their argument (FORMULA-18); a shorthand writes none.
        let order_by = self.order_by(p, "order_by", site);
        let separator = self.text_field(site, p, "separator");
        let distinct = self
            .field(p, "distinct")
            .map(|node| matches!(self.node(node), N::Boolean(true)));
        let quantile = self.bound(site, p, "quantile");
        // A written long form never holds only `expr` (SCHEMA-15), so such
        // a payload is the expanded shorthand and its text is the site.
        let text_meta = if shorthand {
            site
        } else {
            self.meta(site, Segment::Field("expr"), Some(text))
        };
        let path = self.render(text_meta);
        let expr = self.formula(Context::Aggregate, text, (text_meta, text_meta), &path);
        // An aggregate reads a relation when a binding or a name of its
        // expression does; only such a qualified aggregate has a key.
        let qualified = derive.is_some()
            || match &self.exprs[expr.index()].kind {
                ExprKind::Formula(Formula {
                    compiled: Ok(compiled),
                    ..
                }) => {
                    compiled.identifiers().any(|name| name.contains('.'))
                        || compiled.record_relations().next().is_some()
                }
                _ => true,
            };
        // A keyless qualified aggregate matches on the input's declared `key`
        // (P-KEY-1).
        let key = key.or_else(|| qualified.then_some(JoinKey::FromInput));
        Some(self.fill(
            id,
            ExprKind::Aggregate(Aggregate {
                key,
                between,
                filter,
                group_by,
                bindings,
                order_by,
                separator,
                distinct,
                quantile,
                expr,
                written: self.written(
                    p,
                    &[
                        "derivations",
                        "order_by",
                        "separator",
                        "distinct",
                        "quantile",
                    ],
                ),
            }),
        ))
    }

    /// One `aggregate.derivations` entry: `NAME: {type, derivation}`
    /// (P-BIND-1).
    fn aggregate_derivation(&mut self, name: &str, entry: usize, map: Meta) -> Binding {
        let meta = self.meta(map, Segment::Key(name.into()), Some(entry));
        let derivation = match self.d.field(entry, "derivation") {
            Some(node) => {
                let derivation_meta = self.meta(meta, Segment::Field("derivation"), Some(node));
                self.derivation(node, derivation_meta)
            }
            None => Derivation {
                expr: self.push(ExprKind::Unsupported("derivations".into()), meta),
                unconvertible: None,
                meta,
            },
        };
        Binding {
            name: self.named_at(name, meta),
            declared: self.text_of(entry, "type").and_then(column_type),
            derivation,
            meta,
        }
    }

    /// `odm`: an item, written alone or with its selectors and filter.
    fn odm(&mut self, p: usize, site: Meta) -> Option<ExprId> {
        let item = self
            .payload_field(Verb::Odm, p, "item")
            .filter(|&n| self.text(n).is_some())?;
        let shorthand = item == p || self.entries(p).len() == 1;
        let id = self.reserve(site);
        let item_meta = if shorthand {
            site
        } else {
            self.meta(site, Segment::Field("item"), Some(item))
        };
        let item = self.reference(item, item_meta);
        let mut selector = |name| self.field(p, name).map(|_| self.texts(site, p, name));
        let event = selector("event");
        let form = selector("form");
        let item_group = selector("item_group");
        let filter = self.field(p, "filter").map(|node| {
            let meta = self.meta(site, Segment::Field("filter"), Some(node));
            self.predicate(node, meta)
        });
        Some(self.fill(
            id,
            ExprKind::Odm(OdmRead {
                item,
                event,
                form,
                item_group,
                filter,
            }),
        ))
    }

    /// `function`: a project call with named argument leaves.
    fn call(&mut self, p: usize, site: Meta) -> Option<ExprId> {
        let name = self.field(p, "name").filter(|&n| self.text(n).is_some())?;
        let id = self.reserve(site);
        let name_meta = self.meta(site, Segment::Field("name"), Some(name));
        let function = self.named_at(self.text(name).unwrap_or_default(), name_meta);
        let mut args = Vec::new();
        if let Some(list) = self.field(p, "args") {
            let list_meta = self.meta(site, Segment::Field("args"), Some(list));
            for &(key, value) in self.entries(list) {
                let Some(argument) = self.text(key) else {
                    continue;
                };
                let meta = self.meta(list_meta, Segment::Key(argument.into()), Some(value));
                let expr = match self.node(value) {
                    // An argument is one leaf of the grammar: a name, quoted
                    // text or a temporal literal (FUNC-6). Other text lowers
                    // to the formula it is, which the front end refuses.
                    N::Text(_) => self.placed(value, meta, Placement::Value),
                    _ => self.push(ExprKind::Literal(self.literal(value)), meta),
                };
                args.push((self.named_at(argument, meta), expr));
            }
        }
        let shape = crate::ast::CallShape {
            closed: matches!(self.node(p), N::Mapping(entries)
                if entries.len() == 2
                    && entries
                        .iter()
                        .all(|&(key, _)| matches!(self.text(key), Some("name" | "args")))),
            args: matches!(
                self.field(p, "args").map(|node| self.node(node)),
                Some(N::Mapping(_))
            ),
        };
        Some(self.fill(
            id,
            ExprKind::Call {
                function,
                args,
                shape,
            },
        ))
    }
}

/// A verb's payload as the generated readers read it ([`Written`]): each
/// field written and not null, or the bare payload as the field it fills.
struct Payload<'a, 'd> {
    lower: &'a mut Lower<'d>,
    verb: Verb,
    payload: usize,
    site: Meta,
}

impl Payload<'_, '_> {
    fn node(&self, field: &str) -> Option<usize> {
        self.lower.payload_field(self.verb, self.payload, field)
    }
}

impl Written for Payload<'_, '_> {
    fn int(&mut self, field: &'static str) -> Option<Option<String>> {
        let Some(node) = self.node(field) else {
            return Some(None);
        };
        match self.lower.node(node) {
            N::Integer(text) => Some(Some(text.clone())),
            _ => None,
        }
    }
    fn text(&mut self, field: &'static str) -> Option<Option<String>> {
        let Some(node) = self.node(field) else {
            return Some(None);
        };
        self.lower.text(node).map(|text| Some(text.into()))
    }
    fn boolean(&mut self, field: &'static str) -> Option<Option<bool>> {
        let Some(node) = self.node(field) else {
            return Some(None);
        };
        match self.lower.node(node) {
            N::Boolean(value) => Some(Some(*value)),
            _ => None,
        }
    }
    fn numbers(&mut self, field: &'static str) -> Option<Option<Vec<f64>>> {
        let Some(node) = self.node(field) else {
            return Some(None);
        };
        let mut numbers = Vec::new();
        for &item in self.lower.items(node) {
            numbers.push(match self.lower.node(item) {
                N::Float(value) => *value,
                N::Integer(text) => text.parse().ok()?,
                _ => return None,
            });
        }
        Some(Some(numbers))
    }
    fn texts(&mut self, field: &'static str) -> Option<Option<Vec<String>>> {
        let Some(node) = self.node(field) else {
            return Some(None);
        };
        let lower = &*self.lower;
        let texts = lower
            .items(node)
            .iter()
            .map(|&item| lower.text(item).map(String::from));
        texts.collect::<Option<Vec<_>>>().map(Some)
    }
    fn dictionary(&mut self, field: &'static str) -> Option<Option<Dictionary>> {
        let Some(node) = self.node(field) else {
            return Some(None);
        };
        if !matches!(self.lower.node(node), N::Mapping(_) | N::Text(_)) {
            return None;
        }
        Some(Some(self.lower.dictionary(node, field, self.site)))
    }
    fn constant(&mut self, field: &'static str) -> Option<Constant> {
        self.lower.constant(self.site, self.payload, field)
    }
}

/// Each distinct name a compiled text reads, in first written order: as an
/// operand where it occurs outside every reduction, as a reducer where it
/// occurs inside one, and each `D.*` relation `COUNT` reads as a reducer.
fn formula_names(compiled: &CompiledExpression) -> Vec<(Role, String)> {
    let syntax = compiled.syntax();
    let reductions = syntax
        .nodes()
        .iter()
        .filter(|node| matches!(node.kind, NodeKind::Reduction { .. }))
        .map(|node| node.span)
        .collect::<Vec<_>>();
    let mut inside = Vec::new();
    for (index, node) in syntax.nodes().iter().enumerate() {
        if node.kind == NodeKind::Identifier
            && reductions
                .iter()
                .any(|span| span.start <= node.span.start && node.span.end <= span.end)
        {
            inside.push(syntax.text(index));
        }
    }
    let outside = compiled.ungrouped_identifiers().collect::<Vec<_>>();
    let mut names = Vec::new();
    for name in compiled.identifiers() {
        if outside.contains(&name) {
            names.push((Role::Operand, name.to_string()));
        }
        if inside.contains(&name) {
            names.push((Role::Reducer, name.to_string()));
        }
    }
    for relation in compiled.record_relations() {
        names.push((Role::Reducer, format!("{relation}.*")));
    }
    names
}
