"""Check repository source, authored fixture metadata, file layout and prose."""

import argparse
from pathlib import Path

import yaml
from tools.paths import ROOT
from tools.rules.model import authored_files
from tools.validation._repository_yaml import UniqueKeyLoader
from tools.validation.editorial import validate_ascii_sources, validate_unicode_scalars
from tools.validation.grammar_checks import validate_grammar_contracts
from tools.validation.repository_artifacts import (
    validate_examples_csv,
    validate_key_column_order,
)
from tools.validation.repository_manifests import (
    load_condition_registry,
    load_validation_fixtures,
    validate_condition_registry,
    validate_expected_error_contracts,
)
from tools.validation.repository_schema import validate_schema_sources
from tools.validation.repository_structure import validate_examples_layout


def check_yaml_files(root: Path):
    errors = list(validate_ascii_sources(root))
    for path in sorted(authored_files(root, {".yaml"})):
        label = path.relative_to(root)
        if ".github" in label.parts:
            continue
        try:
            document = yaml.load(
                path.read_text(encoding="utf-8"), Loader=UniqueKeyLoader
            )
            errors.extend(validate_unicode_scalars(document, str(label)))
        except (OSError, UnicodeError, yaml.YAMLError, ValueError) as error:
            message = str(error)
            mark = getattr(error, "problem_mark", None)
            if mark is not None:
                message += f" at line {mark.line + 1}"
            errors.append(f"ERROR: {label}: {message}")
    errors.extend(validate_schema_sources(root))
    _, fixture_errors = load_validation_fixtures(root)
    errors.extend(fixture_errors)
    registry, registry_errors = load_condition_registry(root)
    errors.extend(registry_errors)
    if registry is not None:
        errors.extend(validate_condition_registry(root, registry))
    errors.extend(validate_examples_layout(root))
    errors.extend(validate_expected_error_contracts(root))
    errors.extend(validate_key_column_order(root))
    errors.extend(validate_grammar_contracts(root))
    errors.extend(validate_examples_csv(root))
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root", type=Path, default=ROOT, help="Repository root directory"
    )
    args = parser.parse_args()
    errors = check_yaml_files(args.root)
    for error in errors:
        print(error)
    if errors:
        return 1
    print(
        "PASS: Repository sources and layout are clean; native suites own language conformance."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
