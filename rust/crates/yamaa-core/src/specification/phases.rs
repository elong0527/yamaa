//! Per-template phase sets (#2754): which columns each row template
//! completes in the row phase, before its filter runs. The column phase
//! completes every other column for that template's rows, over the
//! constructed dataset, so a column is row-only, column-only or mixed by
//! construction and needs no third state.
//!
//! A template's row phase starts from
//!
//! - its own derivations;
//! - the defaults its filter reads, and, when it does not group, the
//!   columns each lookup its filter reads matches on;
//! - the output keys, when the template reads another input or a lookup
//!   before its filter (key needs);
//! - in every template, the donor fields of the SELF lookups, the defaults
//!   that read ODM items, and the defaults that read the driver of a
//!   grouped template;
//!
//! and is closed over what its row-local defaults read. A default is row
//! local unless it holds an aggregate or a window, as the whole expression
//! or as a `case` result, reads a SELF lookup or reads a column that is not
//! row local (SPEC-6); a template cannot replace a default that is not row
//! local.
//!
//! What a derivation reads comes from [`Spec::reads`]: the columns it reads
//! in the row's scope, and the columns each lookup it reads matches the row
//! on.

use super::{PreflightFinding, PrepareError};
use crate::ast::{ExprId, ExprKind, JoinKey, Read, Scope, Spec, Step, Template};
use std::collections::{BTreeMap, BTreeSet};

/// Why a default cannot complete in the row phase (SPEC-6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NotRowLocal {
    Aggregate,
    Window,
    SelfIntermediate,
}

impl NotRowLocal {
    fn name(self) -> &'static str {
        match self {
            Self::Aggregate => "aggregate",
            Self::Window => "window",
            Self::SelfIntermediate => "self_intermediate",
        }
    }
}

