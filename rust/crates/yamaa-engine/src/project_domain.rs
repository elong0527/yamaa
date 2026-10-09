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
        checked: domain::check_with_project(document, environment.functions())?,
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
            &self.checked,
            self.environment,
            activation,
            source,
            decoder,
            limits,
            attempt,
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
    match domain::check_with_project(document, environment.functions()) {
        Ok(checked) => Ok(CheckedOwnedProject {
            checked,
            environment,
        }),
        Err(error) => Err(Box::new(RejectedOwnedProject { environment, error })),
    }
}
impl CheckedOwnedProject {
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
            &self.checked,
            &self.environment,
            activation,
            source,
            decoder,
            limits,
            attempt,
        )
    }
}
fn build<A: ActivationPort, P: SourcePort, D: SourceDecoder>(
    checked: &CheckedSpecification,
    environment: &ExecutionEnvironment,
    activation: &mut A,
    source: &mut P,
    decoder: &mut D,
    limits: Limits,
    attempt: &mut CapturedAttempt<P::Error, D::Error, D::Table>,
) -> Result<(), Failure<A::Error>>
where
    D::Table: TableAccess<Error = A::Error>,
{
    // A later failed activation cannot retain observations from a prior build.
    attempt.sources.clear();
    attempt.result = Err(PortError::Incomplete);
    let selected = checked.compiled().called_functions();
    if selected.is_empty() {
        checked.build_into(source, decoder, limits, attempt);
        return Ok(());
    }
    let functions = selected
        .iter()
        .map(|&index| &environment.functions()[index])
        .collect::<Vec<_>>();
    let activated = project_activation::activate_references(
        environment
            .language()
            .expect("admitted called functions declare a language"),
        environment
            .lock()
            .expect("admitted called functions declare a lock"),
        &functions,
        activation,
    )?;
    let mut bindings = Bindings::new(&activated, activation);
    checked.build_with_functions_into(source, decoder, &mut bindings, limits, attempt);
    Ok(())
}
