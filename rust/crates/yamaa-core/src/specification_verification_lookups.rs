//! Keep selected-record assertion names held until source schema binding.
use super::*;
use crate::predicate::Plan;

#[derive(Debug)]
pub(super) enum PreparedPredicate {
    Ready(BoundPredicate),
    Lookup(Plan),
}
impl PreparedPredicate {
    pub(super) fn has_lookup(&self) -> bool {
        matches!(self, Self::Lookup(_))
    }
}

#[derive(Debug)]
pub(super) struct PendingAssert {
    pub position: usize,
    pub path: String,
    pub identity: Option<String>,
    pub when: Option<Result<PreparedPredicate, Diagnostic>>,
    pub require: Result<PreparedPredicate, Diagnostic>,
}
impl PendingAssert {
    pub(super) fn finding(&self) -> Option<&Diagnostic> {
        self.when
            .as_ref()
            .and_then(|when| when.as_ref().err())
            .or_else(|| self.require.as_ref().err())
    }
    pub(super) fn bind(
        &self,
        spec: &PreparedSpecification,
        secondary: &[crate::dataset::SecondarySource],
        intermediates: &[Option<crate::dataset::Intermediate>],
    ) -> Result<Vec<Verification>, BindError> {
        let bind = |predicate: &Result<PreparedPredicate, Diagnostic>| match predicate {
            Err(diagnostic) => Ok(Err(diagnostic.clone())),
            Ok(PreparedPredicate::Ready(predicate)) => Ok(Ok(predicate.clone())),
            Ok(PreparedPredicate::Lookup(plan)) => bind(plan, spec, secondary, intermediates),
        };
        let mut checks = Vec::new();
        let when = match self.when.as_ref().map(bind).transpose()? {
            None => None,
            Some(Ok(when)) => Some(when),
            Some(Err(diagnostic)) => {
                checks.push(invalid(diagnostic));
                return Ok(checks);
            }
        };
        match bind(&self.require)? {
            Ok(require) => checks.push(Verification {
                path: self.path.clone(),
                check: Check::Assert { when, require },
            }),
            Err(diagnostic) => {
                // A valid when still has its sample/type checkpoint before a
                // later require finding; it never selects actual donor rows here.
                if let Some(when) = when {
                    checks.push(Verification {
                        path: when.plan().spec_path().into(),
                        check: Check::PredicateDeclaration(when),
                    });
                }
                checks.push(invalid(diagnostic));
            }
        }
        Ok(checks)
    }
}
fn invalid(diagnostic: Diagnostic) -> Verification {
    Verification {
        path: diagnostic.spec_paths[0].clone(),
        check: Check::InvalidDiagnostic(diagnostic),
    }
}
fn bind(
    plan: &Plan,
    spec: &PreparedSpecification,
    secondary: &[crate::dataset::SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
) -> Result<Result<BoundPredicate, Diagnostic>, BindError> {
    let mut bindings = Vec::new();
    let mut unknown = None;
    for name in plan
        .identifiers()
        .into_iter()
        .collect::<alloc::collections::BTreeSet<_>>()
    {
        let read = if let Some(column) = spec
            .output
            .columns()
            .iter()
            .position(|column| column.name == name)
        {
            Some(Read::Column(column))
        } else if let Some((index, field)) = spec.intermediates.reference(name) {
            intermediates[index]
                .as_ref()
                .and_then(|item| item.source_column(secondary, field))
                .map(|column| Read::Intermediate { index, column })
        } else {
            None
        };
        match read {
            Some(read) => bindings.push(Binding {
                name: name.into(),
                read,
            }),
            None if unknown.is_none() => unknown = Some(name),
            None => {}
        }
    }
    if let Some(name) = unknown {
        return Ok(Err(Diagnostic {
            code: ConditionCode::VerificationUnknownReference,
            spec_paths: vec![plan.spec_path().into()],
            context: [(
                "identifier".into(),
                ContextValue::Scalar(Value::Str(name.into())),
            )]
            .into(),
            source_span: None,
            operand_route: None,
        }));
    }
    BoundPredicate::new_contextual(plan.clone(), bindings, Vec::new())
        .map(Ok)
        .map_err(BindError::InvalidPredicateBinding)
}

impl Verifications {
    pub(in super::super) fn bind_assertions(
        &self,
        spec: &PreparedSpecification,
        secondary: &[crate::dataset::SecondarySource],
        intermediates: &[Option<crate::dataset::Intermediate>],
    ) -> Result<Vec<Verification>, BindError> {
        let mut checks = Vec::new();
        for position in 0..=self.checks.len() {
            for pending in self
                .pending_assertions
                .iter()
                .filter(|pending| pending.position == position)
            {
                checks.extend(pending.bind(spec, secondary, intermediates)?);
            }
            if let Some(check) = self.checks.get(position) {
                checks.push(check.clone());
            }
        }
        Ok(checks)
    }
    pub(in super::super) fn pending_findings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.pending_assertions
            .iter()
            .filter_map(PendingAssert::finding)
    }
    pub(in super::super) fn pending_identity(&self, path: &str) -> Option<&str> {
        self.pending_assertions
            .iter()
            .find(|pending| pending.path == path)
            .and_then(|pending| pending.identity.as_deref())
    }
}
