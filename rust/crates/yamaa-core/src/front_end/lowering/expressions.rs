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
    Aggregate, Arm, Between, Binding, Closed, DayRule, Derivation, Dictionary, Direction, ExprId,
    ExprKind, FlagPart, Formula, Handlers, JoinKey, Keep, Literal, Meta, Name, Named, Nearest,
    Nulls, OdmRead, Operand, OrderTerm, RankMethod, Role, ScalarOp, Segment, Slot, SourceRead,
    TemporalType, Text, TextCase, TimeRule, TimeUnit, Window, WindowFunction,
};
use crate::expression::{compile, CompiledExpression, Context, NodeKind, TemporalKind};
use crate::schema::DocumentNode as N;

/// One scalar verb's operand fields, in its operand order.
type Fields = &'static [(&'static str, Slot, bool)];

const SOURCE: Fields = &[("source", Slot::Source, true)];

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

impl Lower<'_> {
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

    fn operation(&mut self, op: &str, p: usize, site: Meta) -> Option<ExprId> {
        Some(match op {
            "source" => self.source(p, site)?,
            "first_available" => {
                let sources = self.field(p, "sources")?;
                let id = self.reserve(site);
                let list = self.meta(site, Segment::Field("sources"), Some(sources));
                let mut lowered = Vec::new();
                for (index, &source) in self.items(sources).iter().enumerate() {
                    let meta = self.meta(list, Segment::Index(index as u32), Some(source));
                    lowered.push(self.source(source, meta).unwrap_or_else(|| {
                        self.push(ExprKind::Unsupported("source".into()), meta)
                    }));
                }
                let missing = self.constant(site, p, "missing");
                self.fill(
                    id,
                    ExprKind::FirstAvailable {
                        sources: lowered,
                        missing,
                    },
                )
            }
            "case" => self.case(p, site)?,
            "flag" => self.flag(p, site)?,
            "aggregate" => self.aggregate(p, site)?,
            "odm" => self.odm(p, site)?,
            "function" => self.call(p, site)?,
            "row_number" | "rank" | "row_value" | "locf" | "baseline_flag" => {
                self.window(op, p, site)?
            }
            _ => self.scalar(op, p, site)?,
        })
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
                    let meta = self.meta(at, keyword("source"), Some(node));
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

    /// A record range: `{value, lower, upper}`.
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
        Between {
            value,
            lower,
            upper,
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
            unit: self.text_of(node, "unit").and_then(time_unit),
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
                        None => self.push(ExprKind::Unsupported("case".into()), meta),
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
        let condition = if self.text(p).is_some() {
            p
        } else {
            self.field(p, "condition")
                .filter(|&n| self.text(n).is_some())?
        };
        let id = self.reserve(site);
        let at = self.meta(site, Segment::Field("condition"), Some(condition));
        let condition = self.predicate(condition, at);
        let true_value = self.constant(site, p, "true_value").unwrap_or_else(|| {
            let meta = self.meta(site, Segment::Field("true_value"), None);
            crate::ast::Constant {
                value: Literal::Text("Y".into()),
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

    fn scalar(&mut self, op: &str, p: usize, site: Meta) -> Option<ExprId> {
        let text = |name| self.text_of(p, name);
        let integer = |name| match self.field(p, name).map(|n| self.node(n)) {
            Some(N::Integer(text)) => Some(text.clone()),
            _ => None,
        };
        let temporal = || match text("type")? {
            "date" => Some(TemporalType::Date),
            "datetime" => Some(TemporalType::DateTime),
            _ => None,
        };
        let unit = |name| text(name).and_then(time_unit);
        let (kind, fields): (ScalarOp, Fields) = match op {
            "round_half_away_from_zero" => (
                ScalarOp::Round {
                    digits: integer("digits")?,
                },
                SOURCE,
            ),
            "diff" => (
                ScalarOp::Diff {
                    kind: temporal()?,
                    unit: unit("unit")?,
                    // Endpoint inclusion defaults to the end only (P-ORD-2).
                    closed: match text("closed") {
                        None | Some("right") => Closed::Right,
                        Some("both") => Closed::Both,
                        Some("neither") => Closed::Neither,
                        Some(_) => return None,
                    },
                },
                &[("start", Slot::Start, true), ("end", Slot::End, true)],
            ),
            // `study_day` counts the date `source` names (P-OP-1).
            "study_day" => (
                ScalarOp::StudyDay,
                &[
                    ("source", Slot::Source, true),
                    ("reference", Slot::Reference, true),
                ],
            ),
            "to_epoch_day" => (ScalarOp::ToEpochDay, SOURCE),
            "shift" => (
                ScalarOp::Shift {
                    kind: temporal()?,
                    unit: match self.field(p, "unit") {
                        Some(_) => Some(unit("unit")?),
                        None => None,
                    },
                },
                &[
                    ("source", Slot::Source, true),
                    ("days", Slot::Amount, false),
                    ("amount", Slot::Amount, false),
                ],
            ),
            "duration_value" => (
                ScalarOp::DurationValue {
                    unit: unit("unit")?,
                },
                SOURCE,
            ),
            "duration_text" => (
                ScalarOp::DurationText {
                    unit: unit("unit")?,
                },
                SOURCE,
            ),
            "dtc" => (
                ScalarOp::Dtc,
                &[
                    ("year", Slot::Year, true),
                    ("month", Slot::Month, false),
                    ("day", Slot::Day, false),
                    ("hour", Slot::Hour, false),
                    ("minute", Slot::Minute, false),
                    ("second", Slot::Second, false),
                ],
            ),
            "time_of_day" => (ScalarOp::TimeOfDay, SOURCE),
            "impute" => (
                ScalarOp::Impute {
                    kind: temporal()?,
                    month: match self.field(p, "month") {
                        Some(_) => Some(integer("month")?),
                        None => None,
                    },
                    day: match self.field(p, "day").map(|n| self.node(n)) {
                        None => None,
                        Some(N::Integer(day)) => Some(DayRule::Day(day.clone())),
                        Some(N::Text(rule)) if rule == "first" => Some(DayRule::First),
                        Some(N::Text(rule)) if rule == "last" => Some(DayRule::Last),
                        Some(_) => return None,
                    },
                    time: match text("time") {
                        None => None,
                        Some("first") => Some(TimeRule::First),
                        Some("last") => Some(TimeRule::Last),
                        Some(_) => return None,
                    },
                },
                &[
                    ("source", Slot::Source, true),
                    ("not_before", Slot::NotBefore, false),
                    ("not_after", Slot::NotAfter, false),
                    ("fallback", Slot::Fallback, false),
                ],
            ),
            "precision" => (ScalarOp::Precision { kind: temporal()? }, SOURCE),
            "imputation_flag" => (
                ScalarOp::ImputationFlag {
                    part: match text("part")? {
                        "date" => FlagPart::Date,
                        "time" => FlagPart::Time,
                        _ => return None,
                    },
                    seconds_collected: self.boolean(p, "seconds_collected", true),
                },
                SOURCE,
            ),
            "to_date" => (ScalarOp::ToDate, SOURCE),
            "str_case" => (
                ScalarOp::StrCase {
                    to: match text("to")? {
                        "upper" => TextCase::Upper,
                        "lower" => TextCase::Lower,
                        "sentence" => TextCase::Sentence,
                        "title" => TextCase::Title,
                        _ => return None,
                    },
                },
                SOURCE,
            ),
            "str_pad" => (
                ScalarOp::StrPad {
                    width: integer("width")?,
                },
                SOURCE,
            ),
            "str_substr" => (
                ScalarOp::StrSubstr {
                    start: integer("start")?,
                    length: match self.field(p, "length") {
                        Some(_) => Some(integer("length")?),
                        None => None,
                    },
                },
                SOURCE,
            ),
            "str_length" => (ScalarOp::StrLength, SOURCE),
            "str_extract" => (
                ScalarOp::StrExtract {
                    pattern: text("pattern")?.into(),
                    group: match self.field(p, "group") {
                        Some(_) => integer("group")?,
                        None => "0".into(),
                    },
                },
                SOURCE,
            ),
            "str_replace" => (
                ScalarOp::StrReplace {
                    pattern: text("pattern")?.into(),
                    replacement: text("replacement")?.into(),
                },
                SOURCE,
            ),
            "str_template" => return self.string_template(p, site),
            "greatest" | "least" => {
                let sources = self.field(p, "sources")?;
                let id = self.reserve(site);
                let list = self.meta(site, Segment::Field("sources"), Some(sources));
                let operands = self
                    .references(sources, list)
                    .into_iter()
                    .map(|expr| Operand {
                        slot: Slot::Source,
                        expr,
                    })
                    .collect();
                let op = if op == "greatest" {
                    ScalarOp::Greatest
                } else {
                    ScalarOp::Least
                };
                return Some(self.fill(
                    id,
                    ExprKind::Scalar {
                        op,
                        operands,
                        handlers: Handlers::default(),
                    },
                ));
            }
            "mapping" | "cut" => return self.mapping_cut(op, p, site),
            _ => return None,
        };
        if fields
            .iter()
            .any(|&(name, _, required)| required && self.field(p, name).is_none())
        {
            return None;
        }
        let id = self.reserve(site);
        let mut operands = Vec::new();
        for &(name, slot, _) in fields {
            if let Some(node) = self.field(p, name) {
                let meta = self.meta(site, Segment::Field(name), Some(node));
                let expr = self.operand(node, meta);
                operands.push(Operand { slot, expr });
            }
        }
        let handlers = self.handlers(p, site);
        Some(self.fill(
            id,
            ExprKind::Scalar {
                op: kind,
                operands,
                handlers,
            },
        ))
    }

    /// An operand: a name, or a written number (`shift.days`, `shift.amount`).
    fn operand(&mut self, node: usize, meta: Meta) -> ExprId {
        match self.node(node) {
            N::Integer(_) | N::Float(_) => self.push(ExprKind::Literal(self.literal(node)), meta),
            _ => self.reference(node, meta),
        }
    }

    fn handlers(&mut self, p: usize, site: Meta) -> Handlers {
        Handlers {
            missing: self.constant(site, p, "missing"),
            invalid: self.constant(site, p, "invalid"),
            no_match: self.constant(site, p, "no_match"),
            unmapped: self.constant(site, p, "unmapped"),
        }
    }

    /// `str_template`: the template text, written alone or with `missing`;
    /// its placeholders are the operands.
    fn string_template(&mut self, p: usize, site: Meta) -> Option<ExprId> {
        let (node, shorthand) = match self.node(p) {
            N::Text(_) => (p, true),
            _ => (
                self.field(p, "template")
                    .filter(|&n| self.text(n).is_some())?,
                self.entries(p).len() == 1,
            ),
        };
        let template = self.text(node)?;
        let id = self.reserve(site);
        let meta = if shorthand {
            site
        } else {
            self.meta(site, Segment::Field("template"), Some(node))
        };
        let parsed =
            crate::text_template::StringTemplate::new(template.into(), self.render(site), None);
        let mut operands = Vec::new();
        for name in parsed.iter().flat_map(|parsed| parsed.identifiers()) {
            let expr = self.push(ExprKind::Ref(Name(name.clone())), meta);
            operands.push(Operand {
                slot: Slot::Source,
                expr,
            });
        }
        let handlers = self.handlers(p, site);
        Some(self.fill(
            id,
            ExprKind::Scalar {
                op: ScalarOp::StrTemplate {
                    template: template.into(),
                },
                operands,
                handlers,
            },
        ))
    }

    fn mapping_cut(&mut self, op: &str, p: usize, site: Meta) -> Option<ExprId> {
        let source = self.field(p, "source")?;
        let cut = if op == "cut" {
            let mut breaks = Vec::new();
            for &item in self.items(self.field(p, "breaks")?) {
                breaks.push(match self.node(item) {
                    N::Float(value) => *value,
                    N::Integer(text) => text.parse().ok()?,
                    _ => return None,
                });
            }
            let mut labels = Vec::new();
            for &item in self.items(self.field(p, "labels")?) {
                labels.push(self.text(item)?.to_string());
            }
            Some((breaks, labels))
        } else {
            let dictionary = self.field(p, "dict")?;
            if !matches!(self.node(dictionary), N::Mapping(_) | N::Text(_)) {
                return None;
            }
            None
        };
        let id = self.reserve(site);
        let source_meta = self.meta(site, Segment::Field("source"), Some(source));
        let expr = match self.source(source, source_meta) {
            Some(expr) => expr,
            None => self.push(ExprKind::Unsupported("source".into()), source_meta),
        };
        let op = match cut {
            Some((breaks, labels)) => ScalarOp::Cut {
                breaks,
                labels,
                // Intervals are left-closed unless `closed: right` (P-ORD-2).
                closed: match self.text_of(p, "closed") {
                    Some("right") => Closed::Right,
                    _ => Closed::Left,
                },
            },
            None => ScalarOp::Mapping {
                dict: self.dictionary(p, site),
                case_sensitive: self.boolean(p, "case_sensitive", true),
            },
        };
        let handlers = self.handlers(p, site);
        Some(self.fill(
            id,
            ExprKind::Scalar {
                op,
                operands: vec![Operand {
                    slot: Slot::Source,
                    expr,
                }],
                handlers,
            },
        ))
    }

    fn dictionary(&mut self, p: usize, site: Meta) -> Dictionary {
        let node = self.field(p, "dict").expect("a checked dictionary");
        let meta = self.meta(site, Segment::Field("dict"), Some(node));
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
                crate::ast::Constant {
                    value: self.literal(value),
                    meta,
                },
            ));
        }
        Dictionary::Inline(entries)
    }

    fn window(&mut self, op: &str, p: usize, site: Meta) -> Option<ExprId> {
        let text = |name| self.text_of(p, name);
        let required = |name| self.field(p, name).filter(|&n| self.text(n).is_some());
        let (mut function, fields): (WindowFunction, Vec<(&'static str, Slot, usize)>) = match op {
            "row_number" => (WindowFunction::RowNumber, Vec::new()),
            "rank" => (
                WindowFunction::Rank {
                    method: match text("method") {
                        None | Some("competition") => RankMethod::Competition,
                        Some("dense") => RankMethod::Dense,
                        Some(_) => return None,
                    },
                },
                Vec::new(),
            ),
            "row_value" => {
                let N::Integer(offset) = self.node(self.field(p, "offset")?) else {
                    return None;
                };
                (
                    WindowFunction::RowValue {
                        offset: offset.clone(),
                    },
                    vec![("source", Slot::Source, required("source")?)],
                )
            }
            "locf" => (
                WindowFunction::Locf {
                    include_current: self.boolean(p, "include_current", true),
                },
                vec![("source", Slot::Source, required("source")?)],
            ),
            // The analysis value is `source` and the date it is compared with
            // `reference` (P-OP-2, P-OP-3).
            "baseline_flag" => (
                WindowFunction::BaselineFlag {
                    inclusive: self.boolean(p, "inclusive", true),
                    window_order_by: Vec::new(),
                },
                vec![
                    ("source", Slot::Source, required("source")?),
                    ("reference", Slot::Reference, required("reference")?),
                    ("date", Slot::Source, required("date")?),
                ],
            ),
            _ => return None,
        };
        let id = self.reserve(site);
        let mut operands = Vec::new();
        let mut date = None;
        for (name, slot, node) in fields {
            let meta = self.meta(site, Segment::Field(name), Some(node));
            if name == "date" {
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
        if let (
            WindowFunction::BaselineFlag {
                window_order_by, ..
            },
            Some((node, meta)),
        ) = (&mut function, window)
        {
            if !matches!(declared, Some((_, true))) {
                *window_order_by = self.order_by(node, "order_by", meta);
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
            }),
        ))
    }

    fn aggregate(&mut self, p: usize, site: Meta) -> Option<ExprId> {
        let (text, shorthand) = match self.node(p) {
            N::Text(_) => (p, true),
            _ => (
                self.field(p, "expr").filter(|&n| self.text(n).is_some())?,
                self.entries(p).len() == 1,
            ),
        };
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
                expr,
                written: self.written(p, &["derivations"]),
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
        let (item, shorthand) = match self.node(p) {
            N::Text(_) => (p, true),
            _ => (
                self.field(p, "item").filter(|&n| self.text(n).is_some())?,
                self.entries(p).len() == 1,
            ),
        };
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

/// A unit as `diff` and `nearest` write it (TIME-4, LOOKUP-26).
fn time_unit(text: &str) -> Option<TimeUnit> {
    Some(match text {
        "day" => TimeUnit::Day,
        "week" => TimeUnit::Week,
        "month" => TimeUnit::Month,
        "year" => TimeUnit::Year,
        "second" => TimeUnit::Second,
        "minute" => TimeUnit::Minute,
        "hour" => TimeUnit::Hour,
        _ => return None,
    })
}
