"""Resolve and validate a project environment before any code is loaded.

REQ-0664 settles what a project claims independently of any specification, so
every case here reads an environment and nothing else. The fingerprint cases
are the cross-project half of that: a contract is an agreement only when two
projects calculate the same identity for it.
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

import pytest
import yaml

from yamaa.functions import FunctionFailure, contract_fingerprint, load_environment
from yamaa.functions.models import binding_arguments


def _failure(root: Path, schema: Path) -> FunctionFailure:
    with pytest.raises(FunctionFailure) as raised:
        load_environment(root, schema)
    return raised.value


def test_a_written_project_root_loads_its_contracts(bmi_project, repository) -> None:
    loaded = load_environment(bmi_project.path, repository.schema)

    assert loaded.environment.runtime.language == "python"
    assert sorted(loaded.environment.functions) == ["bmi"]
    assert loaded.conformance["bmi"].contract_version == "1.0.0"
    assert loaded.vector_identity.startswith("sha256:")


def test_a_root_with_no_environment_is_missing(project, repository) -> None:
    # REQ-0694: the implementation stage requires this document at the root
    # the runner selected, and nothing else stands in for it.
    failure = _failure(project.path, repository.schema)

    assert failure.condition == "project_environment_missing"
    assert failure.requirement == "REQ-0694"


def test_a_root_that_is_not_a_directory_is_missing(tmp_path, repository) -> None:
    failure = _failure(tmp_path / "not-a-project", repository.schema)

    assert failure.condition == "project_environment_missing"


def test_an_unreadable_environment_is_missing(
    bmi_project, repository, monkeypatch
) -> None:
    environment = bmi_project.path / "environment.yaml"
    read_bytes = Path.read_bytes

    def fail_environment(path):
        if path == environment:
            raise PermissionError("environment read denied")
        return read_bytes(path)

    monkeypatch.setattr(Path, "read_bytes", fail_environment)

    failure = _failure(bmi_project.path, repository.schema)

    assert failure.condition == "project_environment_missing"
    assert failure.requirement == "REQ-0694"
    assert failure.context["host_error"] == "PermissionError"


def test_an_environment_outside_its_schema_is_invalid(project, repository) -> None:
    project.write_code("def bmi():\n    return 1.0\n")
    project.write_vectors(repository.vectors)
    (project.path / "environment.yaml").write_text(
        'schema_version: "1.0"\nversion: "1.0.0"\n', "utf-8"
    )

    failure = _failure(project.path, repository.schema)

    assert failure.condition == "project_environment_invalid"
    assert failure.requirement == "REQ-0695"


def test_an_optional_parameter_without_a_default_is_invalid(
    bmi_project, repository
) -> None:
    # REQ-0676: every optional parameter declares an environment default,
    # because omitting the argument has to select something.
    bmi_project.edit_environment("        default: 100\n", "")

    failure = _failure(bmi_project.path, repository.schema)

    assert failure.condition == "project_environment_invalid"
    assert "default" in str(failure.context["reason"])


def test_a_required_parameter_with_a_default_is_invalid(
    bmi_project, repository
) -> None:
    bmi_project.edit_environment(
        "      - name: weight_kg\n        type: float\n",
        "      - name: weight_kg\n        type: float\n        default: 1.0\n",
    )

    failure = _failure(bmi_project.path, repository.schema)

    assert failure.condition == "project_environment_invalid"


def test_a_default_of_another_type_is_invalid(bmi_project, repository) -> None:
    # REQ-0678 admits no conversion, so an int parameter defaulting to a
    # float declares a value it could never be given.
    bmi_project.edit_environment("        default: 100\n", "        default: 100.0\n")

    failure = _failure(bmi_project.path, repository.schema)

    assert failure.condition == "project_environment_invalid"
    assert failure.context["expected"] == "int"
    assert failure.context["actual"] == "float"


def test_a_missing_default_is_valid_when_the_parameter_accepts_missing(
    bmi_project, repository
) -> None:
    bmi_project.edit_environment(
        "        default: 100\n        accepts_missing: false\n",
        "        default: null\n        accepts_missing: true\n",
    )

    loaded = load_environment(bmi_project.path, repository.schema)
    parameter = loaded.environment.functions["bmi"].parameters["cm_per_m"]

    assert parameter.has_default
    assert parameter.default is None


def test_a_missing_default_is_invalid_when_the_parameter_rejects_missing(
    bmi_project, repository
) -> None:
    bmi_project.edit_environment("        default: 100\n", "        default: null\n")

    failure = _failure(bmi_project.path, repository.schema)

    assert failure.condition == "project_environment_invalid"
    assert failure.requirement == "REQ-0695"
    assert failure.context["expected"] == "int"
    assert failure.context["actual"] is None


def test_a_binding_that_leaves_a_parameter_unmapped_is_invalid(
    bmi_project, repository
) -> None:
    # REQ-0683: the mapping covers the logical signature exactly.
    bmi_project.edit_environment("        cm_per_m: cm_per_m\n", "")

    failure = _failure(bmi_project.path, repository.schema)

    assert failure.condition == "project_environment_invalid"
    assert failure.context["unmapped"] == ["cm_per_m"]


def test_a_host_argument_name_that_is_a_keyword_is_invalid(
    bmi_project, repository
) -> None:
    bmi_project.edit_environment(
        "        cm_per_m: cm_per_m\n", "        cm_per_m: class\n"
    )

    failure = _failure(bmi_project.path, repository.schema)

    assert failure.condition == "project_environment_invalid"
    assert failure.context["host_argument"] == "class"


def test_an_omitted_binding_args_maps_each_parameter_to_itself(
    bmi_project, repository
) -> None:
    # REQ-0683: when every host argument carries its logical parameter's
    # name, the mapping stays unwritten.
    bmi_project.edit_environment(
        "      args:\n"
        "        weight_kg: weight_kg\n"
        "        height_cm: height_cm\n"
        "        cm_per_m: cm_per_m\n",
        "",
    )

    loaded = load_environment(bmi_project.path, repository.schema)

    assert binding_arguments(loaded.environment.functions["bmi"]) == {
        "weight_kg": "weight_kg",
        "height_cm": "height_cm",
        "cm_per_m": "cm_per_m",
    }


def test_an_omitted_implementation_version_defaults_to_the_environment_version(
    bmi_project, repository
) -> None:
    # REQ-0669: a project that versions its implementation with its
    # environment writes the version once.
    bmi_project.edit_environment('    implementation_version: "1.0.0"\n', "")

    loaded = load_environment(bmi_project.path, repository.schema)

    contract = loaded.environment.functions["bmi"]
    assert contract.implementation_version == loaded.environment.version


def test_a_binding_that_is_not_module_qualified_is_invalid(project, repository) -> None:
    # REQ-0684 refuses a computed or bare callable name; REQ-0683 requires a
    # statically written module-qualified one.
    project.write_code("def bmi(weight_kg, height_cm, cm_per_m=100):\n    return 1.0\n")
    project.write_vectors(repository.vectors)
    project.write_environment(call="bmi")

    failure = _failure(project.path, repository.schema)

    assert failure.condition == "project_environment_invalid"
    assert failure.context["call"] == "bmi"


def test_a_conformance_path_leaving_the_root_is_invalid(
    bmi_project, repository
) -> None:
    bmi_project.edit_environment(
        "    conformance: conformance/bmi.yaml",
        "    conformance: ../conformance/bmi.yaml",
    )

    failure = _failure(bmi_project.path, repository.schema)

    assert failure.condition == "project_environment_invalid"


def test_a_vector_document_naming_another_contract_is_invalid(
    bmi_project, repository
) -> None:
    # REQ-0687: a vector document identifies the same logical name and the
    # same contract version it activates.
    bmi_project.write_vectors(
        repository.vectors.replace(
            'contract_version: "1.0.0"', 'contract_version: "2.0.0"'
        )
    )

    failure = _failure(bmi_project.path, repository.schema)

    assert failure.condition == "project_environment_invalid"
    assert failure.context["expected"] == ["bmi", "1.0.0"]


def test_an_r_environment_loads_for_inspection_without_a_python_runner(
    repository,
) -> None:
    # REQ-0664 validates an environment independently of the runner, so the
    # committed R root is readable here; REQ-0667 is what refuses to run it.
    loaded = load_environment(repository.bmi_example, repository.schema)

    assert loaded.environment.runtime.language == "r"
    assert loaded.environment.functions["bmi"].binding.call == "projectbmi::bmi"


def test_the_python_root_and_the_r_root_claim_one_contract(repository) -> None:
    # REQ-0675: two projects claim the same logical contract only when their
    # calculated fingerprints are identical. REQ-0690 adds that they run the
    # same vector content, which is why the documents compare byte for byte.
    python_root = load_environment(repository.bmi_project, repository.schema)
    r_root = load_environment(repository.bmi_example, repository.schema)

    assert python_root.fingerprints["bmi"] == r_root.fingerprints["bmi"]
    assert (repository.bmi_project / "conformance/bmi.yaml").read_bytes() == (
        repository.bmi_example / "conformance/bmi.yaml"
    ).read_bytes()


def test_the_repository_validator_calculates_the_same_fingerprint(
    repository,
) -> None:
    """The static validator is the other implementation of REQ-0671.

    A fingerprint only means something when two implementations of the rule
    produce the same bytes, so this reads the committed validator's own
    function rather than a copy of its result.
    """
    specification = importlib.util.spec_from_file_location(
        "validate_repository_under_test", repository.validator
    )
    assert specification is not None and specification.loader is not None
    module = importlib.util.module_from_spec(specification)
    sys.modules[specification.name] = module
    try:
        specification.loader.exec_module(module)
        document = yaml.safe_load(
            (repository.bmi_project / "environment.yaml").read_text("utf-8")
        )
        expected = module.function_contract_fingerprint(
            "bmi", document["functions"]["bmi"]
        )
    finally:
        del sys.modules[specification.name]

    loaded = load_environment(repository.bmi_project, repository.schema)
    assert contract_fingerprint("bmi", loaded.environment.functions["bmi"]) == expected


def test_changing_a_parameter_changes_the_contract_identity(
    bmi_project, repository
) -> None:
    # REQ-0670: changing parameter order, names, types, requiredness,
    # defaults, or missing behavior requires a new contract version, and
    # the fingerprint is what makes that visible.
    before = load_environment(bmi_project.path, repository.schema).fingerprints["bmi"]
    bmi_project.edit_environment("        default: 100\n", "        default: 1000\n")

    after = load_environment(bmi_project.path, repository.schema).fingerprints["bmi"]
    assert after != before


def test_the_implementation_version_stays_out_of_the_identity(
    bmi_project, repository
) -> None:
    # REQ-0675 excludes it: changing only project code changes the
    # implementation version and the artifact, not the logical contract.
    before = load_environment(bmi_project.path, repository.schema).fingerprints["bmi"]
    bmi_project.edit_environment(
        '    implementation_version: "1.0.0"',
        '    implementation_version: "2.7.0"',
    )

    after = load_environment(bmi_project.path, repository.schema).fingerprints["bmi"]
    assert after == before
