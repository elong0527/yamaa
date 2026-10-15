"""Repository manifest and authored diagnostic contract checks."""

import re
from pathlib import Path

import yaml
from tools.benchmark import directories, load_metadata
from tools.benchmark import entries as benchmark_entries
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader


def validation_phase_contracts(root: Path):
    contracts = {}
    examples_dir = root / "benchmarks"
    if not examples_dir.is_dir():
        return contracts
    for example_dir in sorted(examples_dir.glob("negative-*")):
        error_path = example_dir / "expected" / "error.yaml"
        if not error_path.is_file():
            continue
        try:
            with open(error_path, "r", encoding="utf-8") as handle:
                contract = yaml.load(handle, Loader=UniqueKeyLoader)
        except (OSError, UnicodeError, yaml.YAMLError):
            continue
        if isinstance(contract, dict) and contract.get("phase") == "validation":
            contracts[example_dir.name] = contract
    return contracts


def load_validation_fixtures(root: Path):
    """Join each case's metadata with its independently authored error truth."""
    fixtures = {}
    errors = []
    directory = root / "benchmarks"
    if not directory.is_dir():
        return fixtures, errors
    contracts = validation_phase_contracts(root)
    # Focused fixture tests use the checkout's normative vocabulary; a complete
    # --root repository supplies its own conditions owner.
    vocabulary_root = root if (root / "yaml/conditions.yaml").is_file() else ROOT
    vocabulary, vocabulary_errors = load_condition_registry(vocabulary_root)
    errors.extend(vocabulary_errors)
    conditions = (
        vocabulary.get("conditions", {}) if isinstance(vocabulary, dict) else {}
    )
    for case in directories(directory):
        label = f"benchmarks/{case.name}/benchmark.yaml"
        try:
            metadata = load_metadata(case)
            benchmark_entries(case, metadata)
        except ValueError as error:
            errors.append(f"ERROR: {label}: {error}")
            continue
        declared = metadata.get("validation")
        contract = contracts.get(case.name)
        if declared is None:
            if contract is not None:
                errors.append(
                    f"ERROR: {label}.validation: missing validation declaration"
                )
            continue
        if contract is None:
            errors.append(f"ERROR: {label}.validation: stale or non-validation fixture")
            continue
        entry = {
            **declared,
            "condition": contract.get("condition"),
            "spec_paths": contract.get("spec_paths"),
        }
        fixtures[case.name] = entry
        condition = entry["condition"]
        registration = conditions.get(condition) if isinstance(condition, str) else None
        expected_label = f"benchmarks/{case.name}/expected/error.yaml"
        if not isinstance(registration, dict) or "validation" not in registration.get(
            "phases", []
        ):
            errors.append(
                f"ERROR: {expected_label}.condition: unregistered validation condition {condition!r}"
            )
        paths = entry["spec_paths"]
        if not (
            isinstance(paths, list)
            and paths
            and all(isinstance(path, str) and path for path in paths)
            and len(set(paths)) == len(paths)
        ):
            errors.append(
                f"ERROR: {expected_label}.spec_paths: expected unique non-empty paths"
            )
    return fixtures, errors








EXPECTED_ERROR_PHASES = {
    "parse",
    "planning",
    "engine",
    "validation",
    "ingest",
    "row_construction",
    "derivation",
    "output",
    "verification",
    "bind",
    "join",
    "mapping",
    "cut",
    "extract",
    "template",
    "impute",
    "convert",
    "final",
}


def load_condition_registry(root: Path):
    relative_path = Path("yaml/conditions.yaml")
    path = root / relative_path
    if not path.is_file():
        examples_dir = root / "benchmarks"
        if not examples_dir.is_dir() or not any(
            examples_dir.glob("negative-*/expected/error.yaml")
        ):
            return {"version": "1.0", "conditions": {}}, []
        return None, [f"ERROR: {relative_path}: condition registry is missing"]
    try:
        with open(path, "r", encoding="utf-8") as handle:
            registry = yaml.load(handle, Loader=UniqueKeyLoader)
    except (OSError, UnicodeError, yaml.YAMLError) as exc:
        return None, [f"ERROR: {relative_path}: {exc}"]
    return registry, []


