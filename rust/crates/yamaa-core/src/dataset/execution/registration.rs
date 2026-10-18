//! Every handler path a run counts, registered at zero before any row is read
//! (COLUMN-14), so a path that answers nothing still reports its zero.
//!
//! Registration order is report order. First every `unconvertible`: those of
//! the plan's derivations in declared order, then those of each
//! intermediate's typed derivations. Then each derivation in evaluation
//! order: the row templates' derivations, template by template, then the
//! column phase's, then the intermediates' key expressions. Last, each
//! intermediate's derivations, intermediate by intermediate, in declaration
//! order, followed by the key expressions of the intermediates they read, as
//! bound for the donor records. Each derivation lists its paths in expression order: an operation's
//! own paths before those of the expressions it reads, which follow in
//! written order. A read of a named intermediate registers its
//! `multiple_matches` and `no_match` at the reading path. The resolved plan's
//! handler table is checked against this walk on every bind.
use super::*;
use crate::bound_expression::Read;
use crate::dataset::{read_handlers, ReadHandlers};

/// Register every handler path of `plan` at zero, in report order.
pub(super) fn register(plan: &DatasetPlan, counter: &mut HandlerCounter) {
    for declaration in plan.conversion_handlers() {
        counter.register(&declaration.handler.spec_path, HandlerKind::Unconvertible);
    }
    for intermediate in plan.intermediates() {
        if let Some(derived) = &intermediate.derivations {
            for declaration in derived.plan.conversion_handlers() {
                counter.register(&declaration.handler.spec_path, HandlerKind::Unconvertible);
            }
        }
    }
    // Admission refuses a read of an intermediate declared later; such a
    // read registers nothing here.
    let reads = read_handlers(plan.intermediates()).unwrap_or_default();
    let mut walk = Walk {
        plan,
        reads: &reads,
        counter,
    };
    for assignment in plan
        .templates()
        .iter()
        .flat_map(|template| &template.assignments)
        .chain(plan.columns())
    {
        walk.assignment(assignment);
    }
    walk.keys();
    // Each intermediate's derivations, in declaration order, at their own
    // paths, then the key expressions of the intermediates they read, as
    // bound for the donor records. A nested donor plan is an earlier
    // intermediate's, walked in its turn.
    for item in plan.intermediates() {
        if let Some(derived) = &item.derivations {
            let donor = &derived.plan;
            let reads = read_handlers(donor.intermediates()).unwrap_or_default();
            let mut donor_walk = Walk {
                plan: donor,
                reads: &reads,
                counter: &mut *walk.counter,
            };
            for assignment in donor.columns() {
                donor_walk.assignment(assignment);
            }
            donor_walk.keys();
        }
    }
}

struct Walk<'a> {
    plan: &'a DatasetPlan,
    reads: &'a [ReadHandlers],
    counter: &'a mut HandlerCounter,
}