/// The columns each row template completes in the row phase, in
/// declaration order. `driver` drives a template that names none. A
/// template's own derivation of a column whose default is not row local is
/// refused, with the reason the default is not.
pub(super) fn row_phases(spec: &Spec, driver: &str) -> Result<Vec<Vec<usize>>, PrepareError> {
    let phases = Phases { spec, driver };
    let count = spec.columns.len();
    let defaults = (0..count)
        .map(|column| {
            phases
                .default(column)
                .map(|root| (phases.not_row_local(root), phases.needs(root)))
        })
        .collect::<Vec<_>>();
    // A default that reads a column that is not row local is not row local
    // either, for the reason that column is not.
    let mut reason = defaults
        .iter()
        .map(|default| default.as_ref().and_then(|(reason, _)| *reason))
        .collect::<Vec<_>>();
    loop {
        let mut changed = false;
        for (column, default) in defaults.iter().enumerate() {
            let Some((_, needs)) = default else { continue };
            if reason[column].is_none() {
                if let Some(&read) = needs.iter().find(|&&read| reason[read].is_some()) {
                    reason[column] = reason[read];
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let needs = |column: usize| defaults[column].iter().flat_map(|(_, needs)| needs);
    let shared = phases.shared();
    let keys = phases.key_needs();
    let mut refused: BTreeMap<usize, (NotRowLocal, Vec<String>)> = BTreeMap::new();
    let mut result = Vec::with_capacity(spec.rows.templates.len());
    for (index, template) in spec.rows.templates.iter().enumerate() {
        let mut selected = vec![false; count];
        let mut pending = Vec::new();
        let mut seed = |column: usize, pending: &mut Vec<usize>| {
            if reason[column].is_none() && !selected[column] {
                selected[column] = true;
                pending.push(column);
            }
        };
        for &column in shared
            .iter()
            .chain(&phases.filter_needs(template))
            .chain(&keys[index])
        {
            seed(column, &mut pending);
        }
        for (name, derivation) in &template.overrides {
            let column = phases.column(&name.name).ok_or(PrepareError::Internal)?;
            match reason[column] {
                Some(reason) => refused
                    .entry(column)
                    .or_insert_with(|| (reason, Vec::new()))
                    .1
                    .push(template.id.name.clone()),
                None => seed(column, &mut pending),
            }
            for read in phases.needs(derivation.expr) {
                seed(read, &mut pending);
            }
        }
        while let Some(column) = pending.pop() {
            for &read in needs(column) {
                seed(read, &mut pending);
            }
        }
        result.push((0..count).filter(|&column| selected[column]).collect());
    }
    if !refused.is_empty() {
        return Err(PrepareError::Invalid(
            refused
                .into_iter()
                .map(
                    |(column, (reason, rows))| PreflightFinding::DuplicateRowDefault {
                        column: spec.columns[column].name.name.clone(),
                        rows,
                        not_row_local: reason.name().into(),
                    },
                )
                .collect(),
        ));
    }
    Ok(result)
}

struct Phases<'a> {
    spec: &'a Spec,
    driver: &'a str,
}

impl<'a> Phases<'a> {
    fn column(&self, name: &str) -> Option<usize> {
        self.spec
            .columns
            .iter()
            .position(|column| column.name.name == name)
    }

    /// An unqualified name of an output column.
    fn output(&self, name: &str) -> Option<usize> {
        (!name.contains('.')).then(|| self.column(name)).flatten()
    }

    fn default(&self, column: usize) -> Option<ExprId> {
        self.spec.columns[column]
            .derivation
            .as_ref()
            .map(|derivation| derivation.expr)
    }

    fn lookup(&self, owner: &str) -> Option<usize> {
        self.spec
            .intermediates
            .iter()
            .position(|item| item.id.name == owner)
    }

    fn is_self(&self, owner: &str) -> bool {
        self.lookup(owner)
            .is_some_and(|index| self.spec.intermediates[index].dataset.name == "SELF")
    }

    fn is_input(&self, owner: &str) -> bool {
        self.spec
            .inputs
            .iter()
            .any(|input| input.name.name == owner)
    }

    fn driver(&self, template: &'a Template) -> &'a str {
        template
            .driver
            .as_ref()
            .map_or(self.driver, |driver| driver.name.as_str())
    }

    /// Every name `root` reads, in every scope but that of an aggregate's
    /// own bindings.
    fn all_reads(&self, root: ExprId, scope: Scope) -> Vec<Read> {
        self.spec
            .reads(root, scope)
            .into_iter()
            .filter(|read| !matches!(read.scope, Scope::Local(_)))
            .collect()
    }

    /// The names `root` reads for the current row: each name read in the
    /// row's scope, except an ODM item, which names its dataset
    /// (NAME-7), and each key column an aggregate pairs with the row's
    /// same-named value.
    fn row_reads(&self, root: ExprId) -> Vec<&'a str> {
        let spec = self.spec;
        let mut names = Vec::new();
        let mut items = BTreeSet::new();
        spec.walk(root, Scope::Row, |at, scope| {
            match &spec.expr(at).kind {
                ExprKind::Ref(name) if scope == Scope::Row && !items.contains(&at) => {
                    names.push(name.as_str());
                }
                ExprKind::Odm(read) => {
                    items.insert(read.item);
                }
                ExprKind::Aggregate(aggregate) if scope == Scope::Row => {
                    if let Some(JoinKey::Columns { columns, .. }) = &aggregate.key {
                        names.extend(columns.iter().map(|column| column.name.as_str()));
                    }
                }
                _ => {}
            }
            Step::Continue
        });
        names
    }

    /// The columns the lookup at `index` matches the current row on
    /// (LOOKUP-25): the current-row values its key pairs with donor columns,
    /// the output keys when it declares no key, and its range value,
    /// followed into the lookup that value reads. A lookup already followed
    /// is not followed again; binding reports such a cycle.
    fn matches(&self, index: usize, followed: &mut Vec<usize>, out: &mut Vec<usize>) {
        if followed.contains(&index) {
            return;
        }
        followed.push(index);
        let item = &self.spec.intermediates[index];
        match &item.key {
            Some(JoinKey::Columns { columns, .. }) => {
                out.extend(
                    columns
                        .iter()
                        .filter_map(|column| self.output(&column.name)),
                );
            }
            Some(JoinKey::Pairs { pairs, .. }) => {
                for &(_, value) in pairs {
                    out.extend(
                        self.row_reads(value)
                            .into_iter()
                            .filter_map(|name| self.output(name)),
                    );
                }
            }
            Some(JoinKey::FromInput) => {}
            None => {
                out.extend(
                    self.spec
                        .keys
                        .iter()
                        .filter_map(|key| self.output(&key.name)),
                );
            }
        }
        if let Some(between) = &item.between {
            if let ExprKind::Ref(name) = &self.spec.expr(between.value).kind {
                match name.owner() {
                    Some(owner) => {
                        if let Some(next) = self.lookup(owner) {
                            self.matches(next, followed, out);
                        }
                    }
                    None => out.extend(self.output(name.as_str())),
                }
            }
        }
    }

    /// The columns `root` needs before it completes in the row phase: the
    /// columns each lookup it reads matches on, then the columns it reads.
    /// A structured source's range value is one of those reads wherever the
    /// source is written, so a row-local default it names completes in the
    /// row phase (SPEC-6), and a default it names that is not row local
    /// leaves the reading default not row local either.
    fn needs(&self, root: ExprId) -> Vec<usize> {
        let mut needs = Vec::new();
        for read in self.all_reads(root, Scope::Row) {
            if let Some(index) = read.name.owner().and_then(|owner| self.lookup(owner)) {
                self.matches(index, &mut Vec::new(), &mut needs);
            }
        }
        needs.extend(
            self.row_reads(root)
                .into_iter()
                .filter_map(|name| self.output(name)),
        );
        needs
    }

    /// Why the default `root` is not row local, before what it reads: it
    /// holds an aggregate, which reduces its relation, or a window, which
    /// spans the complete constructed dataset, or it reads a SELF lookup.
    fn not_row_local(&self, root: ExprId) -> Option<NotRowLocal> {
        if self.holds(root, |kind| matches!(kind, ExprKind::Aggregate(_))) {
            return Some(NotRowLocal::Aggregate);
        }
        if self.holds(root, |kind| matches!(kind, ExprKind::Window(_))) {
            return Some(NotRowLocal::Window);
        }
        self.all_reads(root, Scope::Row)
            .iter()
            .any(|read| read.name.owner().is_some_and(|owner| self.is_self(owner)))
            .then_some(NotRowLocal::SelfIntermediate)
    }

    /// Whether `e`, or a case result below it, is an operation that
    /// `operation` accepts. Outside an aggregate's own key values, a case
    /// result is the only place where one operation nests in another
    /// (EXPR-3), so this finds the operation anywhere in a derivation.
    fn holds(&self, e: ExprId, operation: fn(&ExprKind) -> bool) -> bool {
        let kind = &self.spec.expr(e).kind;
        operation(kind)
            || matches!(kind, ExprKind::Case(arms)
                if arms.iter().any(|arm| self.holds(arm.result, operation)))
    }

    /// The columns every template's row phase starts from: the donor fields
    /// of the SELF lookups, the defaults that read ODM items, which own each
    /// template's item scope and row identity, and the defaults that read
    /// the driver of a grouped template.
    fn shared(&self) -> Vec<usize> {
        let mut shared = self.donor_fields();
        for column in 0..self.spec.columns.len() {
            if self
                .default(column)
                .is_some_and(|root| super::checks::odm::reads_odm(self.spec, root))
            {
                shared.insert(column);
            }
        }
        shared.extend(self.grouped_driver_reads());
        shared.into_iter().collect()
    }

    /// The output columns the SELF lookups read from completed rows: their
    /// keys, order, filter, derivations, range bounds, selected columns and
    /// unique checks, and the fields any derivation reads through one.
    fn donor_fields(&self) -> BTreeSet<usize> {
        let spec = self.spec;
        let mut fields = BTreeSet::new();
        for (index, item) in spec.intermediates.iter().enumerate() {
            if item.dataset.name != "SELF" {
                continue;
            }
            let mut register = |name: &str| fields.extend(self.column(name));
            match &item.key {
                Some(JoinKey::Columns { columns, .. }) => {
                    columns.iter().for_each(|column| register(&column.name));
                }
                Some(JoinKey::Pairs { pairs, .. }) => {
                    pairs.iter().for_each(|(column, _)| register(&column.name));
                }
                _ => {}
            }
            // Its order, filter and derivations read donor fields, bare or
            // qualified by SELF.
            let roots = item
                .order_by
                .iter()
                .map(|term| term.variable)
                .chain(item.filter)
                .chain(item.bindings.iter().map(|binding| binding.derivation.expr));
            for root in roots {
                for read in self.all_reads(root, Scope::Donor(index)) {
                    match read.name.as_str().split_once('.') {
                        None => register(read.name.as_str()),
                        Some(("SELF", field)) => register(field),
                        Some(_) => {}
                    }
                }
            }
            if let Some(between) = &item.between {
                for bound in [between.lower, between.upper].into_iter().flatten() {
                    if let ExprKind::Ref(name) = &spec.expr(bound).kind {
                        register(name.as_str());
                    }
                }
            }
            item.columns
                .iter()
                .flatten()
                .chain(item.checks.iter().flat_map(|check| &check.columns))
                .for_each(|column| register(&column.name));
        }
        let derivations =
            spec.columns
                .iter()
                .filter_map(|column| column.derivation.as_ref())
                .chain(spec.rows.templates.iter().flat_map(|template| {
                    template.overrides.iter().map(|(_, derivation)| derivation)
                }));
        for derivation in derivations {
            for read in self.all_reads(derivation.expr, Scope::Row) {
                if let Some((owner, field)) = read.name.as_str().split_once('.') {
                    if self.is_self(owner) {
                        fields.extend(self.column(field));
                    }
                }
            }
        }
        fields
    }

    /// The defaults that read the driver of a grouped template and no
    /// lookup, which every template completes in the row phase: their source
    /// reads cannot complete in the column phase under a grouped context they
    /// cannot admit. A default that reads a lookup stays with the templates
    /// that need it (LOOKUP-12), and one that reads only other inputs stays in
    /// the column phase, where it binds against each row's own driver.
    /// Windows, aggregates and the verbs that stay there (a mapping) stay in
    /// the column phase.
    fn grouped_driver_reads(&self) -> Vec<usize> {
        let grouped = self
            .spec
            .rows
            .templates
            .iter()
            .filter(|template| template.group_by.is_some())
            .map(|template| self.driver(template))
            .collect::<BTreeSet<_>>();
        if grouped.is_empty() {
            return Vec::new();
        }
        (0..self.spec.columns.len())
            .filter(|&column| {
                let Some(root) = self.default(column) else {
                    return false;
                };
                let column_phase = match &self.spec.expr(root).kind {
                    ExprKind::Window(_) | ExprKind::Aggregate(_) => true,
                    ExprKind::Scalar { op, .. } => op.stays_in_column_phase(),
                    _ => false,
                };
                if column_phase {
                    return false;
                }
                let owners = self
                    .row_reads(root)
                    .into_iter()
                    .filter_map(|name| name.split_once('.').map(|(owner, _)| owner))
                    .collect::<Vec<_>>();
                !owners.iter().any(|owner| self.lookup(owner).is_some())
                    && owners.iter().any(|owner| grouped.contains(owner))
            })
            .collect()
    }

    /// The columns a template's filter needs before it runs: the defaults it
    /// reads and, when the template does not group, the columns each lookup
    /// it reads matches on.
    fn filter_needs(&self, template: &Template) -> Vec<usize> {
        let Some(filter) = template.filter else {
            return Vec::new();
        };
        let mut needs = Vec::new();
        for name in self.row_reads(filter) {
            if template.group_by.is_none() {
                if let Some(index) = name
                    .split_once('.')
                    .and_then(|(owner, _)| self.lookup(owner))
                {
                    self.matches(index, &mut Vec::new(), &mut needs);
                }
            }
            if let Some(column) = self.output(name) {
                if self.default(column).is_some() {
                    needs.push(column);
                }
            }
        }
        needs
    }

    /// Each template's key needs. A template whose filter or own derivations
    /// read another input or a lookup matches on the output keys before its
    /// filter runs, so it completes their defaults in the row phase; a read
    /// qualified by its own driver is a read of its record. A key whose
    /// default reads a lookup joins only those templates, so that a
    /// candidate another template's filter suppresses never reads that lookup
    /// (LOOKUP-12); any other key joins every template once one needs keys,
    /// so that no column-phase source read runs under a grouped context it
    /// cannot admit. A window key spans the constructed dataset.
    fn key_needs(&self) -> Vec<Vec<usize>> {
        let templates = &self.spec.rows.templates;
        let needing = templates
            .iter()
            .map(|template| {
                let driver = self.driver(template);
                template
                    .filter
                    .into_iter()
                    .chain(
                        template
                            .overrides
                            .iter()
                            .map(|(_, derivation)| derivation.expr),
                    )
                    .flat_map(|root| self.row_reads(root))
                    .any(|name| {
                        name.split_once('.').is_some_and(|(owner, _)| {
                            owner != driver
                                && (self.lookup(owner).is_some() || self.is_input(owner))
                        })
                    })
            })
            .collect::<Vec<_>>();
        let mut keys = vec![Vec::new(); templates.len()];
        if !needing.contains(&true) {
            return keys;
        }
        for key in &self.spec.keys {
            let Some(column) = self.column(&key.name) else {
                continue;
            };
            let Some(root) = self.default(column) else {
                continue;
            };
            if matches!(self.spec.expr(root).kind, ExprKind::Window(_)) {
                continue;
            }
            let reads_lookup = self.row_reads(root).into_iter().any(|name| {
                name.split_once('.')
                    .is_some_and(|(owner, _)| self.lookup(owner).is_some())
            });
            for (template, needs) in keys.iter_mut().enumerate() {
                if needing[template] || !reads_lookup {
                    needs.push(column);
                }
            }
        }
        keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_document::Tree;
    use crate::test_specification::{column, derivation, document, lower, op};

    /// Columns after `ID` (read from `SRC.ID`), with row templates and any
    /// other fields.
    fn phases(
        columns: Vec<Tree>,
        extra: Vec<(&str, Tree)>,
    ) -> Result<Vec<Vec<usize>>, PrepareError> {
        row_phases(&lower(document(columns, extra)), "SRC")
    }
    /// A derivation of the text `value`, quoted as an expression's literal.
    fn literal(value: &str) -> Tree {
        derivation(constant(value))
    }
    fn compute(expr: &str) -> Tree {
        derivation(Tree::text(expr))
    }
    fn source(variable: &str) -> Tree {
        derivation(Tree::text(variable))
    }
    fn declared(name: &str) -> Tree {
        Tree::map(vec![
            ("name", Tree::text(name)),
            ("type", Tree::text("str")),
        ])
    }
    fn row(id: &str, derivations: Vec<(&str, Tree)>, fields: Vec<(&str, Tree)>) -> Tree {
        let mut row = vec![
            ("id", Tree::text(id)),
            ("derivations", Tree::map(derivations)),
        ];
        row.extend(fields);
        Tree::map(row)
    }
    fn inputs(names: &[&str]) -> Tree {
        Tree::map(
            names
                .iter()
                .map(|&name| (name, Tree::map(vec![("path", Tree::text("in.csv"))])))
                .collect(),
        )
    }

    #[test]
    fn a_row_phase_closes_over_the_defaults_it_reads() {
        // ID, A, B = A + 1, C = B * 2, D, E. The first template derives C
        // itself, and the second filters on D; neither needs E.
        let rows = Tree::list(vec![
            row(
                "first",
                vec![("ID", source("SRC.ID")), ("C", literal("c"))],
                vec![],
            ),
            row(
                "second",
                vec![("ID", source("SRC.ID"))],
                vec![("filter", Tree::text("D = 'd'"))],
            ),
        ]);
        let sets = phases(
            vec![
                column("A", "str", literal("a")),
                column("B", "str", compute("A + 1")),
                column("C", "str", compute("B * 2")),
                column("D", "str", literal("d")),
                column("E", "str", literal("e")),
            ],
            vec![("rows", rows)],
        )
        .unwrap();
        // The closure follows the default of every column in the row phase,
        // the first template's own C included.
        assert_eq!(sets, [vec![0, 1, 2, 3], vec![0, 4]]);
    }

    /// A `row_number` over the whole constructed dataset, ordered by `ID`.
    fn window() -> Tree {
        op(
            "row_number",
            Tree::map(vec![(
                "window",
                Tree::map(vec![("order_by", Tree::list(vec![Tree::text("ID")]))]),
            )]),
        )
    }
    /// An aggregate over SRC, matched on the output keys.
    fn sum() -> Tree {
        op("aggregate", Tree::text("SUM(SRC.ID)"))
    }
    /// Quoted text, as an expression field writes a literal (D18).
    fn constant(value: &str) -> Tree {
        Tree::text(format!("'{value}'"))
    }
    /// A `case` that gives `result` when `ID > 0`, and `fallback` otherwise.
    fn positive(result: Tree, fallback: Tree) -> Tree {
        op(
            "case",
            Tree::list(vec![
                Tree::map(vec![("when", Tree::text("ID > 0")), ("then", result)]),
                Tree::map(vec![("otherwise", fallback)]),
            ]),
        )
    }
    /// Each refused default: its column, the templates that name it and why
    /// it is not row local.
    fn refused(findings: &[PreflightFinding]) -> Vec<(&str, Vec<&str>, &str)> {
        findings
            .iter()
            .map(|finding| match finding {
                PreflightFinding::DuplicateRowDefault {
                    column,
                    rows,
                    not_row_local,
                } => (
                    column.as_str(),
                    rows.iter().map(String::as_str).collect(),
                    not_row_local.as_str(),
                ),
                _ => panic!("{finding:?}"),
            })
            .collect()
    }

    #[test]
    fn a_template_cannot_replace_a_default_that_is_not_row_local() {
        let prior = Tree::map(vec![
            ("id", Tree::text("PRIOR")),
            ("dataset", Tree::text("SELF")),
            ("key", Tree::list(vec![Tree::text("ID")])),
        ]);
        let rows = Tree::list(vec![row(
            "only",
            vec![
                ("ID", source("SRC.ID")),
                ("TOTAL", literal("1")),
                ("NEXT", literal("2")),
                ("EARLIER", literal("3")),
            ],
            vec![],
        )]);
        let Err(PrepareError::Invalid(findings)) = phases(
            vec![
                column("TOTAL", "int", derivation(sum())),
                column("NUMBER", "int", derivation(window())),
                // Not row local because it reads a window.
                column("NEXT", "int", compute("NUMBER + 1")),
                column("EARLIER", "int", source("PRIOR.ID")),
            ],
            vec![("intermediates", Tree::list(vec![prior])), ("rows", rows)],
        ) else {
            panic!("refused");
        };
        assert_eq!(
            refused(&findings),
            [
                ("TOTAL", vec!["only"], "aggregate"),
                ("NEXT", vec!["only"], "window"),
                ("EARLIER", vec!["only"], "self_intermediate"),
            ]
        );
    }

    #[test]
    fn an_aggregate_in_a_case_result_is_not_row_local() {
        let rows = Tree::list(vec![
            row(
                "first",
                vec![
                    ("ID", source("SRC.ID")),
                    ("TOTAL", literal("1")),
                    ("NESTED", literal("2")),
                ],
                vec![],
            ),
            row(
                "second",
                vec![
                    ("ID", source("SRC.ID")),
                    ("TOTAL", literal("1")),
                    ("NEXT", literal("3")),
                    ("BOTH", literal("4")),
                ],
                vec![],
            ),
        ]);
        let Err(PrepareError::Invalid(findings)) = phases(
            vec![
                column("TOTAL", "int", derivation(positive(sum(), constant("0")))),
                column("NESTED", "int", derivation(otherwise(otherwise(sum())))),
                // Not row local because it reads TOTAL.
                column("NEXT", "int", compute("TOTAL + 1")),
                column("BOTH", "int", derivation(positive(window(), sum()))),
            ],
            vec![("rows", rows)],
        ) else {
            panic!("refused");
        };
        // An aggregate makes its default not row local wherever a `case`
        // result holds it, as a window does (SPEC-6), and the aggregate is
        // named first when a default holds both.
        assert_eq!(
            refused(&findings),
            [
                ("TOTAL", vec!["first", "second"], "aggregate"),
                ("NESTED", vec!["first"], "aggregate"),
                ("NEXT", vec!["second"], "aggregate"),
                ("BOTH", vec!["second"], "aggregate"),
            ]
        );
    }

    #[test]
    fn a_row_phase_leaves_an_aggregate_in_a_case_result_to_the_column_phase() {
        // `reads` reads TOTAL and PLAIN in its own derivation of SHARE.
        let rows = Tree::list(vec![row(
            "reads",
            vec![
                ("ID", source("SRC.ID")),
                ("SHARE", compute("PLAIN + TOTAL")),
            ],
            vec![],
        )]);
        let sets = phases(
            vec![
                column("TOTAL", "int", derivation(positive(sum(), constant("0")))),
                column(
                    "PLAIN",
                    "int",
                    derivation(positive(constant("1"), constant("0"))),
                ),
                declared("SHARE"),
            ],
            vec![("rows", rows)],
        )
        .unwrap();
        // PLAIN completes in the row phase, while TOTAL is left to the column
        // phase: the template's read of it is a phase boundary (COLUMN-4).
        assert_eq!(sets, [vec![0, 2, 3]]);
    }

    #[test]
    fn a_key_that_reads_a_lookup_joins_only_the_templates_that_need_keys() {
        // `reads` reads the lookup LAST before its filter, so it needs the
        // output keys; `plain` reads only its own driver. SEQ reads LAST,
        // so only `reads` completes it in the row phase; K joins both.
        let lookup = Tree::map(vec![
            ("id", Tree::text("LAST")),
            ("dataset", Tree::text("SRC")),
            ("key", Tree::list(vec![Tree::text("ID")])),
        ]);
        let rows = Tree::list(vec![
            row(
                "reads",
                vec![("ID", source("DM.ID")), ("V", source("LAST.V"))],
                vec![("dataset", Tree::text("DM"))],
            ),
            row(
                "plain",
                vec![("ID", source("SRC.ID")), ("V", literal("v"))],
                vec![],
            ),
        ]);
        let sets = phases(
            vec![
                column("SEQ", "int", source("LAST.SEQ")),
                column("K", "str", literal("k")),
                declared("V"),
            ],
            vec![
                ("input", inputs(&["SRC", "DM"])),
                (
                    "keys",
                    Tree::list(vec![Tree::text("ID"), Tree::text("SEQ"), Tree::text("K")]),
                ),
                ("intermediates", Tree::list(vec![lookup])),
                ("rows", rows),
            ],
        )
        .unwrap();
        assert_eq!(sets, [vec![0, 1, 2, 3], vec![0, 2, 3]]);
    }

    #[test]
    fn odm_defaults_and_self_donor_fields_join_every_template() {
        let mapping = derivation(op(
            "mapping",
            Tree::map(vec![
                ("source", Tree::text("SRC.X")),
                (
                    "dict",
                    Tree::map(vec![("odm", Tree::text("SRC.IT.TEST.OID"))]),
                ),
            ]),
        ));
        let prior = Tree::map(vec![
            ("id", Tree::text("PRIOR")),
            ("dataset", Tree::text("SELF")),
            ("key", Tree::list(vec![Tree::text("ID")])),
            (
                "order_by",
                Tree::list(vec![Tree::map(vec![("variable", Tree::text("ORDER"))])]),
            ),
        ]);
        let rows = Tree::list(vec![
            row("first", vec![("ID", source("SRC.ID"))], vec![]),
            row("second", vec![("ID", source("SRC.ID"))], vec![]),
        ]);
        let sets = phases(
            vec![
                column("ITEM", "str", derivation(op("odm", Tree::text("SRC.IT")))),
                // Dictionary data and literal text read no ODM item.
                column("MAPPED", "str", mapping),
                column("TEXT", "str", literal("SRC.IT.TEST.OID")),
                column("ORDER", "int", literal("1")),
            ],
            vec![("intermediates", Tree::list(vec![prior])), ("rows", rows)],
        )
        .unwrap();
        assert_eq!(sets, [vec![0, 1, 4], vec![0, 1, 4]]);
    }

    /// A structured source of `PERIODS.PERIOD` whose range value is `value`.
    fn structured(value: &str) -> Tree {
        op(
            "source",
            Tree::map(vec![
                ("variable", Tree::text("PERIODS.PERIOD")),
                (
                    "between",
                    Tree::map(vec![
                        ("value", Tree::text(value)),
                        ("lower", Tree::text("FIRST")),
                        ("upper", Tree::text("LAST")),
                    ]),
                ),
            ]),
        )
    }
    /// A `case` whose one branch is `result`.
    fn otherwise(result: Tree) -> Tree {
        op(
            "case",
            Tree::list(vec![Tree::map(vec![("otherwise", result)])]),
        )
    }

    #[test]
    fn a_structured_source_s_range_value_brings_its_default_into_the_row_phase() {
        let rows = Tree::list(vec![
            row(
                "own",
                vec![
                    ("ID", source("SRC.ID")),
                    ("PERIOD", derivation(structured("DAY"))),
                ],
                vec![],
            ),
            row(
                "nested",
                vec![
                    ("ID", source("SRC.ID")),
                    ("PERIOD", derivation(otherwise(structured("DAY")))),
                ],
                vec![],
            ),
        ]);
        let sets = phases(
            vec![column("DAY", "int", source("SRC.DAY")), declared("PERIOD")],
            vec![("input", inputs(&["SRC", "PERIODS"])), ("rows", rows)],
        )
        .unwrap();
        // The range value is read as any other read (SPEC-6), whether the
        // structured source is the derivation itself or a `case` branch: DAY
        // completes in the row phase of both templates.
        assert_eq!(sets, [vec![0, 1, 2], vec![0, 1, 2]]);
    }

    #[test]
    fn a_structured_source_whose_range_value_reads_a_window_is_not_row_local() {
        let rows = Tree::list(vec![row(
            "only",
            vec![
                ("ID", source("SRC.ID")),
                ("OWN", literal("1")),
                ("NESTED", literal("2")),
            ],
            vec![],
        )]);
        let Err(PrepareError::Invalid(findings)) = phases(
            vec![
                column("NUMBER", "int", derivation(window())),
                column("OWN", "int", derivation(structured("NUMBER"))),
                column("NESTED", "int", derivation(otherwise(structured("NUMBER")))),
            ],
            vec![("input", inputs(&["SRC", "PERIODS"])), ("rows", rows)],
        ) else {
            panic!("refused");
        };
        // Each default reads the window through its range value, so neither
        // is row local, and the template replaces neither (SPEC-6).
        assert_eq!(
            refused(&findings),
            [
                ("OWN", vec!["only"], "window"),
                ("NESTED", vec!["only"], "window"),
            ]
        );
    }
}
