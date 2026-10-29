"""Repository manifest and authored diagnostic contract checks."""

import re
from pathlib import Path

import yaml
from tools.benchmark import check_standard_model, directories, load_metadata
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
    """Collect each validation-phase case's independently authored error truth."""
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
            check_standard_model(case, metadata)
        except ValueError as error:
            errors.append(f"ERROR: {label}: {error}")
            continue
        contract = contracts.get(case.name)
        if contract is None:
            continue
        entry = {
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


# A `{key}` placeholder, a doubled literal brace, or a stray brace.
MESSAGE_TOKEN = re.compile(r"\{\{|\}\}|\{([a-z][a-z0-9_]*)\}|[{}]")


def message_placeholders(template: str) -> list[str] | None:
    """The keys a message template names, or None when a brace is stray."""
    keys = []
    for token in MESSAGE_TOKEN.finditer(template):
        if token[0] in ("{{", "}}"):
            continue
        if token[1] is None:
            return None
        keys.append(token[1])
    return keys


def message_errors(template) -> list[str]:
    """Why a condition's message template breaks the message form (P-MSG-1).

    A message is one plain clause: printable ASCII on one line, starting in
    lower case or with a placeholder, with no final period. Braces are
    `{key}` placeholders or doubled literal braces.
    """
    if not isinstance(template, str) or not template:
        return ["expected non-empty text"]
    errors = []
    if not all(" " <= character <= "~" for character in template):
        errors.append("expected printable ASCII on one line")
    if template != template.strip() or "  " in template:
        errors.append("expected single spaces and no surrounding space")
    if not (
        "a" <= template[0] <= "z"
        or (template[0] == "{" and not template.startswith("{{"))
    ):
        errors.append("expected a lower-case start or a placeholder")
    if template.endswith("."):
        errors.append("expected no final period")
    if message_placeholders(template) is None:
        errors.append("expected `{key}` placeholders and doubled literal braces")
    return errors


SNAKE_CASE = re.compile(r"[a-z][a-z0-9]*(?:_[a-z0-9]+)*")
# A release, as `retired_in` names the one that retired a condition.
RELEASE = re.compile(r"(?:0|[1-9][0-9]*)(?:\.(?:0|[1-9][0-9]*)){2}")


def retired_conditions(registry) -> dict:
    """Each retired condition of a readable registry, with its entry."""
    retired = registry.get("retired") if isinstance(registry, dict) else None
    return retired if isinstance(retired, dict) else {}


def retired_errors(label: str, registry, conditions: dict) -> list[str]:
    """Why the registry's `retired` section is malformed: each entry names a
    condition no longer registered and the release that retired it."""
    retired = registry.get("retired", {})
    if not isinstance(retired, dict):
        return [f"ERROR: {label}.retired: expected a mapping"]
    errors = []
    names = list(retired)
    if all(isinstance(name, str) for name in names) and names != sorted(names):
        errors.append(f"ERROR: {label}.retired: expected sorted keys")
    for condition, entry in retired.items():
        path = f"{label}.retired.{condition}"
        if not (isinstance(condition, str) and SNAKE_CASE.fullmatch(condition)):
            errors.append(f"ERROR: {path}: expected a snake-case name")
        elif condition in conditions:
            errors.append(f"ERROR: {path}: a registered condition is not retired")
        if not isinstance(entry, dict):
            errors.append(f"ERROR: {path}: expected a mapping")
            continue
        for field in sorted(set(entry) - {"retired_in"}, key=str):
            errors.append(f"ERROR: {path}.{field}: unknown field")
        release = entry.get("retired_in")
        if not (isinstance(release, str) and RELEASE.fullmatch(release)):
            errors.append(
                f"ERROR: {path}.retired_in: expected the release that retired it, "
                "such as '0.6.0'"
            )
    return errors


def renamed_from_errors(label: str, conditions: dict, retired: dict) -> list[str]:
    """Why a registration's `renamed_from` is malformed: the former name is a
    snake-case name that no condition still has or retired with, and that
    only one condition had."""
    errors = []
    former = {}
    for condition, registration in conditions.items():
        if not isinstance(registration, dict) or "renamed_from" not in registration:
            continue
        path = f"{label}.conditions.{condition}.renamed_from"
        name = registration["renamed_from"]
        if not (isinstance(name, str) and SNAKE_CASE.fullmatch(name)):
            errors.append(f"ERROR: {path}: expected a snake-case name")
        elif name in conditions or name in retired:
            errors.append(f"ERROR: {path}: {name} is still a condition of the registry")
        elif name in former:
            errors.append(f"ERROR: {path}: {former[name]} was renamed from {name} too")
        else:
            former[name] = condition
    return errors


def registry_errors(registry) -> list[str]:
    """Why the condition registry's own form is wrong: its sections, each
    registration's name, phases, message and former name, and each retired
    condition."""
    errors = []
    label = "yaml/conditions.yaml"
    if not isinstance(registry, dict):
        return [f"ERROR: {label}: expected a mapping"]
    if registry.get("version") != "1.0":
        errors.append(f"ERROR: {label}.version: expected '1.0'")
    for field in sorted(set(registry) - {"version", "conditions", "retired"}):
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
    errors.extend(retired_errors(label, registry, conditions))
    retired = retired_conditions(registry)
    errors.extend(renamed_from_errors(label, conditions, retired))

    for condition, registration in conditions.items():
        path = f"{label}.conditions.{condition}"
        if not (isinstance(condition, str) and SNAKE_CASE.fullmatch(condition)):
            errors.append(f"ERROR: {path}: expected a snake-case name")
        if not isinstance(registration, dict):
            errors.append(f"ERROR: {path}: expected a mapping")
            continue
        for field in sorted(
            set(registration)
            - {"phases", "message", "message_variants", "renamed_from"}
        ):
            errors.append(f"ERROR: {path}.{field}: unknown field")
        for problem in message_errors(registration.get("message")):
            errors.append(f"ERROR: {path}.message: {problem}")
        variants = registration.get("message_variants", [])
        if not isinstance(variants, list):
            errors.append(f"ERROR: {path}.message_variants: expected a list")
        else:
            seen = set()
            for index, variant in enumerate(variants):
                vpath = f"{path}.message_variants[{index}]"
                if not isinstance(variant, dict) or set(variant) != {"when", "message"}:
                    errors.append(f"ERROR: {vpath}: expected when and message")
                    continue
                key = variant["when"]
                if (
                    not isinstance(key, str)
                    or not SNAKE_CASE.fullmatch(key)
                    or key in seen
                ):
                    errors.append(f"ERROR: {vpath}.when: expected a unique context key")
                else:
                    seen.add(key)
                for problem in message_errors(variant["message"]):
                    errors.append(f"ERROR: {vpath}.message: {problem}")
        phases = registration.get("phases")
        if not (
            isinstance(phases, list)
            and phases
            and all(
                isinstance(phase, str) and SNAKE_CASE.fullmatch(phase)
                for phase in phases
            )
            and phases == sorted(set(phases))
        ):
            errors.append(f"ERROR: {path}.phases: expected unique sorted phases")
    return errors


def validate_condition_registry(root: Path, registry):
    errors = registry_errors(registry)
    conditions = registry.get("conditions") if isinstance(registry, dict) else None
    if not isinstance(conditions, dict):
        return errors
    retired = retired_conditions(registry)
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
                f"ERROR: {fixture_label}.condition: "
                + (
                    f"condition {condition!r} is retired"
                    if condition in retired
                    else f"unregistered condition {condition!r}"
                )
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
    vocabulary_root = root if (root / "yaml/conditions.yaml").is_file() else ROOT
    registry, _ = load_condition_registry(vocabulary_root)
    conditions = registry.get("conditions", {}) if isinstance(registry, dict) else {}
    phases = {
        phase
        for registration in (
            conditions.values() if isinstance(conditions, dict) else ()
        )
        if isinstance(registration, dict)
        and isinstance(registration.get("phases"), list)
        for phase in registration["phases"]
        if isinstance(phase, str)
    }
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
            set(contract) - {"phase", "condition", "spec_paths", "rule", "context"}
        )
        for field in unknown:
            errors.append(f"ERROR: {label}.{field}: unknown field")
        if contract.get("phase") not in phases:
            errors.append(f"ERROR: {label}.phase: expected one of {sorted(phases)!r}")
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
