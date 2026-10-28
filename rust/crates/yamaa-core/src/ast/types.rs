//! The one result-type table (EXPR-14): each expression's result type,
//! fixed by its verb, set by a written parameter, or taken from the types
//! of what it reads. Grammar text keeps its own table (VALUE-10,
//! [`CompiledExpression::result_type`]), which this one calls for a
//! formula's names.
//!
//! What an expression reads is typed by a [`TypeEnv`]: from the
//! declarations before any schema is captured, and from the captured
//! schemas afterwards. A binding an aggregate declares is the one name the
//! tree types itself, by the type it declares.
//!
//! [`CompiledExpression::result_type`]: crate::expression::CompiledExpression::result_type

use super::{ExprId, ExprKind, Formula, Literal, Name, Read, Scope, Slot, Spec, WindowFunction};
use crate::result_type::ResultType;
use crate::value::ValueType;
use std::collections::BTreeMap;

/// The types of what expressions read: each name in the scope it is read
/// in, and each project call by the return type its definition declares.
pub trait TypeEnv {
    fn read(&self, read: &Read) -> ResultType;
    /// A call no definition types is unknown.
    fn call(&self, _call: ExprId) -> ResultType {
        ResultType::UNKNOWN
    }
}

/// A function from a name to its type types every read by its name alone.
impl<F: Fn(&str) -> ResultType> TypeEnv for F {
    fn read(&self, read: &Read) -> ResultType {
        self(read.name.as_str())
    }
}

/// The result type of `root`, read in `scope`, over `env`.
pub fn result_type(spec: &Spec, root: ExprId, scope: Scope, env: &dyn TypeEnv) -> ResultType {
    Table::new(spec, root, scope, env).of(root)
}

/// The result types of one root's expressions, each name read in the scope
/// the traversal gives it.
pub struct Table<'a> {
    spec: &'a Spec,
    env: &'a dyn TypeEnv,
    scope: Scope,
    scopes: BTreeMap<ExprId, Scope>,
}

impl<'a> Table<'a> {
    pub fn new(spec: &'a Spec, root: ExprId, scope: Scope, env: &'a dyn TypeEnv) -> Self {
        let mut scopes = BTreeMap::new();
        spec.walk(root, scope, |at, scope| {
            scopes.insert(at, scope);
            super::Step::Continue
        });
        Self {
            spec,
            env,
            scope,
            scopes,
        }
    }

    /// The result type of `e`, an expression below the root. Handlers
    /// (`missing`, `invalid`, `absent`, `unmapped`, `no_match`) take no part.
    pub fn of(&self, e: ExprId) -> ResultType {
        let fold = |exprs: &mut dyn Iterator<Item = ExprId>, start: ResultType| {
            exprs.fold(start, |kind, e| kind.or(self.of(e)))
        };
        match &self.spec.expr(e).kind {
            ExprKind::Literal(value) => value.result_type(),
            ExprKind::Ref(name) => self.read(e, name),
            ExprKind::Formula(formula) => self.formula(formula),
            ExprKind::Source(source) => self.of(source.variable),
            ExprKind::Case(arms) => {
                fold(&mut arms.iter().map(|arm| arm.result), ResultType::MISSING)
            }
            ExprKind::Flag {
                true_value,
                false_value,
                missing,
                ..
            } => [Some(true_value), false_value.as_ref(), missing.as_ref()]
                .into_iter()
                .flatten()
                .fold(ResultType::MISSING, |kind, value| {
                    kind.or(value.value.result_type())
                }),
            ExprKind::FirstAvailable { sources, missing } => fold(
                &mut sources.iter().copied(),
                missing
                    .as_ref()
                    .map_or(ResultType::MISSING, |value| value.value.result_type()),
            ),
            ExprKind::Scalar { op, operands, .. } => {
                op.result_type(&mut operands.iter().map(|operand| self.of(operand.expr)))
            }
            ExprKind::Window(window) => match &window.function {
                WindowFunction::RowNumber | WindowFunction::Rank { .. } => ResultType::INT,
                WindowFunction::RowValue { .. } | WindowFunction::Locf { .. } => window
                    .operands
                    .iter()
                    .find(|operand| operand.slot == Slot::Source)
                    .map_or(ResultType::UNKNOWN, |operand| self.of(operand.expr)),
                WindowFunction::BaselineFlag { .. } => ResultType::STR,
            },
            ExprKind::Aggregate(aggregate) => self.of(aggregate.expr),
            ExprKind::Odm(_) => ResultType::STR,
            ExprKind::Call { .. } => self.env.call(e),
            ExprKind::Unsupported(_) => ResultType::UNKNOWN,
        }
    }

