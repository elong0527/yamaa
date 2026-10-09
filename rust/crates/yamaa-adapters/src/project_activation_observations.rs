//! Bounded activation evidence from one owned build. This reads held metadata
//! and actual engine observations; it never queries a host or invokes a case.
use crate::{project_run::PreparedRun, project_source::Origin, scalar_transport::ScalarValue};
use serde_json::{json, Value as Json};
use yamaa_core::{project_function::Function, value::Value};
use yamaa_engine::project_activation::{
    BindingOutcome, LockObservation, Observations, TestOutcome,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidObservation,
    Limit,
    Projection,
}
struct Budget(usize);
impl Budget {
    fn text(&mut self, text: &str) -> Result<(), Error> {
        let charge = text
            .len()
            .checked_mul(12)
            .and_then(|n| n.checked_add(128))
            .ok_or(Error::Limit)?;
        self.0 = self.0.checked_sub(charge).ok_or(Error::Limit)?;
        Ok(())
    }
    fn value(&mut self, value: &Value) -> Result<(), Error> {
        self.text(match value {
            Value::Str(text) => text,
            _ => "01234567890123456789012345678901",
        })
    }
}
fn scalar(value: &Value) -> Result<Json, Error> {
    serde_json::to_value(ScalarValue::from_core(value.clone())).map_err(|_| Error::Projection)
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
    let name = &function.definition().name;
    if run.captured_environment().origins().functions.get(index) != Some(name) {
        return Err(Error::InvalidObservation);
    }
    let source = run
        .captured_environment()
        .captures()
        .iter()
        .find(|capture| matches!(&capture.origin, Origin::Function(held) if held == name))
        .map_or_else(
            || run.captured_environment().root().source().identity.as_str(),
            |capture| capture.document.source().identity.as_str(),
        );
    Ok((function, source))
}

pub fn activation(run: &PreparedRun, observed: &Observations) -> Result<Json, Error> {
    activation_with_limit(run, observed, 16_777_216)
}
pub fn activation_with_limit(
    run: &PreparedRun,
    observed: &Observations,
    maximum: usize,
) -> Result<Json, Error> {
    if observed.bindings.len() > 1024 || observed.tests.len() > 65_536 {
        return Err(Error::Limit);
    }
    let selected = run.compiled().called_functions().len();
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
    let mut budget = Budget(maximum);
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
        let (function, source) = function(run, binding.function)?;
        for text in [
            &function.definition().name,
            &function.definition().function,
            source,
        ] {
            budget.text(text)?;
        }
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
        let (function, source) = function(run, observed.function)?;
        let definition = function.definition();
        let case = definition
            .tests
            .get(observed.case)
            .ok_or(Error::InvalidObservation)?;
        for text in [&definition.name, &definition.function, &case.id, source] {
            budget.text(text)?;
        }
        for (name, value) in &case.args {
            budget.text(name)?;
            budget.value(value)?;
        }
        budget.value(&case.result)?;
        if let Some(actual) = &observed.actual {
            budget.value(actual)?;
        }
        next = if observed.case + 1 == definition.tests.len() {
            (observed.function + 1, 0)
        } else {
            (observed.function, observed.case + 1)
        };
    }
    // Finish the complete escaping budget before copying any recorded metadata.
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
        let args = case.args.iter().map(|(name, value)| Ok((name.clone(), scalar(value)?))).collect::<Result<serde_json::Map<_, _>, Error>>()?;
        Ok(json!({"function":definition.name, "call":definition.function, "source":source, "case":case.id, "args":args,
            "expected":scalar(&case.result)?, "comparison_decimals":definition.comparison_decimals,
            "outcome":match observed.outcome { TestOutcome::Attempted => "attempted", TestOutcome::Passed => "passed", TestOutcome::ResultMismatch => "result_mismatch", TestOutcome::InvocationFailed => "invocation_failed", TestOutcome::Interrupted => "interrupted" },
            "invoked":observed.invoked, "actual_retained":observed.actual.is_some(), "actual":observed.actual.as_ref().map(scalar).transpose()?}))
    }).collect::<Result<Vec<_>, Error>>()?;
    let result = json!({"lock":match observed.lock {
        LockObservation::NotRequested => "not_requested", LockObservation::Attempted => "attempted", LockObservation::Verified => "verified", LockObservation::Rejected => "rejected", LockObservation::Interrupted => "interrupted",
    }, "bindings":bindings, "tests":tests});
    if result.to_string().len() > maximum {
        return Err(Error::Limit);
    }
    Ok(result)
}
