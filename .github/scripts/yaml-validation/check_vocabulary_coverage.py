#!/usr/bin/env python3
"""Enforce yaml/examples/vocabulary-coverage.yaml in CI.

Every expression registered in yaml/schema_expression_*.yaml and every
verification registered in yaml/schema_verification.yaml must name either
the negative examples fixing its failure behavior or the open issue
tracking the gap. A covered entry must point at real negative examples:
each directory exists, carries expected/error.yaml, and either the error
spec_paths or the spec itself names the operation.
"""

import re
import sys
from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parents[3]
EXAMPLES = REPO / "yaml" / "examples"
COVERAGE = EXAMPLES / "vocabulary-coverage.yaml"
GAP = re.compile(r"#[1-9][0-9]*$")


def registered_operations():
    operations = {}
    for path in sorted((REPO / "yaml").glob("schema_expression_*.yaml")):
        document = yaml.safe_load(path.read_text(encoding="utf-8"))
        for name in document.get("expressions") or {}:
            operations[name] = path.name
    verification = yaml.safe_load(
        (REPO / "yaml" / "schema_verification.yaml").read_text(encoding="utf-8")
    )
    for group in ("column_verifications", "dataset_verifications"):
        for name in verification.get(group) or {}:
            operations[f"verify:{name}"] = "schema_verification.yaml"
    return operations


def check(operation, entry):
    errors = []
    if isinstance(entry, list):
        names = entry
        gap = None
    elif isinstance(entry, dict) and set(entry) == {"gap"}:
        names = []
        gap = entry["gap"]
    else:
        return [f"ERROR: coverage.{operation}: expected a list or {{gap: '#N'}}"]
    if gap is not None:
        if not isinstance(gap, str) or GAP.fullmatch(gap) is None:
            errors.append(f"ERROR: coverage.{operation}.gap: expected '#N'")
        return errors
    if not names:
        return [f"ERROR: coverage.{operation}: no negative example and no gap"]
    plain = operation.split(":", 1)[-1]
    for name in names:
        directory = EXAMPLES / name
        error = directory / "expected" / "error.yaml"
        spec = directory / "spec.yaml"
        if not directory.is_dir():
            errors.append(f"ERROR: coverage.{operation}: missing {name}/")
            continue
        if not error.is_file():
            errors.append(f"ERROR: coverage.{operation}: {name} lacks error.yaml")
            continue
        contract = yaml.safe_load(error.read_text(encoding="utf-8"))
        paths = " ".join(contract.get("spec_paths") or [])
        body = spec.read_text(encoding="utf-8") if spec.is_file() else ""
        if plain not in paths and plain not in body:
            errors.append(
                f"ERROR: coverage.{operation}: {name} never names {plain}"
            )
    return errors


def main():
    coverage = yaml.safe_load(COVERAGE.read_text(encoding="utf-8"))["coverage"]
    operations = registered_operations()
    errors = []
    for operation in sorted(operations):
        if operation not in coverage:
            errors.append(f"ERROR: coverage.{operation}: unmapped operation")
        else:
            errors.extend(check(operation, coverage[operation]))
    for operation in sorted(set(coverage) - set(operations)):
        errors.append(f"ERROR: coverage.{operation}: unknown operation")
    for error in errors:
        print(error)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
