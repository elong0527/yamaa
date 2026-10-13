//! Check and build against one immutable admitted environment.
//! Each build activates called definitions before granting study-data authority.
use crate::{
    domain::{self, CheckedSpecification},
    project_activation::{self, ActivationPort, Bindings, Failure},
    specification_run::{CapturedAttempt, Limits, PortError, SourceDecoder, SourcePort},
};
use alloc::{boxed::Box, vec::Vec};
use yamaa_core::{
    project_environment::ExecutionEnvironment,
    schema::SpecificationDocument,
    specification::{PrepareError, PreparedSpecification},
    table::TableAccess,
};

pub type BuildEvidence<'a, P, D> = (
    &'a mut CapturedAttempt<
        <P as SourcePort>::Error,
        <D as SourceDecoder>::Error,
        <D as SourceDecoder>::Table,
    >,
    &'a mut project_activation::Observations,
);

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
    pub fn check_diagnostics(&self) -> Vec<yamaa_core::diagnostic::Diagnostic> {
        self.checked.verification_declaration_diagnostics()
    }
    /// Repeat lock, binding and all complete cases on each build. Source capture
    /// uses already activated handles and preserves the original host payloads.
    pub fn build_into<A: ActivationPort, P: SourcePort, D: SourceDecoder>(
        &self,
        activation: &mut A,
        source: &mut P,
        decoder: &mut D,
        limits: Limits,
        attempt: &mut CapturedAttempt<P::Error, D::Error, D::Table>,
    ) -> Result<(), Failure<A::Error>>
    where
        D::Table: TableAccess<Error = A::Error>,
    {
        build(
            (&self.checked, self.environment),
            activation,
            source,
            decoder,
            limits,
            attempt,
            (
                None,
                crate::application_observer::Scope::new(&crate::application_observer::Noop),
            ),
        )
    }

    /// Keep activation evidence outside the host boundary and dataset attempt.
    /// Successful cases remain observable after a later activation/build failure.
    pub fn build_observed_into<A: ActivationPort, P: SourcePort, D: SourceDecoder>(
        &self,
        activation: &mut A,
        source: &mut P,
        decoder: &mut D,
        limits: Limits,
        attempt: &mut CapturedAttempt<P::Error, D::Error, D::Table>,
        observations: &mut project_activation::Observations,
    ) -> Result<(), Failure<A::Error>>
    where
        D::Table: TableAccess<Error = A::Error>,
    {
        build(
            (&self.checked, self.environment),
            activation,
            source,
            decoder,
            limits,
            attempt,
            (
                Some(observations),
                crate::application_observer::Scope::new(&crate::application_observer::Noop),
            ),
        )
    }

    /// Retain activation evidence and forward trusted application events without
    /// adding measurement fields to the semantic attempt.
    pub fn build_with_observer_into<A: ActivationPort, P: SourcePort, D: SourceDecoder>(
        &self,
        activation: &mut A,
        source: &mut P,
        decoder: &mut D,
        limits: Limits,
        evidence: BuildEvidence<'_, P, D>,
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
            limits,
            evidence.0,
            (Some(evidence.1), scope),
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
    pub fn check_diagnostics(&self) -> Vec<yamaa_core::diagnostic::Diagnostic> {
        self.checked.verification_declaration_diagnostics()
    }
    pub fn build_into<A: ActivationPort, P: SourcePort, D: SourceDecoder>(
        &self,
        activation: &mut A,
        source: &mut P,
        decoder: &mut D,
        limits: Limits,
        attempt: &mut CapturedAttempt<P::Error, D::Error, D::Table>,
    ) -> Result<(), Failure<A::Error>>
    where
        D::Table: TableAccess<Error = A::Error>,
    {
        build(
            (&self.checked, &self.environment),
            activation,
            source,
            decoder,
            limits,
            attempt,
            (
                None,
                crate::application_observer::Scope::new(&crate::application_observer::Noop),
            ),
        )
    }

    pub fn build_observed_into<A: ActivationPort, P: SourcePort, D: SourceDecoder>(
        &self,
        activation: &mut A,
        source: &mut P,
        decoder: &mut D,
        limits: Limits,
        attempt: &mut CapturedAttempt<P::Error, D::Error, D::Table>,
        observations: &mut project_activation::Observations,
    ) -> Result<(), Failure<A::Error>>
    where
        D::Table: TableAccess<Error = A::Error>,
    {
        build(
            (&self.checked, &self.environment),
            activation,
            source,
            decoder,
            limits,
            attempt,
            (
                Some(observations),
                crate::application_observer::Scope::new(&crate::application_observer::Noop),
            ),
        )
    }

    pub fn build_with_observer_into<A: ActivationPort, P: SourcePort, D: SourceDecoder>(
        &self,
        activation: &mut A,
        source: &mut P,
        decoder: &mut D,
        limits: Limits,
        evidence: BuildEvidence<'_, P, D>,
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
            limits,
            evidence.0,
            (Some(evidence.1), scope),
        )
    }
}
fn build<A: ActivationPort, P: SourcePort, D: SourceDecoder>(
    (checked, environment): (&CheckedSpecification, &ExecutionEnvironment),
    activation: &mut A,
    source: &mut P,
    decoder: &mut D,
    limits: Limits,
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
        checked.build_with_observer_into(
            source,
            decoder,
            &mut crate::dataset::unavailable_functions(),
            limits,
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
    let lock = environment
        .lock()
        .expect("admitted called functions declare a lock");
    scope.phase(crate::application_observer::Phase::Activation);
    let activated = match observations {
        Some(observations) => project_activation::activate_references_observed(
            language,
            lock,
            &functions,
            &mut scope.activation(activation),
            project_activation::Limits::default(),
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
    checked.build_with_observer_into(source, decoder, &mut bindings, limits, attempt, scope);
    Ok(())
}
