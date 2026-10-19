//! The one traversal. [`Spec::children`] is the only match on an
//! expression's shape; [`Spec::walk`], [`Spec::reads`], [`Spec::calls`] and
//! every analysis derive from it, and [`Spec::sites`] lists every root.
//!
//! A child's role sets the scope its names read in: the current row, the
//! parent's relation (a lookup filter, a reducer, range bounds, aggregate
//! bindings), the donor record (an intermediate's own declarations), or the
//! local bindings of an aggregate.

use super::{DatasetCheckKind, ExprId, ExprKind, JoinKey, Name, Spec, WindowFunction};

/// Why an expression is a child of its parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// A value the parent computes from, a function argument, or a name a
    /// formula reads outside any reduction.
    Operand,
    /// A predicate choosing a branch, a flag value or window rows.
    Condition,
    /// A case result.
    Branch,
    /// A predicate over the parent's relation.
    Relation,
    /// A current-row value matched against a key column.
    KeyValue,
    /// The current-row value of a record range.
    RangeValue,
    /// A donor-record field bounding a record range.
    RangeBound,
    /// A name a formula reads inside a reduction.
    Reducer,
    /// A per-record binding of an aggregate.
    Binding,
    /// A grouping key.
    Group,
    /// An order term.
    Order,
}

/// Where the names below an expression are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The row being derived: the current row, or a grouped row's records.
    Row,
    /// The records of the relation the expression owns: an aggregate, a
    /// structured source or an ODM read.
    Relation(ExprId),
    /// The donor record of an intermediate (an index into
    /// [`Spec::intermediates`]), in its filter, order, bounds and bindings.
    Donor(usize),
    /// A binding the aggregate declares.
    Local(ExprId),
}

/// One name a walk reads, where and in which scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Read {
    pub name: Name,
    pub at: ExprId,
    pub scope: Scope,
}

/// An expression root, its owner and the scope it is read in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Site {
    pub root: ExprId,
    pub owner: Owner,
    pub scope: Scope,
}