impl Walk<'_> {
    fn site(&mut self, path: &str, kind: HandlerKind) {
        self.counter.register(path, kind);
    }

    /// The key expressions of the plan's intermediates, in declaration order.
    fn keys(&mut self) {
        let plan = self.plan;
        for item in plan.intermediates() {
            for key in &item.keys {
                if let MatchValue::Expression(expression) = &key.value {
                    self.assignment(&expression.assignment);
                }
            }
        }
    }

    /// The handlers a read of a named intermediate counts at its reading path,
    /// those of the intermediates it reads through included.
    fn named(&self, index: usize) -> ReadHandlers {
        self.reads.get(index).copied().unwrap_or_default()
    }

    /// The handlers a range value read from a named intermediate counts at the
    /// range's reading path.
    fn range(&self, range: Option<&RelationRange>) -> ReadHandlers {
        match range.map(|range| range.current().read) {
            Some(CurrentRead::Intermediate { index, .. }) => self.named(index),
            _ => ReadHandlers::default(),
        }
    }

    /// A lookup registers `multiple_matches` before its absence handler.
    fn lookup(&mut self, path: &str, handlers: ReadHandlers) {
        if handlers.multiple_matches {
            self.site(
                &std::format!("{path}.multiple_matches"),
                HandlerKind::MultipleMatches,
            );
        }
        if handlers.no_match {
            self.site(&std::format!("{path}.no_match"), HandlerKind::NoMatch);
        }
    }

    /// A read through an implicit join counts its answer under `absent` at
    /// its path, unless it fails instead (LOOKUP-24).
    fn absent(&mut self, absence: &JoinAbsence, input: &str) {
        if !absence.requires_match(self.plan.requires_match(input)) {
            self.site(
                &std::format!("{}.absent", absence.path),
                HandlerKind::Absent,
            );
        }
    }

    /// Operand reads of a formula or a function, each counted at the owner's
    /// path: the named reads' `multiple_matches` and `no_match`, then the
    /// implicit joins' `absent`.
    fn operands<'r>(&mut self, path: &str, reads: impl Iterator<Item = &'r Read> + Clone) {
        let mut handlers = ReadHandlers::default();
        for read in reads.clone() {
            if let Read::Intermediate { index, .. } = read {
                let read = self.named(*index);
                handlers.multiple_matches |= read.multiple_matches;
                handlers.no_match |= read.no_match;
            }
        }
        self.lookup(path, handlers);
        for read in reads {
            if let Read::Joined(read) = read {
                self.absent(&JoinAbsence::operand(path), &read.name);
            }
        }
    }

    fn assignment(&mut self, assignment: &Assignment) {
        let path = assignment.path.as_str();
        match &assignment.expression {
            Expression::Collect { selection, .. } => {
                if selection.is_some() {
                    self.site(
                        &std::format!("{path}.multiple_matches"),
                        HandlerKind::MultipleMatches,
                    );
                }
            }
            Expression::Compute(numeric) => {
                self.operands(path, numeric.bindings().iter().map(|binding| &binding.read))
            }
            Expression::Function(function) => self.operands(path, arguments(function.arguments())),
            Expression::ProjectFunction(function) => {
                self.operands(path, arguments(function.arguments()))
            }
            Expression::Flag(flag) => self.site(&flag.missing_path(), HandlerKind::Missing),
            Expression::Case(branches) => {
                for branch in branches {
                    self.assignment(&branch.assignment);
                }
            }
            Expression::MappingCut { source, operation } => {
                let base = operation.path();
                self.site(&std::format!("{base}.missing"), HandlerKind::Missing);
                if operation.unmapped().is_some() {
                    self.site(&std::format!("{base}.unmapped"), HandlerKind::Unmapped);
                }
                self.assignment(source);
            }
            Expression::Imputation {
                operands,
                operation,
            } => {
                let base = operation.path();
                if operation.counts_missing_source() {
                    // A missing source is counted at `fallback` when one is declared.
                    let field = if operation.fallback().is_some() {
                        "fallback"
                    } else {
                        "missing"
                    };
                    self.site(&std::format!("{base}.{field}"), HandlerKind::Missing);
                }
                if operation.handler("invalid").is_some() {
                    self.site(&std::format!("{base}.invalid"), HandlerKind::Invalid);
                }
                if operation.counts_crossed_bounds() {
                    self.site(
                        &std::format!("{base}.not_after"),
                        HandlerKind::CrossedBounds,
                    );
                }
                if matches!(operation.kind(), crate::dataset::ImputationKind::ToDate) {
                    self.site(&std::format!("{base}.partial"), HandlerKind::Partial);
                }
                for operand in operands {
                    self.assignment(operand);
                }
            }
            Expression::Temporal {
                operands,
                operation,
            } => {
                if operation.invalid_handler().is_some() {
                    self.site(
                        &std::format!("{}.invalid", operation.path()),
                        HandlerKind::Invalid,
                    );
                }
                for operand in operands {
                    self.assignment(operand);
                }
            }
            Expression::StrTemplate {
                operands,
                operation,
            } => {
                if operation.missing().is_some() {
                    self.site(
                        &std::format!("{}.missing", operation.path()),
                        HandlerKind::Missing,
                    );
                }
                for operand in operands {
                    self.assignment(operand);
                }
            }
            Expression::Extreme { operands, .. } => {
                for operand in operands {
                    self.assignment(operand);
                }
            }
            Expression::StrExtract { source, operation } => {
                let base = operation.path();
                self.site(&std::format!("{base}.missing"), HandlerKind::Missing);
                if operation.handler("no_match").is_some() {
                    self.site(&std::format!("{base}.no_match"), HandlerKind::NoMatch);
                }
                self.assignment(source);
            }
            Expression::StrReplace { source, operation } => {
                let path = std::format!("{}.missing", operation.path());
                self.site(&path, HandlerKind::Missing);
                self.assignment(source);
            }
            Expression::StrCase { source, operation } => {
                let path = std::format!("{}.missing", operation.path());
                self.site(&path, HandlerKind::Missing);
                self.assignment(source);
            }
            Expression::StrPad { source, .. } | Expression::Round { source, .. } => {
                self.assignment(source)
            }
            Expression::SourceBinding {
                source,
                absent,
                selection_declared,
                path: binding,
                ..
            } => {
                if *selection_declared {
                    self.site(
                        &std::format!("{}.multiple_matches", source.path),
                        HandlerKind::MultipleMatches,
                    );
                }
                if absent.is_some() {
                    self.site(&std::format!("{binding}.absent"), HandlerKind::Absent);
                }
                self.assignment(source);
            }
            Expression::FirstAvailable(selection) => {
                self.site(&selection.missing_path(), HandlerKind::Missing);
                for source in selection.sources() {
                    if let crate::dataset::SelectionRead::Expression(source) = &source.read {
                        self.assignment(source);
                    }
                }
            }
            Expression::SourceLookup {
                lookup,
                range,
                selection,
                absence,
                ..
            } => {
                let mut handlers = self.range(range.as_ref());
                handlers.multiple_matches |= selection.is_some();
                self.lookup(path, handlers);
                if let Some(absence) = absence {
                    let plan = self.plan;
                    self.absent(absence, &plan.secondary()[lookup.source].name);
                }
            }
            Expression::JoinedSource {
                read,
                range,
                selection,
                absence,
                ..
            } => {
                let mut handlers = self.range(range.as_ref());
                handlers.multiple_matches |= selection.is_some();
                self.lookup(path, handlers);
                if let Some(absence) = absence {
                    self.absent(absence, &read.name);
                }
            }
            Expression::Joined(read) => self.absent(&JoinAbsence::operand(path), &read.name),
            Expression::Intermediate { index, .. } => {
                let handlers = self.named(*index);
                self.lookup(path, handlers);
            }
            Expression::Aggregate(aggregate) => {
                if let Some(range) = aggregate.range() {
                    let handlers = self.range(Some(range));
                    self.lookup(range.path(), handlers);
                }
                if let crate::dataset::AggregateRelation::Matched { keys, .. } =
                    aggregate.relation()
                {
                    for key in keys {
                        if let MatchValue::Expression(expression) = &key.value {
                            self.assignment(&expression.assignment);
                        }
                    }
                }
                for derivation in aggregate.derivations() {
                    if let Some(handler) = &derivation.handler {
                        self.site(&handler.spec_path, HandlerKind::Unconvertible);
                    }
                    self.assignment(&derivation.assignment);
                }
            }
            Expression::Odm(_)
            | Expression::Literal(_)
            | Expression::Window(_)
            | Expression::ContextWindow(_)
            | Expression::Source(_)
            | Expression::Lookup(_)
            | Expression::RowLookup(_)
            | Expression::Column(_)
            | Expression::Reduce { .. }
            | Expression::Count { .. } => (),
        }
    }
}

/// The bound reads of a function's arguments, in argument order.
fn arguments(
    arguments: &[crate::dataset::FunctionArgument],
) -> impl Iterator<Item = &Read> + Clone {
    arguments
        .iter()
        .filter_map(|argument| match &argument.input {
            crate::dataset::FunctionInput::Read(read) => Some(read),
            _ => None,
        })
}