    /// The scope a name below the root is read in; any other expression
    /// reads in the root's.
    fn scope(&self, e: ExprId) -> Scope {
        self.scopes.get(&e).copied().unwrap_or(self.scope)
    }

    fn read(&self, at: ExprId, name: &Name) -> ResultType {
        match self.scope(at) {
            Scope::Local(aggregate) => self.binding(aggregate, name),
            scope => self.env.read(&Read {
                name: name.clone(),
                at,
                scope,
            }),
        }
    }

    /// A binding's declared type.
    fn binding(&self, aggregate: ExprId, name: &Name) -> ResultType {
        match &self.spec.expr(aggregate).kind {
            ExprKind::Aggregate(aggregate) => aggregate
                .bindings
                .iter()
                .find(|binding| binding.name.name == name.as_str())
                .and_then(|binding| binding.declared)
                .map_or(ResultType::UNKNOWN, ResultType::column),
            _ => ResultType::UNKNOWN,
        }
    }

    /// VALUE-10 and FORMULA-23 over the types of the names the text reads.
    /// A name the text reads inside a reduction and outside one has one
    /// type; a binding's is its declared type.
    fn formula(&self, formula: &Formula) -> ResultType {
        let Ok(compiled) = &formula.compiled else {
            return ResultType::UNKNOWN;
        };
        compiled.result_type(|name| {
            let reads = formula
                .names
                .iter()
                .map(|&(_, child)| child)
                .filter(|&child| {
                    matches!(&self.spec.expr(child).kind, ExprKind::Ref(read) if read.as_str() == name)
                })
                .collect::<Vec<_>>();
            reads
                .iter()
                .copied()
                .find(|&child| matches!(self.scope(child), Scope::Local(_)))
                .or(reads.first().copied())
                .map_or(ResultType::UNKNOWN, |child| self.of(child))
        })
    }
}

