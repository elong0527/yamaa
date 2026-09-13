"""Activate a pinned project, and keep study data behind that activation.

R018-30 puts the vectors before every specification, R018-6 puts the
language and the artifact before the vectors, and R018-40 and R018-41 say
what happens when the code a run does reach misbehaves. Each case here runs
real pinned Python: the recording variant of the project's own code reports
what the binding was actually called with, in the order it was called.
"""

from __future__ import annotations

import importlib
import sys

import pytest
from conftest import RECORDING_CODE

from yamaa.functions import (
    ACTIVATION_CACHE,
    FunctionActivationError,
    activate_project_functions,
    execute_with_project_functions,
    results_match,
)
from yamaa.io.project import ProjectResources
from yamaa.io.source import load_source_tables
from yamaa.models import MISSING
from yamaa.runtime import ExecutionFailure, ExecutionSuccess
from yamaa.specification import load_specification

# The three vectors that invoke the binding; the other three supply a
# missing value to a non-accepting parameter and are short-circuited.
INVOKED_VECTORS = [(70.0, 175.0, 100), (80.0, 200.0, 100), (0.0, 175.0, 100)]
# The study rows of `adam-adsl-bmi-function` that reach the binding. The
# fourth subject has no height, so R018-20 answers it without a call.
STUDY_ROWS = [(81.0, 180.0, 100), (64.0, 160.0, 100), (45.0, 150.0, 100)]


def _specification(repository):
    return load_specification(
        repository.bmi_example / "spec.yaml", repository.schema
    ).specification


def _calls(activated):
    """Return what the recording artifact has been invoked with so far."""
    module = sys.modules[f"{activated.artifact.namespace}.projectbmi"]
    return list(module.CALLS)


def _activate(root, repository, **keywords):
    return activate_project_functions(
        _specification(repository), root.path, repository.schema, **keywords
    )


def _execute(root, repository, *, on_source=None):
    specification = _specification(repository)

    def provide(datasets):
        if on_source is not None:
            on_source()
        return load_source_tables(datasets, ProjectResources(repository.bmi_example))

    return execute_with_project_functions(
        specification, provide, root.path, repository.schema
    )


def _failure(root, repository) -> FunctionActivationError:
    with pytest.raises(FunctionActivationError) as raised:
        _activate(root, repository)
    return raised.value


@pytest.fixture
def recording_project(project, repository):
    """A correctly pinned project whose code records every invocation."""
    project.write_code(RECORDING_CODE)
    project.write_vectors(repository.vectors)
    project.write_environment()
    return project


def test_every_vector_runs_before_any_study_value_reaches_the_binding(
    recording_project, repository
) -> None:
    # R018-30: activation runs all vectors before any specification may
    # execute, so the order the binding itself observes is vectors first.
    activated = _activate(recording_project, repository)
    after_activation = _calls(activated)

    result = _execute(recording_project, repository)

    assert after_activation == INVOKED_VECTORS
    assert isinstance(result, ExecutionSuccess), result
    # The run activates the same unchanged project, which R018-30 lets the
    # cache answer, so the study rows follow one pass of the vectors.
    assert _calls(activated) == INVOKED_VECTORS + STUDY_ROWS


def test_a_missing_non_accepting_argument_never_reaches_the_binding(
    recording_project, repository
) -> None:
    # R018-20 and R018-21: the call is not invoked, its result is missing,
    # and that missing is not one the contract had to declare.
    activated = _activate(recording_project, repository)

    result = _execute(recording_project, repository)

    assert isinstance(result, ExecutionSuccess), result
    assert not any(None in call for call in _calls(activated))
    assert result.table.frame.to_dicts()[3]["BMI"] is None


def test_a_failing_vector_stops_the_run_before_any_study_data(
    project, repository
) -> None:
    # R018-42: a vector failure is the whole point of running them first.
    project.write_code(
        RECORDING_CODE.replace("(height_cm / cm_per_m) ** 2", "height_cm")
    )
    project.write_vectors(repository.vectors)
    project.write_environment()
    reached = []

    result = _execute(project, repository, on_source=lambda: reached.append("source"))

    assert isinstance(result, ExecutionFailure), result
    diagnostic = result.diagnostics[0]
    assert diagnostic.condition == "function_conformance_failed"
    assert diagnostic.requirement == "R018-42"
    assert diagnostic.context["case"] == "explicit-scale"
    assert reached == []


def test_a_mismatched_digest_is_refused_before_the_code_is_read(
    recording_project, repository
) -> None:
    # R018-6 verifies the artifact before activation, so nothing inside it
    # is imported when the bytes are not the ones that were pinned.
    recording_project.write_code(RECORDING_CODE + "\nEXTRA = 1\n")

    failure = _failure(recording_project, repository)

    diagnostic = failure.diagnostics[0]
    assert diagnostic.condition == "runtime_artifact_mismatch"
    assert diagnostic.requirement == "R018-36"
    assert diagnostic.context["declared"] != diagnostic.context["computed"]
    assert not [name for name in sys.modules if name.startswith("_yamaa_artifact_")]