/// What an expression root belongs to. Indices address the spec's lists:
/// columns, row templates (and their overrides), intermediates (and their
/// bindings) and dataset checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    /// A column's derivation, the default of every row template.
    Default {
        column: usize,
    },
    /// A row template's own derivation of one column.
    Override {
        template: usize,
        entry: usize,
    },
    Filter(Filtered),
    /// A row template's grouping key.
    Group {
        template: usize,
    },
    /// A current-row value an intermediate's key matches.
    Match {
        intermediate: usize,
    },
    /// An intermediate's record range: the value or a bound.
    Range {
        intermediate: usize,
    },
    Order(Ordered),
    /// An intermediate's derivation.
    Binding {
        intermediate: usize,
        binding: usize,
    },
    /// A dataset check's predicate.
    Check {
        check: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filtered {
    /// The root `filter` of a spec without row templates.
    Base,
    Template(usize),
    Intermediate(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ordered {
    Intermediate(usize),
    Output,
}

/// What a walk does after visiting an expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Continue,
    /// Do not visit this expression's children.
    Skip,
    /// End the walk.
    Stop,
}

/// An expression's children in written order, each with its role.
#[derive(Debug)]
pub struct Children(std::vec::IntoIter<(Role, ExprId)>);

impl Iterator for Children {
    type Item = (Role, ExprId);
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

#[derive(Clone, Copy)]
struct Frame {
    expr: ExprId,
    scope: Scope,
    /// The innermost aggregate, whose relation a reducer reads.
    relation: Option<ExprId>,
    /// The innermost aggregate whose bindings are being walked.
    bindings: Option<ExprId>,
}

impl Spec {
    /// The one match on an expression's shape.
    pub fn children(&self, e: ExprId) -> Children {
        let mut out = Vec::new();
        let between = |out: &mut Vec<(Role, ExprId)>, between: &Option<super::Between>| {
            if let Some(between) = between {
                out.push((Role::RangeValue, between.value));
                for bound in [between.lower, between.upper].into_iter().flatten() {
                    out.push((Role::RangeBound, bound));
                }
            }
        };
        match &self.expr(e).kind {
            ExprKind::Literal(_) | ExprKind::Ref(_) | ExprKind::Unsupported(_) => {}
            ExprKind::Formula(formula) => out.extend(formula.names.iter().copied()),
            ExprKind::Source(source) => {
                out.push((Role::Operand, source.variable));
                between(&mut out, &source.between);
                out.extend(source.filter.map(|filter| (Role::Relation, filter)));
                out.extend(source.order_by.iter().map(|t| (Role::Order, t.variable)));
            }
            ExprKind::Case(arms) => {
                for arm in arms {
                    out.extend(arm.condition.map(|condition| (Role::Condition, condition)));
                    out.push((Role::Branch, arm.result));
                }
            }
            ExprKind::Flag { condition, .. } => out.push((Role::Condition, *condition)),
            ExprKind::FirstAvailable { sources, .. } => {
                out.extend(sources.iter().map(|&source| (Role::Operand, source)));
            }
            ExprKind::Scalar { operands, .. } => {
                out.extend(operands.iter().map(|operand| (Role::Operand, operand.expr)));
            }
            ExprKind::Window(window) => {
                out.extend(window.operands.iter().map(|o| (Role::Operand, o.expr)));
                out.extend(window.group_by.iter().map(|&group| (Role::Group, group)));
                out.extend(window.order_by.iter().map(|t| (Role::Order, t.variable)));
                out.extend(window.filter.map(|filter| (Role::Condition, filter)));
                if let WindowFunction::BaselineFlag {
                    window_order_by, ..
                } = &window.function
                {
                    out.extend(window_order_by.iter().map(|t| (Role::Order, t.variable)));
                }
            }
            ExprKind::Aggregate(aggregate) => {
                if let Some(JoinKey::Pairs { pairs, .. }) = &aggregate.key {
                    out.extend(pairs.iter().map(|&(_, value)| (Role::KeyValue, value)));
                }
                between(&mut out, &aggregate.between);
                out.extend(aggregate.filter.map(|filter| (Role::Relation, filter)));
                out.extend(aggregate.group_by.iter().map(|&group| (Role::Group, group)));
                out.extend(
                    aggregate
                        .bindings
                        .iter()
                        .map(|binding| (Role::Binding, binding.derivation.expr)),
                );
                out.push((Role::Operand, aggregate.expr));
            }
            ExprKind::Odm(read) => {
                out.push((Role::Operand, read.item));
                out.extend(read.filter.map(|filter| (Role::Relation, filter)));
            }
            ExprKind::Call { args, .. } => {
                out.extend(args.iter().map(|&(_, value)| (Role::Operand, value)));
            }
        }
        Children(out.into_iter())
    }

    /// Every expression root in declaration order: column defaults, the
    /// root filter, each row template's groups, filter and overrides, each
    /// intermediate's key values, range, filter, order and bindings, the
    /// dataset checks' predicates and the output order. Every expression
    /// of the arena is reachable from exactly one root.
    pub fn sites(&self) -> Vec<Site> {
        let mut sites = Vec::new();
        let mut push = |root, owner, scope| sites.push(Site { root, owner, scope });
        for (column, declared) in self.columns.iter().enumerate() {
            if let Some(derivation) = &declared.derivation {
                push(derivation.expr, Owner::Default { column }, Scope::Row);
            }
        }
        if let Some(filter) = self.rows.filter {
            push(filter, Owner::Filter(Filtered::Base), Scope::Row);
        }
        for (template, row) in self.rows.templates.iter().enumerate() {
            for &group in row.group_by.iter().flatten() {
                push(group, Owner::Group { template }, Scope::Row);
            }
            if let Some(filter) = row.filter {
                push(
                    filter,
                    Owner::Filter(Filtered::Template(template)),
                    Scope::Row,
                );
            }
            for (entry, (_, derivation)) in row.overrides.iter().enumerate() {
                push(
                    derivation.expr,
                    Owner::Override { template, entry },
                    Scope::Row,
                );
            }
        }
        for (index, item) in self.intermediates.iter().enumerate() {
            let donor = Scope::Donor(index);
            if let Some(JoinKey::Pairs { pairs, .. }) = &item.key {
                for &(_, value) in pairs {
                    push(
                        value,
                        Owner::Match {
                            intermediate: index,
                        },
                        Scope::Row,
                    );
                }
            }
            if let Some(between) = &item.between {
                let owner = Owner::Range {
                    intermediate: index,
                };
                push(between.value, owner, Scope::Row);
                for bound in [between.lower, between.upper].into_iter().flatten() {
                    push(bound, owner, donor);
                }
            }
            if let Some(filter) = item.filter {
                push(filter, Owner::Filter(Filtered::Intermediate(index)), donor);
            }
            for term in &item.order_by {
                push(
                    term.variable,
                    Owner::Order(Ordered::Intermediate(index)),
                    donor,
                );
            }
            for (binding, declared) in item.bindings.iter().enumerate() {
                let owner = Owner::Binding {
                    intermediate: index,
                    binding,
                };
                push(declared.derivation.expr, owner, donor);
            }
        }
        for (check, declared) in self.checks.iter().enumerate() {
            let roots = match &declared.kind {
                DatasetCheckKind::Assert { when, require } => vec![*when, Some(*require)],
                DatasetCheckKind::RowCount(count) => vec![count.filter, count.when],
                _ => Vec::new(),
            };
            for root in roots.into_iter().flatten() {
                push(root, Owner::Check { check }, Scope::Row);
            }
        }
        for term in &self.output.order_by {
            push(term.variable, Owner::Order(Ordered::Output), Scope::Row);
        }
        sites
    }

    /// Visit `root` and its descendants in pre-order with the scope each is
    /// read in, starting from `scope` (a [`Site`]'s own scope).
    pub fn walk(&self, root: ExprId, scope: Scope, mut visit: impl FnMut(ExprId, Scope) -> Step) {
        let mut stack = vec![Frame {
            expr: root,
            scope,
            relation: None,
            bindings: None,
        }];
        while let Some(frame) = stack.pop() {
            match visit(frame.expr, frame.scope) {
                Step::Stop => return,
                Step::Skip => continue,
                Step::Continue => {}
            }
            let children = self.children(frame.expr).collect::<Vec<_>>();
            stack.extend(
                children
                    .into_iter()
                    .rev()
                    .map(|(role, child)| self.enter(frame, role, child)),
            );
        }
    }

    /// The scope a child is read in.
    fn enter(&self, parent: Frame, role: Role, child: ExprId) -> Frame {
        let node = &self.expr(parent.expr).kind;
        let aggregate = matches!(node, ExprKind::Aggregate(_));
        let owns_relation = matches!(
            node,
            ExprKind::Aggregate(_) | ExprKind::Source(_) | ExprKind::Odm(_)
        );
        let relation = if aggregate {
            Some(parent.expr)
        } else {
            parent.relation
        };
        let bindings = match (aggregate, role) {
            (true, Role::Binding) => Some(parent.expr),
            (true, _) => None,
            (false, _) => parent.bindings,
        };
        let mut scope = match role {
            Role::Relation | Role::RangeBound | Role::Binding if owns_relation => {
                Scope::Relation(parent.expr)
            }
            Role::Group | Role::Order
                if matches!(node, ExprKind::Aggregate(_) | ExprKind::Source(_)) =>
            {
                Scope::Relation(parent.expr)
            }
            Role::Reducer => relation.map_or(parent.scope, Scope::Relation),
            _ => parent.scope,
        };
        if let ExprKind::Ref(name) = &self.expr(child).kind {
            let local = if role == Role::Reducer {
                relation
            } else {
                bindings
            };
            if let Some(aggregate) = local.filter(|&aggregate| self.binds(aggregate, name)) {
                scope = Scope::Local(aggregate);
            }
        }
        Frame {
            expr: child,
            scope,
            relation,
            bindings,
        }
    }

    /// Whether `aggregate` declares a binding that the unqualified `name` reads.
    fn binds(&self, aggregate: ExprId, name: &Name) -> bool {
        name.owner().is_none()
            && matches!(&self.expr(aggregate).kind, ExprKind::Aggregate(aggregate)
                if aggregate.bindings.iter().any(|binding| binding.name.name == name.as_str()))
    }

    /// Every name `root` reads, in pre-order, with its scope.
    pub fn reads(&self, root: ExprId, scope: Scope) -> Vec<Read> {
        let mut reads = Vec::new();
        self.walk(root, scope, |at, scope| {
            if let ExprKind::Ref(name) = &self.expr(at).kind {
                reads.push(Read {
                    name: name.clone(),
                    at,
                    scope,
                });
            }
            Step::Continue
        });
        reads
    }

    /// Every project function call `root` evaluates, in pre-order.
    pub fn calls(&self, root: ExprId) -> Vec<ExprId> {
        let mut calls = Vec::new();
        self.walk(root, Scope::Row, |at, _| {
            if matches!(self.expr(at).kind, ExprKind::Call { .. }) {
                calls.push(at);
            }
            Step::Continue
        });
        calls
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::front_end::v0_5::tests::{admit, column, derivation, document, op};
    use crate::test_document::Tree;

    fn lowered(columns: Vec<Tree>, extra: Vec<(&str, Tree)>) -> Spec {
        crate::front_end::v0_5::lower(&admit(document(columns, extra)), None)
            .spec()
            .clone()
    }
    fn names(spec: &Spec, root: ExprId, scope: Scope) -> Vec<(String, Scope)> {
        spec.reads(root, scope)
            .into_iter()
            .map(|read| (read.name.0, read.scope))
            .collect()
    }

    #[test]
    fn children_follow_written_order_with_their_roles() {
        let case = op(
            "case",
            Tree::list(vec![
                Tree::map(vec![
                    ("when", Tree::text("AGE >= 65")),
                    ("then", op("literal", Tree::text("Y"))),
                ]),
                Tree::map(vec![("otherwise", op("source", source("SRC.FLAG")))]),
            ]),
        );
        let spec = lowered(vec![column("FLAG", "str", derivation(case))], vec![]);
        let root = spec.columns[1].derivation.as_ref().unwrap().expr;
        let roles = spec
            .children(root)
            .map(|(role, _)| role)
            .collect::<Vec<_>>();
        assert_eq!(roles, [Role::Condition, Role::Branch, Role::Branch]);
        let condition = spec.children(root).next().unwrap().1;
        assert!(matches!(spec.expr(condition).kind, ExprKind::Formula(_)));
        assert_eq!(
            names(&spec, root, Scope::Row),
            [
                ("AGE".to_string(), Scope::Row),
                ("SRC.FLAG".to_string(), Scope::Row)
            ]
        );
    }

    #[test]
    fn aggregate_children_read_the_relation_and_its_bindings() {
        let aggregate = op(
            "aggregate",
            Tree::map(vec![
                ("filter", Tree::text("EX.EXDOSE > 0")),
                (
                    "derive",
                    Tree::list(vec![Tree::map(vec![
                        ("name", Tree::text("DOSE")),
                        ("type", Tree::text("float")),
                        ("derivation", derivation(op("source", source("EX.EXDOSE")))),
                    ])]),
                ),
                ("expr", Tree::text("SUM(DOSE) + COUNT(EX.*) + BASE")),
            ]),
        );
        let spec = lowered(
            vec![column("TOTAL", "float", derivation(aggregate))],
            vec![],
        );
        let root = spec.columns[1].derivation.as_ref().unwrap().expr;
        let ExprKind::Aggregate(node) = &spec.expr(root).kind else {
            panic!("an aggregate");
        };
        assert_eq!(node.key, Some(crate::ast::JoinKey::Applicable));
        let relation = Scope::Relation(root);
        assert_eq!(
            names(&spec, root, Scope::Row),
            [
                ("EX.EXDOSE".to_string(), relation),
                ("EX.EXDOSE".to_string(), relation),
                ("DOSE".to_string(), Scope::Local(root)),
                ("BASE".to_string(), Scope::Row),
                ("EX.*".to_string(), relation),
            ]
        );
    }

    #[test]
    fn calls_are_found_in_branches_and_bindings_in_written_order() {
        let call = |name: &str| {
            op(
                "function",
                Tree::map(vec![
                    ("name", Tree::text(name)),
                    ("args", Tree::map(vec![("x", Tree::text("AGE"))])),
                ]),
            )
        };
        let case = op(
            "case",
            Tree::list(vec![
                Tree::map(vec![
                    ("when", Tree::text("AGE > 1")),
                    ("then", call("first")),
                ]),
                Tree::map(vec![("otherwise", call("second"))]),
            ]),
        );
        let spec = lowered(vec![column("OUT", "str", derivation(case))], vec![]);
        let root = spec.columns[1].derivation.as_ref().unwrap().expr;
        let calls = spec
            .calls(root)
            .into_iter()
            .map(|call| match &spec.expr(call).kind {
                ExprKind::Call { function, args } => {
                    assert_eq!(args.len(), 1);
                    function.name.clone()
                }
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        assert_eq!(calls, ["first", "second"]);
    }

    #[test]
    fn walk_skips_and_stops() {
        let case = op(
            "case",
            Tree::list(vec![
                Tree::map(vec![
                    ("when", Tree::text("A > 1")),
                    ("then", op("source", source("B"))),
                ]),
                Tree::map(vec![("otherwise", op("source", source("C")))]),
            ]),
        );
        let spec = lowered(vec![column("OUT", "str", derivation(case))], vec![]);
        let root = spec.columns[1].derivation.as_ref().unwrap().expr;
        let mut seen = 0;
        spec.walk(root, Scope::Row, |at, _| {
            seen += 1;
            if matches!(spec.expr(at).kind, ExprKind::Formula(_)) {
                Step::Skip
            } else {
                Step::Continue
            }
        });
        // The case, its condition (whose name is skipped) and two results.
        assert_eq!(seen, 4);
        let mut first = None;
        spec.walk(root, Scope::Row, |at, _| {
            first = Some(at);
            Step::Stop
        });
        assert_eq!(first, Some(root));
    }

    #[test]
    fn sites_partition_the_arena_and_carry_their_scopes() {
        let intermediate = Tree::map(vec![
            ("id", Tree::text("LAST")),
            ("dataset", Tree::text("SRC")),
            ("key", Tree::list(vec![Tree::text("ID")])),
            ("filter", Tree::text("SRC.FLAG = 'Y'")),
            (
                "order_by",
                Tree::list(vec![Tree::map(vec![("variable", Tree::text("SRC.DAY"))])]),
            ),
            ("keep", Tree::text("last")),
            (
                "derivations",
                Tree::map(vec![(
                    "DOUBLE",
                    Tree::map(vec![(
                        "value",
                        op(
                            "compute",
                            Tree::map(vec![("expr", Tree::text("SRC.DAY * 2"))]),
                        ),
                    )]),
                )]),
            ),
        ]);
        let check = op(
            "assert",
            Tree::map(vec![
                ("when", Tree::text("ID > 0")),
                ("require", Tree::text("FLAG = 'Y'")),
            ]),
        );
        let spec = lowered(
            vec![column(
                "FLAG",
                "str",
                derivation(op("source", source("LAST.DOUBLE"))),
            )],
            vec![
                ("intermediates", Tree::list(vec![intermediate])),
                ("verifications", Tree::list(vec![check])),
            ],
        );
        let sites = spec.sites();
        let owners = sites.iter().map(|site| site.owner).collect::<Vec<_>>();
        assert_eq!(
            owners,
            [
                Owner::Default { column: 0 },
                Owner::Default { column: 1 },
                Owner::Filter(Filtered::Intermediate(0)),
                Owner::Order(Ordered::Intermediate(0)),
                Owner::Binding {
                    intermediate: 0,
                    binding: 0
                },
                Owner::Check { check: 0 },
                Owner::Check { check: 0 },
            ]
        );
        assert_eq!(sites[2].scope, Scope::Donor(0));
        assert_eq!(sites[4].scope, Scope::Donor(0));
        let mut reached = vec![0; spec.exprs.len()];
        for site in &sites {
            spec.walk(site.root, site.scope, |at, _| {
                reached[at.index()] += 1;
                Step::Continue
            });
        }
        assert!(reached.iter().all(|&count| count == 1), "{reached:?}");
        assert_eq!(
            names(&spec, sites[4].root, sites[4].scope),
            [("SRC.DAY".to_string(), Scope::Donor(0))]
        );
    }

    fn source(variable: &str) -> Tree {
        Tree::map(vec![("variable", Tree::text(variable))])
    }
}
