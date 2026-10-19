//! Debug builds check the lowered tree against this compiler (#3133): a
//! spec the compiler admits lowers its derivations without an unsupported
//! node, and each declaration it compiles renders, in the tree, the path
//! it cites.
use super::*;
use crate::ast::{ExprId, ExprKind, Literal, Meta, Notation, Owner, Step};
use crate::front_end::v0_5::Lowered;

impl PreparedSpecification {
    /// Loud in debug builds: a disagreement is a lowering defect, while
    /// binding still reads the document.
    pub(super) fn audit_lowering(self, lowered: &Lowered) -> Self {
        if cfg!(debug_assertions) {
            let mismatches = self.lowering_mismatches(lowered);
            assert!(
                mismatches.is_empty(),
                "the lowered tree disagrees with the compiler:\n{}",
                mismatches.join("\n")
            );
        }
        self
    }

    fn lowering_mismatches(&self, lowered: &Lowered) -> Vec<String> {
        let spec = lowered.spec();
        let mut audit = Audit {
            lowered,
            mismatches: Vec::new(),
        };
        // Verifications keep a malformed declaration as a finding reported
        // at its checkpoint, so only the other sites must lower completely.
        let mut unsupported = Vec::new();
        for site in spec.sites() {
            if matches!(site.owner, Owner::Check { .. }) {
                continue;
            }
            spec.walk(site.root, site.scope, |at, _| {
                if matches!(
                    spec.expr(at).kind,
                    ExprKind::Unsupported(_) | ExprKind::Literal(Literal::Unsupported)
                ) {
                    unsupported.push(spec.expr(at).meta);
                }
                Step::Continue
            });
        }
        for meta in unsupported {
            let path = audit.render(meta);
            audit
                .mismatches
                .push(format!("{path}: lowered as unsupported"));
        }
        for (column, declaration) in self.declarations.iter().enumerate() {
            match spec.columns.get(column).and_then(|c| c.derivation.as_ref()) {
                Some(derivation) => {
                    audit.operation(&declaration.path, &declaration.operation, derivation.expr)
                }
                None => audit.missing(&declaration.path),
            }
        }
        for (value, path, branch) in self.rows.iter().flat_map(|rows| rows.audit_entries()) {
            match lowered.expression(value) {
                Some(expr) => {
                    audit.path(path, expr);
                    if let Some(branch) = branch {
                        audit.operation(&branch.path, &branch.operation, expr);
                    }
                }
                None => audit.missing(path),
            }
        }
        for (name, path, derivations) in self.intermediates.audit_entries() {
            let Some(item) = spec.intermediates.iter().find(|item| item.id.name == name) else {
                audit.missing(path);
                continue;
            };
            audit.meta(path, item.meta);
            for derivation in derivations {
                match item
                    .bindings
                    .iter()
                    .find(|binding| binding.name.name == derivation.name)
                {
                    Some(binding) => {
                        audit.meta(derivation.path(), binding.meta);
                        let branch = &derivation.branch;
                        audit.operation(&branch.path, &branch.operation, binding.derivation.expr);
                    }
                    None => audit.missing(derivation.path()),
                }
            }
        }
        audit.mismatches
    }
}

struct Audit<'a> {
    lowered: &'a Lowered,
    mismatches: Vec<String>,
}

impl Audit<'_> {
    fn render(&self, meta: Meta) -> String {
        self.lowered.spec().render(meta.path, Notation::V0_5)
    }
    fn missing(&mut self, cited: &str) {
        self.mismatches.push(format!("{cited}: not lowered"));
    }
    fn meta(&mut self, cited: &str, meta: Meta) {
        let rendered = self.render(meta);
        if rendered != cited {
            self.mismatches
                .push(format!("{cited}: lowered at {rendered}"));
        }
    }
    fn path(&mut self, cited: &str, expr: ExprId) {
        self.meta(cited, self.lowered.spec().expr(expr).meta);
    }
    /// The operation's own path, then each branch and derive binding below it.
    fn operation(&mut self, cited: &str, operation: &Operation, expr: ExprId) {
        self.path(cited, expr);
        let spec = self.lowered.spec();
        match (operation, &spec.expr(expr).kind) {
            (Operation::Case(branches), ExprKind::Case(arms)) if branches.len() == arms.len() => {
                for (branch, arm) in branches.iter().zip(arms) {
                    self.operation(&branch.path, &branch.operation, arm.result);
                }
            }
            (Operation::Aggregate(declaration), ExprKind::Aggregate(aggregate)) => {
                let derived = declaration.derive_entries().collect::<Vec<_>>();
                if derived.len() != aggregate.bindings.len() {
                    self.missing(cited);
                }
                for ((path, branch), binding) in derived.into_iter().zip(&aggregate.bindings) {
                    self.meta(path, binding.meta);
                    self.operation(&branch.path, &branch.operation, binding.derivation.expr);
                }
            }
            (Operation::Case(_) | Operation::Aggregate(_), _) => self.missing(cited),
            _ => {}
        }
    }
}