def test_a_runner_that_does_not_support_the_language_refuses_the_project(
    project, repository
) -> None:
    # R018-6 and R018-35: this runner is Python, and an R project is not
    # something it may quietly run.
    project.write_code(RECORDING_CODE)
    project.write_vectors(repository.vectors)
    project.write_environment(language="r", call="projectbmi::bmi")

    failure = _failure(project, repository)

    diagnostic = failure.diagnostics[0]
    assert diagnostic.condition == "runner_language_mismatch"
    assert diagnostic.context == {"runner": "python", "declared": "r"}


def test_the_language_is_settled_before_the_calls_are(project, repository) -> None:
    # R018-6 puts the runner language first, so a project this runner cannot
    # execute is refused as that rather than as a contract it never reaches.
    project.write_code(RECORDING_CODE)
    project.write_vectors(
        repository.vectors.replace(
            'contract_version: "1.0.0"', 'contract_version: "2.0.0"'
        )
    )
    project.write_environment(
        language="r", call="projectbmi::bmi", contract_version="2.0.0"
    )

    failure = _failure(project, repository)

    assert failure.diagnostics[0].condition == "runner_language_mismatch"


def test_a_binding_loads_without_an_ambient_import_machinery(
    recording_project, repository, monkeypatch
) -> None:
    """Loading a binding depends on nothing another import happened to do.

    `importlib.machinery` is not reachable through `importlib` unless
    something imported it, and under pytest something always has. Removing
    it is what tells the two apart, so an ordinary application run loads
    project code the same way this suite does.
    """
    monkeypatch.delattr(importlib, "machinery", raising=False)
    monkeypatch.delitem(sys.modules, "importlib.machinery", raising=False)

    activated = _activate(recording_project, repository)

    assert activated.bound("bmi") is not None


def test_a_binding_is_resolved_inside_the_artifact_and_nowhere_else(
    project, repository
) -> None:
    # R018-5: an installed package on the process search path is not a
    # fallback, even when it does have the callable the binding names.
    project.write_code(RECORDING_CODE)
    project.write_vectors(repository.vectors)
    project.write_environment(call="json.dumps")

    failure = _failure(project, repository)

    diagnostic = failure.diagnostics[0]
    assert diagnostic.condition == "project_environment_invalid"
    assert diagnostic.context["reason"] == "the artifact contains no such module"


def test_an_unchanged_project_activates_once_and_a_repinned_one_again(
    recording_project, repository
) -> None:
    # R018-30 caches success for exactly one combination of identities, so
    # an unchanged project skips the vectors and a new artifact does not.
    first = _activate(recording_project, repository)
    second = _activate(recording_project, repository)

    recording_project.write_code(RECORDING_CODE.replace("CALLS = []", "CALLS = []\n"))
    recording_project.repin()
    third = _activate(recording_project, repository)

    assert first.vectors_executed
    assert not second.vectors_executed
    assert third.vectors_executed
    assert third.artifact.digest != first.artifact.digest


def test_changed_vector_content_activates_again(recording_project, repository) -> None:
    # The vectors are not part of the artifact, so R018-30 names their
    # content separately: editing a case is a different activation.
    _activate(recording_project, repository)
    recording_project.write_vectors(
        repository.vectors.replace("    result: 20.0\n", "    result: 20.00\n")
    )

    assert _activate(recording_project, repository).vectors_executed


def test_a_host_exception_during_a_study_row_is_fatal(project, repository) -> None:
    # R018-40: fatal, with no R008 local fallback. The vectors pass, so
    # this is the run reaching a row the vectors did not describe.
    project.write_code(
        RECORDING_CODE.replace(
            "    CALLS.append",
            "    if weight_kg == 81.0:\n"
            "        raise ZeroDivisionError('project code failed')\n"
            "    CALLS.append",
        )
    )
    project.write_vectors(repository.vectors)
    project.write_environment()

    result = _execute(project, repository)

    assert isinstance(result, ExecutionFailure), result
    diagnostic = result.diagnostics[0]
    assert diagnostic.condition == "function_call_failed"
    assert diagnostic.requirement == "R018-40"
    assert diagnostic.phase == "derivation"
    assert diagnostic.context["host_error"] == "ZeroDivisionError"
    assert diagnostic.context["function"] == "bmi"
    assert diagnostic.spec_paths == ("columns.BMI.derivation.function",)