def validate_condition_registry(root: Path, registry):
    errors = []
    label = "yaml/conditions.yaml"
    if not isinstance(registry, dict):
        return [f"ERROR: {label}: expected a mapping"]
    if registry.get("version") != "1.0":
        errors.append(f"ERROR: {label}.version: expected '1.0'")
    for field in sorted(set(registry) - {"version", "conditions"}):
        errors.append(f"ERROR: {label}.{field}: unknown field")
    conditions = registry.get("conditions")
    if not isinstance(conditions, dict):
        errors.append(f"ERROR: {label}.conditions: expected a mapping")
        return errors
    condition_names = list(conditions)
    if all(
        isinstance(condition, str) for condition in condition_names
    ) and condition_names != sorted(condition_names):
        errors.append(f"ERROR: {label}.conditions: expected sorted keys")

    for condition, registration in conditions.items():
        path = f"{label}.conditions.{condition}"
        if not (
            isinstance(condition, str)
            and re.fullmatch(r"[a-z][a-z0-9]*(?:_[a-z0-9]+)*", condition)
        ):
            errors.append(f"ERROR: {path}: expected a snake-case name")
        if not isinstance(registration, dict):
            errors.append(f"ERROR: {path}: expected a mapping")
            continue
        for field in sorted(set(registration) - {"phases"}):
            errors.append(f"ERROR: {path}.{field}: unknown field")
        phases = registration.get("phases")
        if not (
            isinstance(phases, list)
            and phases
            and all(
                isinstance(phase, str) and phase in EXPECTED_ERROR_PHASES
                for phase in phases
            )
            and phases == sorted(set(phases))
        ):
            errors.append(f"ERROR: {path}.phases: expected unique sorted phases")

    examples_dir = root / "benchmarks"
    if not examples_dir.is_dir():
        return errors
    for error_path in sorted(examples_dir.glob("negative-*/expected/error.yaml")):
        try:
            with open(error_path, "r", encoding="utf-8") as handle:
                contract = yaml.load(handle, Loader=UniqueKeyLoader)
        except (OSError, UnicodeError, yaml.YAMLError):
            continue
        if not isinstance(contract, dict):
            continue
        condition = contract.get("condition")
        phase = contract.get("phase")
        if not isinstance(condition, str) or not isinstance(phase, str):
            continue
        fixture_label = error_path.relative_to(root)
        registration = conditions.get(condition)
        if registration is None:
            errors.append(
                f"ERROR: {fixture_label}.condition: unregistered condition "
                f"{condition!r}"
            )
            continue
        if (
            isinstance(registration, dict)
            and isinstance(registration.get("phases"), list)
            and phase not in registration["phases"]
        ):
            errors.append(
                f"ERROR: {fixture_label}.phase: condition {condition!r} is "
                f"not registered for phase {phase!r}"
            )
    return errors




def validate_expected_error_contracts(root: Path):
    errors = []
    examples_dir = root / "benchmarks"
    if not examples_dir.exists():
        return errors
    for ex_dir in sorted(examples_dir.glob("negative-*")):
        if not ex_dir.is_dir():
            continue
        error_path = ex_dir / "expected" / "error.yaml"
        if not error_path.exists():
            continue
        label = error_path.relative_to(root)
        try:
            with open(error_path, "r", encoding="utf-8") as f:
                contract = yaml.load(f, Loader=UniqueKeyLoader)
        except Exception:
            continue
        if not isinstance(contract, dict):
            errors.append(f"ERROR: {label}: expected a mapping")
            continue
        unknown = sorted(
            set(contract)
            - {"phase", "condition", "spec_paths", "requirement", "context"}
        )
        for field in unknown:
            errors.append(f"ERROR: {label}.{field}: unknown field")
        if contract.get("phase") not in EXPECTED_ERROR_PHASES:
            errors.append(
                f"ERROR: {label}.phase: expected one of "
                f"{sorted(EXPECTED_ERROR_PHASES)!r}"
            )
        condition = contract.get("condition")
        if not (
            isinstance(condition, str)
            and re.fullmatch(r"[a-z][a-z0-9]*(?:_[a-z0-9]+)*", condition)
        ):
            errors.append(f"ERROR: {label}.condition: expected a snake-case name")
        paths = contract.get("spec_paths")
        if not (
            isinstance(paths, list)
            and paths
            and all(isinstance(path, str) and path for path in paths)
        ):
            errors.append(
                f"ERROR: {label}.spec_paths: expected a non-empty string list"
            )
            paths = []
        if "context" in contract and not isinstance(contract["context"], dict):
            errors.append(f"ERROR: {label}.context: expected a mapping")

    return errors


