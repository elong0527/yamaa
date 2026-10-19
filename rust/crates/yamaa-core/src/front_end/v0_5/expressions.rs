//! Expression lowering: every 0.5 operation to its node.
//!
//! An operation lowers at its written site, `<container>.<operation>`,
//! the path today's compiler cites; a normalized `value` wrapper adds no
//! segment. Each operation reads its own fields before it lowers any child,
//! so a payload outside the language becomes one
//! [`ExprKind::Unsupported`] node without orphaned children.

use super::{column_type, keyword, Lower};
use crate::ast::{
    Aggregate, Arm, Between, Binding, Closed, DayRule, Derivation, Dictionary, Direction, ExprId,
    ExprKind, FlagPart, Formula, Handlers, JoinKey, Keep, Literal, Meta, Name, Nulls, OdmRead,
    Operand, OrderTerm, Precision, RankMethod, Role, ScalarOp, Segment, Slot, SourceRead,
    TemporalType, Text, TextCase, TimeRule, TimeUnit, Window, WindowFunction,
};
use crate::expression::{compile, CompiledExpression, Context, NodeKind};
use crate::schema::DocumentNode as N;

/// One scalar verb's operand fields, in its operand order.
type Fields = &'static [(&'static str, Slot, bool)];

const SOURCE: Fields = &[("source", Slot::Source, true)];

