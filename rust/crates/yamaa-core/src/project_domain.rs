//! Check and build against one immutable admitted environment.
//! Each build activates called definitions through the host-functions port
//! before granting study-data authority.
use crate::{
    decoder::Decoder,
    domain::{self, CheckedSpecification},
    host_functions::{HostFunctions, Lock},
    project_activation::{self, Bindings, Failure},
    resolver::Resolver,
    specification_run::{CapturedAttempt, PortError},
};
use crate::{
    project_environment::ExecutionEnvironment,
    specification::{PrepareError, PreparedSpecification},
    table::TableAccess,
};

/// Own the admitted metadata alongside its private checked specification. Native
/// prepared handles can move this capability without a self-referential borrow.
#[derive(Debug)]
pub struct CheckedOwnedProject {
    checked: CheckedSpecification,
    environment: ExecutionEnvironment,
}
#[derive(Debug)]
pub struct RejectedOwnedProject {
    pub environment: ExecutionEnvironment,
    pub error: PrepareError,
}
/// Own the admitted environment beside the compilation of a 1.x
/// specification its front end lowered into `tree`, beside its resolved
/// `document`, which declares its linked inputs, and the output contracts of
/// the producing specifications they link. A build without a project
/// compiles against no environment, as a specification checked alone does;
/// its empty environment only carries the host to the build.
pub fn check_owned_tree_linked(
    tree: crate::ast::Spec,
    document: &crate::schema::Document,
    environment: ExecutionEnvironment,
    links: crate::producer_contract::Links<'_>,
) -> Result<CheckedOwnedProject, Box<RejectedOwnedProject>> {
    let admitted = environment.is_project().then_some(&environment);
    match domain::check_tree_linked(tree, document, admitted, links) {
        Ok(checked) => Ok(CheckedOwnedProject {
            checked,
            environment,
        }),
        Err(error) => Err(Box::new(RejectedOwnedProject { environment, error })),
    }
}
impl CheckedOwnedProject {
    /// Static native-location admission can reject a checked document while
    /// retaining its original environment; no runtime port has been entered.
    pub fn reject(self, error: PrepareError) -> Box<RejectedOwnedProject> {
        Box::new(RejectedOwnedProject {
            environment: self.environment,
            error,
        })
    }
    pub fn compiled(&self) -> &PreparedSpecification {
        self.checked.compiled()
    }
    pub fn environment(&self) -> &ExecutionEnvironment {
        &self.environment
    }
    pub fn check_diagnostics(&self) -> Vec<crate::diagnostic::Diagnostic> {
        self.checked.check_diagnostics()
    }
    /// [`CheckedProject::build_into`] over the owned environment.
    #[allow(clippy::too_many_arguments)]
    pub fn build_into<A: HostFunctions, P: Resolver, D: Decoder>(
        &self,
        functions: &mut A,
        lock: &[u8],
        source: &mut P,
        decoder: &mut D,
        attempt: &mut CapturedAttempt<P::Error, D::Error, D::Table>,
        observations: Option<&mut project_activation::Observations>,
        scope: crate::application_observer::Scope<'_>,
    ) -> Result<(), Failure<A::Error>>
    where
        D::Table: TableAccess<Error = A::Error>,
    {
        build(
            (&self.checked, &self.environment),
            (functions, lock),
            source,
            decoder,
            attempt,
            (observations, scope),
        )
    }
}
fn build<A: HostFunctions, P: Resolver, D: Decoder>(
    (checked, environment): (&CheckedSpecification, &ExecutionEnvironment),
    (activation, lock_bytes): (&mut A, &[u8]),
    source: &mut P,
    decoder: &mut D,
    attempt: &mut CapturedAttempt<P::Error, D::Error, D::Table>,
    (observations, scope): (
        Option<&mut project_activation::Observations>,
        crate::application_observer::Scope<'_>,
    ),
) -> Result<(), Failure<A::Error>>
where
    D::Table: TableAccess<Error = A::Error>,
{
    // A later failed activation cannot retain observations from a prior build.
    attempt.sources.clear();
    attempt.result = Err(PortError::Incomplete);
    let observations = observations.map(|observations| {
        *observations = project_activation::Observations::default();
        observations
    });
    let selected = checked.compiled().called_functions();
    if selected.is_empty() {
        checked.build_into(
            source,
            decoder,
            &mut crate::function_invocation::NoFunctions::new(),
            attempt,
            scope,
        );
        return Ok(());
    }
    let functions = selected
        .iter()
        .map(|&index| &environment.functions()[index])
        .collect::<Vec<_>>();
    // The empty port and an engine that claims no host language refuse the
    // build before it enters activation (D110, D123).
    let language =
        project_activation::selected_language(environment.language(), &functions, activation)?;
    let lock = environment.lock().map(|reference| Lock {
        reference,
        bytes: lock_bytes,
    });
    scope.phase(crate::application_observer::Phase::Activation);
    let activated = match observations {
        Some(observations) => project_activation::activate_references_observed(
            Some(language),
            lock,
            &functions,
            &mut scope.activation(activation),
            observations,
        ),
        None => project_activation::activate_references(
            Some(language),
            lock,
            &functions,
            &mut scope.activation(activation),
        ),
    };
    scope.exit(
        crate::application_observer::Phase::Activation,
        crate::application_observer::Outcome::from_result(&activated),
    );
    let activated = activated?;
    let mut bindings = Bindings::new(&activated, activation);
    checked.build_into(source, decoder, &mut bindings, attempt, scope);
    Ok(())
}
