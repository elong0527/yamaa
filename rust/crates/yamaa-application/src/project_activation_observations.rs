//! Complete activation evidence from one owned build. This reads held metadata
//! and actual engine observations; it never queries a host or invokes a case.
use crate::json;
use crate::{report_scalar::ScalarValue, specification_run::PreparedRun};
use yamaa_core::json::Value as Json;
use yamaa_core::project_activation::{BindingOutcome, LockObservation, Observations, TestOutcome};
use yamaa_core::{project_function::Function, value::Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidObservation,
    Projection,
}
fn scalar(value: &Value) -> Result<Json, Error> {
    Ok(ScalarValue::from_core(value.clone()).to_json())
}
fn function(run: &PreparedRun, selected: usize) -> Result<(&Function, &str), Error> {
    let index = *run
        .compiled()
        .called_functions()
        .get(selected)
        .ok_or(Error::InvalidObservation)?;
    let function = run
        .environment()
        .functions()
        .get(index)
        .ok_or(Error::InvalidObservation)?;
    let name = function.name();
    if run
        .captured_environment()
        .origins()
        .functions
        .get(index)
        .map(String::as_str)
        != Some(name)
    {
        return Err(Error::InvalidObservation);
    }
    // The function file that holds the definition.
    let source = run
        .captured_environment()
        .function_origin(index)
        .ok_or(Error::InvalidObservation)?
        .source;
    Ok((function, source))
}

pub fn activation(run: &PreparedRun, observed: &Observations) -> Result<Json, Error> {
    let selected = run.compiled().called_functions().len();
    if selected == 0
        && (observed.lock != LockObservation::NotRequested
            || !observed.bindings.is_empty()
            || !observed.tests.is_empty())
    {
        return Err(Error::InvalidObservation);
    }
    let lock_ready = if run.environment().lock().is_some() {
        observed.lock == LockObservation::Verified
    } else {
        if observed.lock != LockObservation::NotRequested {
            return Err(Error::InvalidObservation);
        }
        true
    };
    if !observed.bindings.is_empty() && !lock_ready {
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
        Ok(json!({"function":function.name(), "call":function.call(), "source":source, "outcome":match binding.outcome {
            BindingOutcome::Attempted => "attempted", BindingOutcome::Bound => "bound", BindingOutcome::Rejected => "rejected", BindingOutcome::Interrupted => "interrupted",
        }}))
    }).collect::<Result<Vec<_>, Error>>()?;
    let tests = observed.tests.iter().map(|observed| {
        let (function, source) = function(run, observed.function)?;
        let definition = function.definition();
        let case = &definition.tests[observed.case];
        let args = case.args.iter().map(|(name, value)| Ok((name.name.clone(), scalar(value)?))).collect::<Result<yamaa_core::json::Map<_, _>, Error>>()?;
        Ok(json!({"function":function.name(), "call":function.call(), "source":source, "case":case.id.text, "args":args,
            "expected":scalar(&case.result)?, "comparison_decimals":definition.comparison_decimals,
            "outcome":match observed.outcome { TestOutcome::Attempted => "attempted", TestOutcome::Passed => "passed", TestOutcome::ResultMismatch => "result_mismatch", TestOutcome::InvocationFailed => "invocation_failed", TestOutcome::Interrupted => "interrupted" },
            "invoked":observed.invoked, "actual_retained":observed.actual.is_some(), "actual":observed.actual.as_ref().map(scalar).transpose()?}))
    }).collect::<Result<Vec<_>, Error>>()?;
    let result = json!({"lock":match observed.lock {
        LockObservation::NotRequested => "not_requested", LockObservation::Attempted => "attempted", LockObservation::Verified => "verified", LockObservation::Rejected => "rejected", LockObservation::Interrupted => "interrupted",
    }, "bindings":bindings, "tests":tests});
    Ok(result)
}
