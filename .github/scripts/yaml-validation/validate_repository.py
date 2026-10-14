"""Validate repository checks through their focused owners."""

import argparse
import sys
from pathlib import Path

import _native_specification
from _repository_yaml import UniqueKeyLoader
from editorial import validate_ascii_sources, validate_unicode_scalars
from grammar_checks import validate_grammar_contracts
from regex_checks import (
    RegexBindingUnavailable,
    require_regex_binding,
    validate_regex_conformance,
)
from repository_artifacts import (
    validate_csv_shapes,
    validate_examples_csv,
    validate_key_column_order,
)
from repository_manifests import (
    load_condition_registry,
    load_validation_manifest,
    validate_condition_registry,
    validate_expected_error_contracts,
    validate_validation_manifest,
)
from repository_schema import build_schema_env
from repository_structure import (
    validate_examples_define_documents,
    validate_examples_environment_documents,
    validate_examples_layout,
    validate_examples_structure,
)

import yaml


def check_yaml_files(root: Path):
    errors = []
    warnings = []
    errors.extend(validate_ascii_sources(root))
    for yaml_file in sorted(root.rglob("*.yaml")):
        if ".github" in yaml_file.parts:
            continue
        try:
            with open(yaml_file, "r", encoding="utf-8") as f:
                document = yaml.load(f, Loader=UniqueKeyLoader)
            errors.extend(
                validate_unicode_scalars(
                    document,
                    str(yaml_file.relative_to(root)),
                )
            )
        except Exception as e:
            msg = str(e)
            if hasattr(e, "problem_mark") and e.problem_mark:
                msg += f" at line {e.problem_mark.line + 1}"
            errors.append(f"ERROR: {yaml_file.relative_to(root)}: {msg}")

    # Also validate schemas
    env, schema_errors = build_schema_env(root)
    errors.extend(schema_errors)
    environment_schema = None
    if (root / "yaml" / "schema_environment.yaml").exists():
        environment_schema, environment_schema_errors = build_schema_env(
            root, "schema_environment.yaml"
        )
        errors.extend(environment_schema_errors)
        if environment_schema_errors:
            environment_schema = None
    validation_manifest, manifest_load_errors = load_validation_manifest(root)
    errors.extend(manifest_load_errors)
    if validation_manifest is not None:
        errors.extend(validate_validation_manifest(root, validation_manifest))
    condition_registry, condition_registry_load_errors = load_condition_registry(root)
    errors.extend(condition_registry_load_errors)
    if condition_registry is not None:
        errors.extend(validate_condition_registry(root, condition_registry))
    errors.extend(validate_examples_structure(root, env, warnings, validation_manifest))
    errors.extend(validate_examples_layout(root))
    if environment_schema is not None:
        errors.extend(validate_examples_environment_documents(root, environment_schema))
    errors.extend(validate_examples_define_documents(root))
    errors.extend(validate_expected_error_contracts(root))
    errors.extend(validate_csv_shapes(root))
    errors.extend(validate_key_column_order(root))
    errors.extend(validate_grammar_contracts(root))
    errors.extend(validate_regex_conformance(root))

    csv_errors, csv_warnings = validate_examples_csv(root, env)
    errors.extend(csv_errors)
    warnings.extend(csv_warnings)
    return errors, warnings


def main():
    parser = argparse.ArgumentParser(
        description="Validate yamaa repository structure and specs."
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[3],
        help="Repository root directory",
    )
    parser.add_argument(
        "--warnings-as-errors", action="store_true", help="Treat warnings as errors"
    )
    args = parser.parse_args()

    try:
        require_regex_binding()
    except RegexBindingUnavailable as exc:
        print(f"ERROR: {exc}")
        return 1

    errors, warnings = check_yaml_files(args.root)

    for warning in warnings:
        print(warning)

    if errors:
        for e in errors:
            print(e)
        return 1

    if warnings and args.warnings_as_errors:
        return 1

    if _native_specification.DEFERRED_ENVIRONMENTS:
        for environment in sorted(_native_specification.DEFERRED_ENVIRONMENTS):
            print(
                f"DEFERRED: R semantic source/binding/runtime coverage for {environment}; required installed R-host CI owns this lane."
            )
        print(
            "PASS: Available-host repository checks are clean; R semantic coverage is deferred."
        )
    else:
        print("PASS: Repository looks clean.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
