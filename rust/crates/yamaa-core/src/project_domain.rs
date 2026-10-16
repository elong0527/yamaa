//! Check and build against one immutable admitted environment.
//! Each build activates called definitions before granting study-data authority.
use crate::{
    domain::{self, CheckedSpecification},
    project_activation::{self, ActivationPort, Bindings, Failure},
    specification_run::{CapturedAttempt, PortError, SourceDecoder, SpecificationSourcePort},
};
use crate::{
    project_environment::ExecutionEnvironment,
    schema::SpecificationDocument,
    specification::{PrepareError, PreparedSpecification},
    table::TableAccess,
};

/// The private fields tie compilation and selected indices to the same immutable
/// environment. No caller can swap a different definition collection at build.
#[derive(Debug)]
pub struct CheckedProject<'a> {
    checked: CheckedSpecification,
    environment: &'a ExecutionEnvironment,
}

/// Static checking uses admitted metadata only, without activation or study ports.
pub fn check<'a>(
    document: &SpecificationDocument,
    environment: &'a ExecutionEnvironment,
) -> Result<CheckedProject<'a>, PrepareError> {
    Ok(CheckedProject {
        checked: domain::check_with_environment(document, environment)?,
        environment,
    })
}
impl CheckedProject<'_> {
    pub fn compiled(&self) -> &PreparedSpecification {
        self.checked.compiled()
    }
    pub fn check_diagnostics(&self) -> Vec<crate::diagnostic::Diagnostic> {
        self.checked.check_diagnostics()
    }
    /// Repeat lock, binding and all complete cases on each build. Source capture
    /// uses already activated handles and preserves the original host payloads.
    pub fn build_into<A: ActivationPort, P: SpecificationSourcePort, D: SourceDecoder>(
        &self,
        activation: &mut A,
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
            (&self.checked, self.environment),
            activation,
            source,
            decoder,
            attempt,
            (observations, scope),
        )
    }
}

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
pub fn check_owned(
    document: &SpecificationDocument,
    environment: ExecutionEnvironment,
) -> Result<CheckedOwnedProject, Box<RejectedOwnedProject>> {
    match domain::check_with_environment(document, &environment) {
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
    pub fn build_into<A: ActivationPort, P: SpecificationSourcePort, D: SourceDecoder>(
        &self,
        activation: &mut A,
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
            activation,
            source,
            decoder,
            attempt,
            (observations, scope),
        )
    }
}
fn build<A: ActivationPort, P: SpecificationSourcePort, D: SourceDecoder>(
    (checked, environment): (&CheckedSpecification, &ExecutionEnvironment),
    activation: &mut A,
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
    let language = environment
        .language()
        .expect("admitted called functions declare a language");
    let lock = environment.lock();
    scope.phase(crate::application_observer::Phase::Activation);
    let activated = match observations {
        Some(observations) => project_activation::activate_references_observed(
            language,
            lock,
            &functions,
            &mut scope.activation(activation),
            observations,
        ),
        None => project_activation::activate_references(
            language,
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
