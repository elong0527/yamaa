"""Repository layout, authored document and editorial orchestration."""

from pathlib import Path

import yaml
from tools.validation._repository_yaml import UniqueKeyLoader
from tools.validation.editorial import example_spec_paths
from tools.validation.repository_artifacts import (
    example_entry_specs,
    validate_expected_resolved_fixture,
)
from tools.validation.repository_manifests import (
    validate_registered_fixture_diagnostics,
)
from tools.validation.repository_schema import (
    build_schema_env,
    validate_spec_document,
    validate_type,
)

HISTORICAL_RESOURCE_FIXTURES = frozenset(
    {
        "negative-path-absolute",
        "negative-path-parent-escape",
    }
)


def validate_examples_structure(root: Path, env, warnings=None, fixtures=None):
    errors = []
    if warnings is None:
        warnings = []
    if env is None:
        return errors

    examples_dir = root / "benchmarks"
    if not examples_dir.exists():
        return errors

    fixtures = {} if fixtures is None else fixtures
    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith("."):
            continue

        if ex_dir.name in HISTORICAL_RESOURCE_FIXTURES:
            # Structural, schema, README and expected-contract checks still run
            # in their own passes; the retired containment verdict does not.
            continue

        example_errors = []
        spec_labels = []
        for spec_path in example_entry_specs(ex_dir):
            try:
                with open(spec_path, "r", encoding="utf-8") as f:
                    spec = yaml.load(f, Loader=UniqueKeyLoader)
            except Exception:
                continue

            spec_label = f"{ex_dir.name}/{spec_path.name}"
            spec_labels.append(spec_label)
            is_negative = ex_dir.name.startswith("negative-")
            spec_errors = validate_spec_document(spec, spec_label, spec_path, env)
            if not is_negative:
                errors.extend(spec_errors)
                errors.extend(
                    validate_expected_resolved_fixture(
                        spec, spec_label, spec_path, ex_dir, env
                    )
                )
                continue

            example_errors.extend(spec_errors)

        if not ex_dir.name.startswith("negative-"):
            continue

        error_yaml_path = ex_dir / "expected" / "error.yaml"
        if not error_yaml_path.exists():
            errors.extend(example_errors)
            continue

        try:
            with open(error_yaml_path, "r", encoding="utf-8") as f:
                err_spec = yaml.load(f, Loader=UniqueKeyLoader)
        except Exception:
            errors.extend(example_errors)
            continue
        if not (isinstance(err_spec, dict) and err_spec.get("phase") == "validation"):
            errors.extend(example_errors)
            continue

        entry = fixtures.get(ex_dir.name)
        if isinstance(entry, dict):
            errors.extend(
                validate_registered_fixture_diagnostics(
                    ex_dir.name, entry, example_errors, spec_labels
                )
            )
            continue

        # Focused schema checks still compare independently authored paths;
        # repository validation requires the metadata owner before this pass.
        expected_paths = err_spec.get("spec_paths", [])
        if not isinstance(expected_paths, list):
            expected_paths = [expected_paths]
        for err in example_errors:
            parts = err.split(": ", 2)
            if len(parts) < 2:
                errors.append(err)
                continue
            path_part = parts[1]
            norm_path = path_part
            for spec_label in spec_labels:
                prefix = f"{spec_label}."
                if norm_path.startswith(prefix):
                    norm_path = norm_path[len(prefix) :]
                    break
            if not any(
                norm_path == expected_path
                or norm_path.startswith(f"{expected_path}.")
                or norm_path.startswith(f"{expected_path}[")
                for expected_path in expected_paths
            ):
                errors.append(err)

    return errors


def validate_examples_environment_documents(root: Path, env=None):
    """Admit benchmark environment roots without invoking domain planning.

    Function calls retain their native static validator. Submission fixtures
    also need schema admission when they declare no execution function calls;
    this does not request Define rendering or a published dataset.
    """
    documents = sorted(
        set((root / "benchmarks").glob("*/environment.yaml"))
        | set((root / "benchmarks").glob("*/python/environment.yaml"))
        | set((root / "benchmarks").glob("*/r/environment.yaml"))
    )
    if not documents:
        return []
    errors = []
    if env is None:
        env, errors = build_schema_env(root, entrypoint="schema_environment.yaml")
        if env is None or errors:
            return errors
    for path in documents:
        label = str(path.relative_to(root))
        try:
            document = yaml.load(
                path.read_text(encoding="utf-8"), Loader=UniqueKeyLoader
            )
        except (OSError, UnicodeError, yaml.YAMLError) as exc:
            errors.append(f"ERROR: {label}: {exc}")
            continue
        errors.extend(validate_type(document, ["environment_class"], env, label))
    return errors


def validate_examples_define_documents(root: Path):
    """Validate each study document against the entry point R026 defines.

    A define document is a separate entry point with its own class, so the
    specification schema never sees it. Nothing else reads these files, and
    a field renamed in the specification language would otherwise reach one
    unnoticed.
    """
    errors = []
    documents = sorted((root / "benchmarks").glob("*/define.yaml"))
    if not documents:
        return errors

    env, env_errors = build_schema_env(root, entrypoint="schema_define.yaml")
    if env is None:
        return env_errors
    errors.extend(env_errors)

    for path in documents:
        label = str(path.relative_to(root))
        try:
            with open(path, "r", encoding="utf-8") as handle:
                document = yaml.load(handle, Loader=UniqueKeyLoader)
        except (OSError, UnicodeError, yaml.YAMLError) as exc:
            errors.append(f"ERROR: {label}: {exc}")
            continue
        if not isinstance(document, dict):
            errors.append(f"ERROR: {label}: document must be a mapping")
            continue
        errors.extend(validate_type(document, ["define_class"], env, label))
    return errors


def validate_examples_layout(root: Path):
    errors = []
    examples_dir = root / "benchmarks"
    if not examples_dir.exists():
        return errors

    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith("."):
            continue

        rel = ex_dir.relative_to(root)

        # Specification files
        spec_paths = example_spec_paths(ex_dir)
        if not spec_paths:
            errors.append(f"ERROR: {rel} missing spec.yaml or spec_<variant>.yaml")
        elif len(spec_paths) > 1 and any(
            path.name == "spec.yaml" for path in spec_paths
        ):
            errors.append(f"ERROR: {rel} cannot mix spec.yaml with variant specs")

        # input/
        if not (ex_dir / "input").is_dir():
            errors.append(f"ERROR: {rel} missing input/ directory")

        # expected/
        expected_dir = ex_dir / "expected"
        if not expected_dir.is_dir():
            errors.append(f"ERROR: {rel} missing expected/ directory")
        else:
            is_negative = ex_dir.name.startswith("negative-")
            has_error_yaml = (expected_dir / "error.yaml").exists()
            if is_negative and not has_error_yaml:
                errors.append(
                    f"ERROR: {rel} is negative but missing expected/error.yaml"
                )
            elif not is_negative and has_error_yaml:
                errors.append(f"ERROR: {rel} is positive but has expected/error.yaml")

            has_artifacts = any(
                f.is_file() for f in expected_dir.iterdir() if f.name != "error.yaml"
            )
            if not is_negative and not has_artifacts:
                errors.append(f"ERROR: {rel}/expected has no artifacts")

    return errors