@pytest.mark.parametrize(
    ("returned", "expected_context"),
    [
        ("'twenty-five'", {"expected": "float", "actual": "str"}),
        ("25", {"expected": "float", "actual": "int"}),
        ("[25.0]", {"reason": "a binding returned a value of no scalar type"}),
    ],
    ids=["text", "integer", "collection"],
)
def test_a_result_outside_the_declared_type_is_fatal(
    project, repository, returned, expected_context
) -> None:
    # R018-25: a binding returns one scalar of the declared exact type, and
    # R005 conversion is not a repair mechanism for anything else.
    project.write_code(
        RECORDING_CODE.replace(
            "    CALLS.append",
            f"    if weight_kg == 81.0:\n        return {returned}\n    CALLS.append",
        )
    )
    project.write_vectors(repository.vectors)
    project.write_environment()

    result = _execute(project, repository)

    assert isinstance(result, ExecutionFailure), result
    diagnostic = result.diagnostics[0]
    assert diagnostic.condition == "invalid_function_result"
    assert diagnostic.requirement == "R018-41"
    assert expected_context.items() <= diagnostic.context.items()


@pytest.mark.parametrize("returned", ["None", "float('inf')"], ids=["none", "infinity"])
def test_an_undeclared_missing_result_is_fatal(project, repository, returned) -> None:
    # R018-25: R011's non-finite normalization runs first, so a returned
    # infinity is a missing result a contract still has to declare.
    project.write_code(
        RECORDING_CODE.replace(
            "    CALLS.append",
            f"    if weight_kg == 81.0:\n        return {returned}\n    CALLS.append",
        )
    )
    project.write_vectors(repository.vectors)
    project.write_environment()

    result = _execute(project, repository)

    assert isinstance(result, ExecutionFailure), result
    diagnostic = result.diagnostics[0]
    assert diagnostic.condition == "invalid_function_result"
    assert "undeclared missing" in str(diagnostic.context["reason"])


def test_a_declared_nullable_binding_may_return_missing(project, repository) -> None:
    # The same returned value, once the contract says it can happen.
    project.write_code(
        RECORDING_CODE.replace(
            "    CALLS.append",
            "    if weight_kg == 81.0:\n        return None\n    CALLS.append",
        )
    )
    project.write_vectors(repository.vectors)
    project.write_environment(may_return_missing="true")

    result = _execute(project, repository)

    assert isinstance(result, ExecutionSuccess), result
    assert result.table.frame.to_dicts()[0]["BMI"] is None


def test_an_accepting_parameter_receives_the_host_missing_scalar(
    project, repository
) -> None:
    # R018-20: a missing value for an accepting parameter is passed to the
    # binding as the host runtime's canonical missing scalar.
    project.write_code(
        RECORDING_CODE.replace(
            "    CALLS.append",
            "    if height_cm is None:\n        return -1.0\n    CALLS.append",
        )
    )
    project.write_vectors(
        repository.vectors.replace(
            """  - id: missing-height-short-circuits
    covers: [short-circuit-missing:height_cm]
    args:
      weight_kg: 70.0
      height_cm: null
    result: null""",
            """  - id: missing-height-accepted
    covers: [accepted-missing:height_cm]
    args:
      weight_kg: 70.0
      height_cm: null
    result: -1.0""",
        )
    )
    project.write_environment(accepts_missing="true")

    result = _execute(project, repository)

    assert isinstance(result, ExecutionSuccess), result
    assert result.table.frame.to_dicts()[3]["BMI"] == -1.0


def test_the_activation_cache_is_not_consulted_when_a_caller_declines_it(
    recording_project, repository
) -> None:
    first = _activate(recording_project, repository, cache=None)
    second = _activate(recording_project, repository, cache=None)

    assert first.vectors_executed and second.vectors_executed
    assert not ACTIVATION_CACHE.passed(ACTIVATION_CACHE.key(second.environment))


@pytest.mark.parametrize(
    ("actual", "expected", "decimals", "matches"),
    [
        (22.857142857142858, 22.8571, 4, True),
        (22.857142857142858, 22.8572, 4, False),
        (1.23445, 1.2345, 4, True),
        (-1.23445, -1.2345, 4, True),
        (1.0, 1, 0, False),
        (MISSING, MISSING, 4, True),
        (MISSING, 0.0, 4, False),
        ("SEVERE", "SEVERE", 4, True),
    ],
    ids=[
        "float-within-precision",
        "float-beyond-precision",
        "positive-tie-away-from-zero",
        "negative-tie-away-from-zero",
        "no-implicit-widening",
        "missing-equals-missing",
        "missing-is-not-a-value",
        "text-compares-exactly",
    ],
)
def test_a_result_compares_under_the_contract_precision(
    actual, expected, decimals, matches
) -> None:
    # R018-31 compares temporary decimal copies at the contract's
    # precision, with an exact decimal tie going away from zero. R018-32
    # keeps that off the value, which is why this takes copies of both.
    assert results_match(actual, expected, decimals) is matches
