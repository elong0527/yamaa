"""Execute the committed R018 examples against a real Python project root.

`adam-adsl-bmi-function` is the same logical specification in both projects:
the committed root implements `bmi` in R, and `python/tests/projects/
bmi-python` implements the same contract here. Nothing in `spec.yaml`
changes between them, which is the portability R018 exists for. Reading
`expected/` happens here, in the test, and never in runtime code.
"""

from __future__ import annotations

import pytest
import yaml
from conftest import RECORDING_CODE

from yamaa.functions import execute_with_project_functions
from yamaa.io import render_csv
from yamaa.io.project import ProjectResources
from yamaa.io.source import load_source_tables
from yamaa.runtime import (
    ExecutionFailure,
    ExecutionSuccess,
    ExecutionUnsupported,
    execute_with_source_provider,
)
from yamaa.specification import load_specification


def _specification(repository, example):
    return load_specification(
        repository.examples / example / "spec.yaml", repository.schema
    ).specification


def _run(repository, example, project_root):
    specification = _specification(repository, example)
    directory = repository.examples / example
    return execute_with_project_functions(
        specification,
        lambda datasets: load_source_tables(datasets, ProjectResources(directory)),
        project_root,
        repository.schema,
    )


def test_the_bmi_example_reproduces_its_committed_artifact(repository) -> None:
    result = _run(repository, "adam-adsl-bmi-function", repository.bmi_project)

    assert isinstance(result, ExecutionSuccess), result
    committed = (repository.bmi_example / "expected" / "adsl.csv").read_bytes()
    assert render_csv(result.artifact) == committed


def test_the_pinned_code_decides_the_values_and_not_the_declaration(
    repository, project
) -> None:
    """The same specification and the same contract, over different code.

    A declaration that validated and a vector file that passed would give
    the same artifact whatever the project computed. Pinning one subject's
    result to another value, with every vector still passing, is what shows
    the artifact came out of the binding.
    """
    project.write_code(
        RECORDING_CODE.replace(
            "    CALLS.append",
            "    if weight_kg == 81.0:\n        return 0.0\n    CALLS.append",
        )
    )
    project.write_vectors(repository.vectors)
    project.write_environment()

    result = _run(repository, "adam-adsl-bmi-function", project.path)

    assert isinstance(result, ExecutionSuccess), result
    produced = render_csv(result.artifact).decode("utf-8").splitlines()
    committed = (
        (repository.bmi_example / "expected" / "adsl.csv")
        .read_text("utf-8")
        .splitlines()
    )
    assert produced[1] == "CATH,CATH-001,180,81,0"
    assert committed[1] == "CATH,CATH-001,180,81,25"
    assert produced[2:] == committed[2:]


def test_the_committed_contract_mismatch_is_reproduced(repository) -> None:
    # The negative example selects its own directory as the project root:
    # the environment there provides `project_value` at contract 1.0.0 and
    # the specification asks for 2.0.0.
    example = repository.examples / "negative-function-contract-mismatch"
    committed = yaml.safe_load((example / "expected" / "error.yaml").read_text("utf-8"))

    result = _run(repository, "negative-function-contract-mismatch", example)

    assert isinstance(result, ExecutionFailure), result
    diagnostic = result.diagnostics[0]
    assert diagnostic.phase == committed["phase"]
    assert diagnostic.condition == committed["condition"]
    assert list(diagnostic.spec_paths) == committed["spec_paths"]
    assert diagnostic.requirement == committed["requirement"]
    assert committed["context"].items() <= diagnostic.context.items()


def test_a_rejected_run_reads_no_study_data(repository) -> None:
    # The negative example's artifact digest names nothing that exists, so
    # a run that read data before settling the contract would fail on the
    # artifact instead, and a run that read data at all would be wrong.
    example = repository.examples / "negative-function-contract-mismatch"
    specification = _specification(repository, "negative-function-contract-mismatch")
    reached: list[str] = []

    def provide(datasets):
        reached.append("source")
        return load_source_tables(datasets, ProjectResources(example))

    result = execute_with_project_functions(
        specification, provide, example, repository.schema
    )

    assert isinstance(result, ExecutionFailure), result
    assert reached == []


def test_the_committed_r_project_root_is_refused_by_this_runner(repository) -> None:
    # REQ-0667: the same example, with the project root that implements it in
    # R. A Python runner does not run it and does not pretend to.
    result = _run(repository, "adam-adsl-bmi-function", repository.bmi_example)

    assert isinstance(result, ExecutionFailure), result
    assert result.diagnostics[0].condition == "runner_language_mismatch"


def test_the_specification_stays_portable_with_no_project_selected(
    repository,
) -> None:
    # REQ-0662: a portable specification may declare a logical call before a
    # project implements it, so executing without a selected root reports
    # an unimplemented operation rather than inventing a result.
    specification = _specification(repository, "adam-adsl-bmi-function")

    result = execute_with_source_provider(
        specification,
        lambda datasets: load_source_tables(
            datasets, ProjectResources(repository.bmi_example)
        ),
    )

    assert isinstance(result, ExecutionUnsupported), result
    assert result.features[0].operation == "function"


def test_repeated_execution_produces_an_identical_artifact(repository) -> None:
    first = _run(repository, "adam-adsl-bmi-function", repository.bmi_project)
    second = _run(repository, "adam-adsl-bmi-function", repository.bmi_project)

    assert isinstance(first, ExecutionSuccess)
    assert isinstance(second, ExecutionSuccess)
    assert render_csv(first.artifact) == render_csv(second.artifact)


@pytest.mark.parametrize(
    ("example", "condition"),
    [
        ("adam-adsl-bmi-function", "runner_language_mismatch"),
        ("negative-function-contract-mismatch", "function_contract_mismatch"),
    ],
)
def test_every_failure_names_the_call_that_required_an_implementation(
    repository, example, condition
) -> None:
    # REQ-0704: a failure is reported against the specification text that
    # asked for project code, whichever stage discovered it.
    result = _run(repository, example, repository.examples / example)

    assert isinstance(result, ExecutionFailure), result
    diagnostic = result.diagnostics[0]
    assert diagnostic.condition == condition
    assert diagnostic.spec_paths[0].endswith(
        ("function.name", "function.contract_version")
    )