impl Lower<'_> {
    /// Lower the expression written at `at`: an operation mapping, or a
    /// bare name (a case result or key value outside normalization).
    pub(super) fn expression(&mut self, value: usize, at: Meta) -> ExprId {
        match self.node(value) {
            N::Text(name) => self.push(ExprKind::Ref(Name(name.clone())), at, value),
            N::Mapping(entries) if entries.len() == 1 => {
                let (key, payload) = entries[0];
                let Some(operation) = self.text(key) else {
                    return self.push(ExprKind::Unsupported(String::new()), at, value);
                };
                let site = self.meta(at, keyword(operation), Some(value));
                match self.operation(operation, payload, site, value) {
                    Some(id) => id,
                    None => self.push(ExprKind::Unsupported(operation.into()), site, value),
                }
            }
            _ => self.push(ExprKind::Unsupported(String::new()), at, value),
        }
    }

    fn operation(&mut self, op: &str, p: usize, site: Meta, origin: usize) -> Option<ExprId> {
        Some(match op {
            "literal" => self.push(ExprKind::Literal(self.literal(p)), site, origin),
            "source" => self.source(p, site, origin)?,
            "first_available" => {
                let sources = self.field(p, "sources")?;
                let id = self.reserve(site, origin);
                let list = self.meta(site, Segment::Field("sources"), Some(sources));
                let mut lowered = Vec::new();
                for (index, &source) in self.items(sources).iter().enumerate() {
                    let meta = self.meta(list, Segment::Index(index as u32), Some(source));
                    lowered.push(self.source(source, meta, source).unwrap_or_else(|| {
                        self.push(ExprKind::Unsupported("source".into()), meta, source)
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
            "case" => self.case(p, site, origin)?,
            "flag" => self.flag(p, site, origin)?,
            "compute" => {
                let text = self.field(p, "expr").filter(|&n| self.text(n).is_some())?;
                let text_meta = self.meta(site, Segment::Field("expr"), Some(text));
                let path = self.render(site);
                self.formula(Context::Compute, text, (site, text_meta), &path, origin)
            }
            "aggregate" => self.aggregate(p, site, origin)?,
            "odm" => self.odm(p, site, origin)?,
            "function" => self.call(p, site, origin)?,
            "row_number" | "rank" | "row_value" | "locf" | "baseline_flag" => {
                self.window(op, p, site, origin)?
            }
            _ => self.scalar(op, p, site, origin)?,
        })
    }

    /// A name read at `meta`.
    fn reference(&mut self, node: usize, meta: Meta) -> ExprId {
        match self.text(node) {
            Some(name) => self.push(ExprKind::Ref(Name(name.into())), meta, node),
            None => self.push(ExprKind::Unsupported(String::new()), meta, node),
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
            return self.push(ExprKind::Unsupported(String::new()), meta, node);
        }
        let path = self.render(meta);
        self.formula(Context::Predicate, node, (meta, meta), &path, node)
    }

    /// Grammar text compiled once, with each name it reads as a child.
    fn formula(
        &mut self,
        context: Context,
        node: usize,
        (site, text_meta): (Meta, Meta),
        spec_path: &str,
        origin: usize,
    ) -> ExprId {
        let text = self.text(node).unwrap_or_default();
        let compiled = compile(text, context, spec_path);
        let id = self.reserve(site, origin);
        let mut names = Vec::new();
        if let Ok(compiled) = &compiled {
            for (role, name) in formula_names(compiled) {
                names.push((role, self.push(ExprKind::Ref(Name(name)), text_meta, node)));
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
    fn source(&mut self, p: usize, site: Meta, origin: usize) -> Option<ExprId> {
        if let Some(name) = self.text(p) {
            return Some(self.push(ExprKind::Ref(Name(name.into())), site, origin));
        }
        let variable = self
            .field(p, "variable")
            .filter(|&n| self.text(n).is_some())?;
        if self.entries(p).len() == 1 {
            let name = self.text(variable).unwrap_or_default();
            return Some(self.push(ExprKind::Ref(Name(name.into())), site, origin));
        }
        let id = self.reserve(site, origin);
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
            .unwrap_or_else(|| self.push(ExprKind::Unsupported("between".into()), meta, node));
        let lower = part(self, "lower");
        let upper = part(self, "upper");
        Between {
            value,
            lower,
            upper,
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
        let mut terms = Vec::new();
        for (index, &term) in self.items(list).iter().enumerate() {
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
                let value = self.expression(value, pair);
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

    fn case(&mut self, p: usize, site: Meta, origin: usize) -> Option<ExprId> {
        if !matches!(self.node(p), N::Sequence(_)) {
            return None;
        }
        let id = self.reserve(site, origin);
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
                        None => self.push(ExprKind::Unsupported("case".into()), meta, item),
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
    fn flag(&mut self, p: usize, site: Meta, origin: usize) -> Option<ExprId> {
        let condition = if self.text(p).is_some() {
            p
        } else {
            self.field(p, "condition")
                .filter(|&n| self.text(n).is_some())?
        };
        let id = self.reserve(site, origin);
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

    fn scalar(&mut self, op: &str, p: usize, site: Meta, origin: usize) -> Option<ExprId> {
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
        let unit = |name| {
            Some(match text(name)? {
                "day" => TimeUnit::Day,
                "week" => TimeUnit::Week,
                "month" => TimeUnit::Month,
                "year" => TimeUnit::Year,
                "second" => TimeUnit::Second,
                "minute" => TimeUnit::Minute,
                "hour" => TimeUnit::Hour,
                _ => return None,
            })
        };
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
                    closed: match text("bounds") {
                        None | Some("exclusive") => Closed::Right,
                        Some("inclusive") => Closed::Both,
                        Some("between") => Closed::Neither,
                        Some(_) => return None,
                    },
                },
                &[("start", Slot::Start, true), ("end", Slot::End, true)],
            ),
            "study_day" => (
                ScalarOp::StudyDay,
                &[
                    ("date", Slot::Source, true),
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
                    minimum_source_precision: match text("minimum_source_precision") {
                        None => None,
                        Some("year") => Some(Precision::Year),
                        Some("month") => Some(Precision::Month),
                        Some("day") => Some(Precision::Day),
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
            "str_template" => return self.string_template(p, site, origin),
            "greatest" | "least" => {
                let sources = self.field(p, "sources")?;
                let id = self.reserve(site, origin);
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
            "mapping" | "cut" => return self.mapping_cut(op, p, site, origin),
            _ => return None,
        };
        if fields
            .iter()
            .any(|&(name, _, required)| required && self.field(p, name).is_none())
        {
            return None;
        }
        let id = self.reserve(site, origin);
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
            N::Integer(_) | N::Float(_) => {
                self.push(ExprKind::Literal(self.literal(node)), meta, node)
            }
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
    fn string_template(&mut self, p: usize, site: Meta, origin: usize) -> Option<ExprId> {
        let (node, shorthand) = match self.node(p) {
            N::Text(_) => (p, true),
            _ => (
                self.field(p, "template")
                    .filter(|&n| self.text(n).is_some())?,
                self.entries(p).len() == 1,
            ),
        };
        let template = self.text(node)?;
        let id = self.reserve(site, origin);
        let meta = if shorthand {
            site
        } else {
            self.meta(site, Segment::Field("template"), Some(node))
        };
        let parsed =
            crate::text_template::StringTemplate::new(template.into(), self.render(site), None);
        let mut operands = Vec::new();
        for name in parsed.iter().flat_map(|parsed| parsed.identifiers()) {
            let expr = self.push(ExprKind::Ref(Name(name.clone())), meta, node);
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

    fn mapping_cut(&mut self, op: &str, p: usize, site: Meta, origin: usize) -> Option<ExprId> {
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
        let id = self.reserve(site, origin);
        let source_meta = self.meta(site, Segment::Field("source"), Some(source));
        let expr = match self.source(source, source_meta, source) {
            Some(expr) => expr,
            None => self.push(ExprKind::Unsupported("source".into()), source_meta, source),
        };
        let op = match cut {
            Some((breaks, labels)) => ScalarOp::Cut {
                breaks,
                labels,
                closed: if self.boolean(p, "right", false) {
                    Closed::Right
                } else {
                    Closed::Left
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

    fn window(&mut self, op: &str, p: usize, site: Meta, origin: usize) -> Option<ExprId> {
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
            "baseline_flag" => (
                WindowFunction::BaselineFlag {
                    inclusive: self.boolean(p, "inclusive", true),
                    window_order_by: Vec::new(),
                },
                vec![
                    ("value", Slot::Source, required("value")?),
                    (
                        "reference_date",
                        Slot::Reference,
                        required("reference_date")?,
                    ),
                    ("date", Slot::Source, required("date")?),
                ],
            ),
            _ => return None,
        };
        let id = self.reserve(site, origin);
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
        let order_by = match date {
            Some((node, meta)) => {
                let mut order_by = vec![OrderTerm {
                    variable: self.reference(node, meta),
                    direction: Direction::Asc,
                    nulls: None,
                    meta,
                }];
                order_by.extend(self.order_by(p, "tie_break", site));
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
            *window_order_by = self.order_by(node, "order_by", meta);
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

    fn aggregate(&mut self, p: usize, site: Meta, origin: usize) -> Option<ExprId> {
        let (text, shorthand) = match self.node(p) {
            N::Text(_) => (p, true),
            _ => (
                self.field(p, "expr").filter(|&n| self.text(n).is_some())?,
                self.entries(p).len() == 1,
            ),
        };
        let id = self.reserve(site, origin);
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
        let derive = self.field(p, "derive");
        let mut bindings = Vec::new();
        if let Some(list) = derive {
            let list_meta = self.meta(site, Segment::Field("derive"), Some(list));
            for (index, &item) in self.items(list).iter().enumerate() {
                bindings.push(self.aggregate_binding(item, index, list_meta));
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
        let expr = self.formula(
            Context::Aggregate,
            text,
            (text_meta, text_meta),
            &path,
            text,
        );
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
        let key = key.or_else(|| qualified.then_some(JoinKey::Applicable));
        Some(self.fill(
            id,
            ExprKind::Aggregate(Aggregate {
                key,
                between,
                filter,
                group_by,
                bindings,
                expr,
            }),
        ))
    }

    /// One `aggregate.derive` entry: `{name, type, derivation}` (P-BIND-1).
    fn aggregate_binding(&mut self, item: usize, index: usize, list: Meta) -> Binding {
        let meta = self.meta(list, Segment::Index(index as u32), Some(item));
        let name = self.text_of(item, "name").unwrap_or_default();
        let name_meta = match self.field(item, "name") {
            Some(node) => self.meta(meta, Segment::Field("name"), Some(node)),
            None => meta,
        };
        let derivation = match self.field(item, "derivation") {
            Some(node) => {
                let derivation_meta = self.meta(meta, Segment::Field("derivation"), Some(node));
                if self.d.field(node, "value").is_some() {
                    self.derivation(node, derivation_meta)
                } else {
                    let expr = self.expression(node, derivation_meta);
                    Derivation {
                        expr,
                        unconvertible: None,
                        meta: derivation_meta,
                    }
                }
            }
            None => Derivation {
                expr: self.push(ExprKind::Unsupported("derive".into()), meta, item),
                unconvertible: None,
                meta,
            },
        };
        Binding {
            name: self.named_at(name, name_meta),
            declared: self.text_of(item, "type").and_then(column_type),
            derivation,
            meta,
        }
    }

    /// `odm`: an item, written alone or with its selectors and filter.
    fn odm(&mut self, p: usize, site: Meta, origin: usize) -> Option<ExprId> {
        let (item, shorthand) = match self.node(p) {
            N::Text(_) => (p, true),
            _ => (
                self.field(p, "item").filter(|&n| self.text(n).is_some())?,
                self.entries(p).len() == 1,
            ),
        };
        let id = self.reserve(site, origin);
        let item_meta = if shorthand {
            site
        } else {
            self.meta(site, Segment::Field("item"), Some(item))
        };
        let item = self.reference(item, item_meta);
        let event = self.texts(site, p, "event");
        let form = self.texts(site, p, "form");
        let item_group = self.texts(site, p, "item_group");
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
    fn call(&mut self, p: usize, site: Meta, origin: usize) -> Option<ExprId> {
        let name = self.field(p, "name").filter(|&n| self.text(n).is_some())?;
        let id = self.reserve(site, origin);
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
                    N::Text(_) => self.reference(value, meta),
                    N::Mapping(_) => {
                        let literal = self.argument_literal(value);
                        self.push(ExprKind::Literal(literal), meta, value)
                    }
                    _ => self.push(ExprKind::Literal(self.literal(value)), meta, value),
                };
                args.push((self.named_at(argument, meta), expr));
            }
        }
        Some(self.fill(id, ExprKind::Call { function, args }))
    }

    /// A tagged argument: `{literal: TEXT | null}`, `{date: TEXT}` or `{datetime: TEXT}`.
    fn argument_literal(&self, node: usize) -> Literal {
        let &[(key, value)] = self.entries(node) else {
            return Literal::Unsupported;
        };
        match (self.text(key), self.node(value)) {
            (Some("literal"), N::Null) => Literal::Missing,
            (Some("literal"), N::Text(text)) => Literal::Text(text.clone()),
            (Some("date"), N::Text(text)) => Literal::Date(text.clone()),
            (Some("datetime"), N::Text(text)) => Literal::DateTime(text.clone()),
            _ => Literal::Unsupported,
        }
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
