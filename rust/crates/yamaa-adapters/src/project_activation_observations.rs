//! Complete activation evidence from one owned build. This reads held metadata
//! and actual engine observations; it never queries a host or invokes a case.
use crate::{
    project_run::PreparedRun,
    project_source::{CapturedEnvironment, Origin},
    report_scalar::ScalarValue,
};
use yamaa_core::json::{json, Value as Json};
use yamaa_core::project_activation::{BindingOutcome, LockObservation, Observations, TestOutcome};
use yamaa_core::{
    project_environment::ExecutionEnvironment, project_function::Function, value::Value,
};

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::PreparedRun {}
    impl Sealed for crate::file_producer_build::Provenance {}
    impl<T: super::ActivationView + ?Sized> Sealed for std::sync::Arc<T> {}
}

/// One retained activation context: ordinary selected calls or the complete
/// consumed graph union. External callers cannot mix unrelated provenance.
pub trait ActivationView: sealed::Sealed {
    fn called_functions(&self) -> &[usize];
    fn environment(&self) -> &ExecutionEnvironment;
    fn captured_environment(&self) -> &CapturedEnvironment;
}
impl<T: ActivationView + ?Sized> ActivationView for std::sync::Arc<T> {
    fn called_functions(&self) -> &[usize] {
        self.as_ref().called_functions()
    }
    fn environment(&self) -> &ExecutionEnvironment {
        self.as_ref().environment()
    }
    fn captured_environment(&self) -> &CapturedEnvironment {
        self.as_ref().captured_environment()
    }
}
impl ActivationView for PreparedRun {
    fn called_functions(&self) -> &[usize] {
        self.compiled().called_functions()
    }
    fn environment(&self) -> &ExecutionEnvironment {
        self.environment()
    }
    fn captured_environment(&self) -> &CapturedEnvironment {
        self.captured_environment()
    }
}
impl ActivationView for crate::file_producer_build::Provenance {
    fn called_functions(&self) -> &[usize] {
        self.metadata().called_functions()
    }
    fn environment(&self) -> &ExecutionEnvironment {
        self.metadata().environment()
    }
    fn captured_environment(&self) -> &CapturedEnvironment {
        self.environment()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidObservation,
    Projection,
}
fn scalar(value: &Value) -> Result<Json, Error> {
    yamaa_core::json::to_value(ScalarValue::from_core(value.clone())).map_err(|_| Error::Projection)
}
fn function(run: &dyn ActivationView, selected: usize) -> Result<(&Function, &str), Error> {
    let index = *run
        .called_functions()
        .get(selected)
        .ok_or(Error::InvalidObservation)?;
    let function = run
        .environment()
        .functions()
        .get(index)
        .ok_or(Error::InvalidObservation)?;
    let name = &function.definition().name;
    if run.captured_environment().origins().functions.get(index) != Some(name) {
        return Err(Error::InvalidObservation);
    }
    let source = run
        .captured_environment()
        .captures()
        .iter()
        .find(|capture| matches!(&capture.origin, Origin::Function(held) if held == name))
        .map(|capture| capture.document.source().identity.as_str())
        .or_else(|| {
            run.captured_environment()
                .root_optional()
                .map(|root| root.source().identity.as_str())
        })
        .ok_or(Error::InvalidObservation)?;
    Ok((function, source))
}

pub fn activation(run: &dyn ActivationView, observed: &Observations) -> Result<Json, Error> {
    let selected = run.called_functions().len();
    if selected == 0
        && (observed.lock != LockObservation::NotRequested
            || !observed.bindings.is_empty()
            || !observed.tests.is_empty())
    {
        return Err(Error::InvalidObservation);
    }
    if !observed.bindings.is_empty() && observed.lock != LockObservation::Verified {
        return Err(Error::InvalidObservation);
    }
    for (index, binding) in observed.bindings.iter().enumerate() {
        if binding.function != index
            || (index + 1 < observed.bindings.len()
                && matches!(
                    binding.outcome,
                    BindingOutcome::Attempted | BindingOutcome::Interrupted
                ))
        {
            return Err(Error::InvalidObservation);
        }
        function(run, binding.function)?;
    }
    if !observed.tests.is_empty()
        && (observed.bindings.len() != selected
            || observed
                .bindings
                .iter()
                .any(|binding| binding.outcome != BindingOutcome::Bound))
    {
        return Err(Error::InvalidObservation);
    }
    let mut next = (0, 0);
    let total_tests = observed.tests.len();
    for (index, observed) in observed.tests.iter().enumerate() {
        if (observed.function, observed.case) != next
            || (index + 1 < total_tests
                && matches!(
                    observed.outcome,
                    TestOutcome::Attempted | TestOutcome::Interrupted
                ))
        {
            return Err(Error::InvalidObservation);
        }
        let (function, _) = function(run, observed.function)?;
        let definition = function.definition();
        definition
            .tests
            .get(observed.case)
            .ok_or(Error::InvalidObservation)?;

        next = if observed.case + 1 == definition.tests.len() {
            (observed.function + 1, 0)
        } else {
            (observed.function, observed.case + 1)
        };
    }
    let bindings = observed.bindings.iter().map(|binding| {
        let (function, source) = function(run, binding.function)?;
        Ok(json!({"function":function.definition().name, "call":function.definition().function, "source":source, "outcome":match binding.outcome {
            BindingOutcome::Attempted => "attempted", BindingOutcome::Bound => "bound", BindingOutcome::Rejected => "rejected", BindingOutcome::Interrupted => "interrupted",
        }}))
    }).collect::<Result<Vec<_>, Error>>()?;
    let tests = observed.tests.iter().map(|observed| {
        let (function, source) = function(run, observed.function)?;
        let definition = function.definition();
        let case = &definition.tests[observed.case];
        let args = case.args.iter().map(|(name, value)| Ok((name.clone(), scalar(value)?))).collect::<Result<yamaa_core::json::Map<_, _>, Error>>()?;
        Ok(json!({"function":definition.name, "call":definition.function, "source":source, "case":case.id, "args":args,
            "expected":scalar(&case.result)?, "comparison_decimals":definition.comparison_decimals,
            "outcome":match observed.outcome { TestOutcome::Attempted => "attempted", TestOutcome::Passed => "passed", TestOutcome::ResultMismatch => "result_mismatch", TestOutcome::InvocationFailed => "invocation_failed", TestOutcome::Interrupted => "interrupted" },
            "invoked":observed.invoked, "actual_retained":observed.actual.is_some(), "actual":observed.actual.as_ref().map(scalar).transpose()?}))
    }).collect::<Result<Vec<_>, Error>>()?;
    let result = json!({"lock":match observed.lock {
        LockObservation::NotRequested => "not_requested", LockObservation::Attempted => "attempted", LockObservation::Verified => "verified", LockObservation::Rejected => "rejected", LockObservation::Interrupted => "interrupted",
    }, "bindings":bindings, "tests":tests});
    Ok(result)
}