impl Literal {
    /// A written scalar's type; a value outside the language has none known.
    pub fn result_type(&self) -> ResultType {
        match self {
            Literal::Missing => ResultType::MISSING,
            Literal::Bool(_) => ResultType::of(ValueType::Bool),
            Literal::Int(_) => ResultType::INT,
            Literal::Float(value) if value.is_finite() => ResultType::FLOAT,
            Literal::Float(_) => ResultType::MISSING,
            Literal::Text(_) => ResultType::STR,
            Literal::Date(_) => ResultType::DATE,
            Literal::DateTime(_) => ResultType::DATETIME,
            Literal::Unsupported => ResultType::UNKNOWN,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ExprKind, Scope};
    use crate::test_document::Tree;
    use crate::test_specification::{column, derivation, document, lower, op};

    fn spec(columns: Vec<Tree>) -> Spec {
        lower(document(columns, vec![]))
    }
    fn root(spec: &Spec, column: usize) -> ExprId {
        spec.columns[column].derivation.as_ref().unwrap().expr
    }
    /// `A` is an int, `B` a float, `T` text and `D` a date; `CALL` returns a
    /// date; anything else is unknown.
    struct Env;
    impl TypeEnv for Env {
        fn read(&self, read: &Read) -> ResultType {
            match read.name.field() {
                "A" => ResultType::INT,
                "B" => ResultType::FLOAT,
                "T" => ResultType::STR,
                "D" => ResultType::DATE,
                _ => ResultType::UNKNOWN,
            }
        }
        fn call(&self, _call: ExprId) -> ResultType {
            ResultType::DATE
        }
    }
    fn types(columns: Vec<Tree>) -> Vec<ResultType> {
        let spec = spec(columns);
        (1..spec.columns.len())
            .map(|column| result_type(&spec, root(&spec, column), Scope::Row, &Env))
            .collect()
    }
    fn value(name: &str, value: Tree) -> Tree {
        column(name, "str", derivation(value))
    }
    fn fields(pairs: Vec<(&str, Tree)>) -> Tree {
        Tree::map(pairs)
    }

    #[test]
    fn verbs_fix_or_set_their_result_type() {
        let temporal = |verb: &str, kind: &str, unit: &str| {
            op(
                verb,
                fields(vec![
                    ("type", Tree::text(kind)),
                    ("start", Tree::text("SRC.D")),
                    ("end", Tree::text("SRC.D")),
                    ("source", Tree::text("SRC.D")),
                    ("unit", Tree::text(unit)),
                ]),
            )
        };
        let source = |verb: &str| op(verb, fields(vec![("source", Tree::text("SRC.T"))]));
        assert_eq!(
            types(vec![
                value("DAYS", temporal("diff", "date", "month")),
                value("SECONDS", temporal("diff", "datetime", "second")),
                value("HOURS", temporal("diff", "datetime", "hour")),
                value(
                    "SDY",
                    op(
                        "study_day",
                        fields(vec![
                            ("source", Tree::text("SRC.D")),
                            ("reference", Tree::text("SRC.D")),
                        ]),
                    ),
                ),
                value("SHIFTED", temporal("shift", "datetime", "hour")),
                value("DURATION", temporal("duration_value", "datetime", "hour")),
                value("WRITTEN", temporal("duration_text", "datetime", "minute")),
                value(
                    "DTC",
                    op("dtc", fields(vec![("year", Tree::text("SRC.A"))]))
                ),
                value("IMPUTED", temporal("impute", "date", "day")),
                value("DATE", source("to_date")),
                value(
                    "READ",
                    op(
                        "date_text",
                        fields(vec![
                            ("source", Tree::text("SRC.T")),
                            ("forms", Tree::list(vec![Tree::text("iso")])),
                        ]),
                    ),
                ),
                value(
                    "CASED",
                    op(
                        "str_case",
                        fields(vec![
                            ("source", Tree::text("SRC.T")),
                            ("to", Tree::text("upper"))
                        ])
                    )
                ),
                value(
                    "PIECE",
                    op(
                        "str_substr",
                        fields(vec![
                            ("source", Tree::text("SRC.T")),
                            ("start", Tree::integer(1))
                        ])
                    )
                ),
                value("LENGTH", source("str_length")),
                value(
                    "ROUNDED",
                    op(
                        "round_half_away_from_zero",
                        fields(vec![
                            ("source", Tree::text("SRC.B")),
                            ("digits", Tree::integer(1))
                        ]),
                    ),
                ),
                value(
                    "DECIMAL",
                    op(
                        "str_decimal",
                        fields(vec![
                            ("source", Tree::text("SRC.B")),
                            ("decimals", Tree::integer(2))
                        ]),
                    ),
                ),
                value(
                    "DECODED",
                    op(
                        "decode",
                        fields(vec![
                            ("source", Tree::text("SRC.T")),
                            ("codelist", Tree::text("PARAMCD"))
                        ]),
                    ),
                ),
                value("NUMBER", op("row_number", fields(vec![]))),
                value("ODM", op("odm", Tree::text("ODM.IT"))),
                value(
                    "CALLED",
                    op("function", fields(vec![("name", Tree::text("CALL"))]))
                ),
            ]),
            [
                ResultType::INT,
                ResultType::INT,
                ResultType::FLOAT,
                ResultType::INT,
                ResultType::DATETIME,
                ResultType::FLOAT,
                ResultType::STR,
                ResultType::STR,
                ResultType::DATE,
                ResultType::DATE,
                ResultType::STR,
                ResultType::STR,
                ResultType::STR,
                ResultType::INT,
                ResultType::FLOAT,
                ResultType::STR,
                ResultType::STR,
                ResultType::INT,
                ResultType::STR,
                ResultType::DATE,
            ]
        );
    }

    #[test]
    fn selections_take_every_type_they_can_select() {
        let case = op(
            "case",
            Tree::list(vec![
                fields(vec![
                    ("when", Tree::text("SRC.A > 1")),
                    ("then", Tree::integer(1)),
                ]),
                fields(vec![("otherwise", Tree::text("SRC.B"))]),
            ]),
        );
        let first = op(
            "first_available",
            fields(vec![
                (
                    "sources",
                    Tree::list(vec![fields(vec![("variable", Tree::text("SRC.D"))])]),
                ),
                ("missing", Tree::text("none")),
            ]),
        );
        let flag = op(
            "flag",
            fields(vec![
                ("condition", Tree::text("SRC.A > 1")),
                ("false_value", Tree::integer(0)),
                ("missing", Tree::Scalar(crate::schema::DocumentNode::Null)),
            ]),
        );
        let mapping = op(
            "mapping",
            fields(vec![
                ("source", Tree::text("SRC.T")),
                (
                    "dict",
                    fields(vec![("a", Tree::integer(1)), ("b", Tree::text("x"))]),
                ),
            ]),
        );
        let extremes = op(
            "greatest",
            fields(vec![(
                "sources",
                Tree::list(vec![Tree::text("SRC.A"), Tree::text("SRC.B")]),
            )]),
        );
        let neighbor = op(
            "row_value",
            fields(vec![
                ("source", Tree::text("SRC.D")),
                ("offset", Tree::integer(-1)),
            ]),
        );
        assert_eq!(
            types(vec![
                // A YAML null or Boolean is itself a literal (D18).
                value("MISSING", Tree::Scalar(crate::schema::DocumentNode::Null)),
                value("TRUE", Tree::boolean(true)),
                value("CASE", case),
                value("FIRST", first),
                value("FLAG", flag),
                value("MAPPED", mapping),
                value("GREATEST", extremes),
                value("PRIOR", neighbor),
            ]),
            [
                ResultType::MISSING,
                ResultType::of(ValueType::Bool),
                ResultType::NUMBER,
                ResultType::DATE.or(ResultType::STR),
                ResultType::STR.or(ResultType::INT),
                ResultType::INT.or(ResultType::STR),
                ResultType::NUMBER,
                ResultType::DATE,
            ]
        );
    }

    /// The grammar's table types grammar text; an aggregate's binding is
    /// typed by its declaration, whatever a name of the row would be.
    #[test]
    fn grammar_text_and_aggregate_bindings_are_typed_by_the_tree() {
        let aggregate = op(
            "aggregate",
            fields(vec![
                (
                    "derivations",
                    fields(vec![(
                        "T",
                        fields(vec![
                            ("type", Tree::text("float")),
                            ("derivation", derivation(Tree::text("EX.B"))),
                        ]),
                    )]),
                ),
                ("expr", Tree::text("SUM(T) + COUNT(EX.*)")),
            ]),
        );
        let spec = spec(vec![
            value("SUMMED", Tree::text("A + SRC.A")),
            value("HALF", Tree::text("A / 2")),
            value("TOTAL", aggregate),
        ]);
        let typed = |column| result_type(&spec, root(&spec, column), Scope::Row, &Env);
        assert_eq!(typed(1), ResultType::INT);
        assert_eq!(typed(2), ResultType::FLOAT);
        assert_eq!(typed(3), ResultType::FLOAT);
        // The binding's derivation reads the relation; the table answers it
        // within the aggregate's site.
        let ExprKind::Aggregate(aggregate) = &spec.expr(root(&spec, 3)).kind else {
            panic!("an aggregate");
        };
        let table = Table::new(&spec, root(&spec, 3), Scope::Row, &Env);
        assert_eq!(
            table.of(aggregate.bindings[0].derivation.expr),
            ResultType::FLOAT
        );
        // A name typed alone has its type wherever it is read.
        let names = |name: &str| match name {
            "A" => ResultType::STR,
            _ => ResultType::UNKNOWN,
        };
        assert_eq!(
            result_type(&spec, root(&spec, 1), Scope::Row, &names),
            ResultType::NUMBER
        );
    }
}
