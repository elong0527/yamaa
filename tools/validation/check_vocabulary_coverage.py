"""Require authored failure coverage beside benchmark and shared fixtures.

Every expression registered in yaml/schema_expression_*.yaml and every
verification registered in yaml/schema_verification.yaml must name negative
examples or shared installed refusal cases.
A benchmark entry must point at real negative examples:
each directory exists, carries expected/error.yaml, and either the error
spec_paths or the spec itself names the operation. A spec in the 1.0 surface
writes `compute` and `literal` as grammar text in the expression field (#3030
D18), so there the field an error path names must hold a formula or a literal.
Shared cases use the independent owner exercised by Rust, installed Python
and R; successful cases cannot satisfy failure coverage.
"""

import json
import re
import sys

import yaml
from tools import surface
from tools.benchmark import directories, load_metadata
from tools.paths import ROOT

REPO = ROOT
EXAMPLES = REPO / "benchmarks"
SHARED = REPO / "rust/tests/host-fixtures/cases.json"
# The operations 1.0 writes as grammar text in an expression field, and what
# that text is (D18; the rename table's compute_to_text and
# literal_to_expression transforms).
MERGED = {"compute": "formula", "literal": "literal"}
# Keyed members a spec path addresses by identity (P-CLR-2); any other list
# is addressed by [N].
IDENTITY = {"columns": "name", "intermediates": "id", "rows": "id"}
SEGMENT = re.compile(r"\.?([^.\[\]]+)|\[(\d+)\]")


def resolve(document, path):
    """The node a diagnostic spec path names in a parsed document, or None."""
    node, owner = document, None
    for name, index in SEGMENT.findall(path):
        if index:
            if not isinstance(node, list) or int(index) >= len(node):
                return None
            node, owner = node[int(index)], None
        elif isinstance(node, dict) and name in node:
            node, owner = node[name], name
        elif isinstance(node, list) and owner in IDENTITY:
            node = next(
                (
                    item
                    for item in node
                    if isinstance(item, dict) and item.get(IDENTITY[owner]) == name
                ),
                None,
            )
            if node is None:
                return None
            owner = None
        else:
            return None
    return node


def names_merged(operation, spec, paths):
    """Whether a 1.0 spec holds `operation` as grammar text at an error path."""
    try:
        document = yaml.safe_load(spec.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, yaml.YAMLError):
        return False
    if operation not in MERGED or not surface.declares_header(document):
        return False
    return any(
        surface.expression(node)[0] == MERGED[operation]
        for node in (resolve(document, path) for path in paths)
        if isinstance(node, (str, int, float, bool))
    )


def check_shared(operation, names):
    """Use the existing independently authored Rust/Python/R refusal owner."""
    label = f"coverage.{operation}.shared_cases"
    if (
        not isinstance(names, list)
        or not names
        or not all(isinstance(name, str) for name in names)
    ):
        return [f"ERROR: {label}: expected nonempty case names"]
    try:
        document = json.loads(SHARED.read_text(encoding="utf-8"))
        if document["version"] != "1":
            raise ValueError("unsupported shared fixture version")
        cases = document["families"]["original-specifications"]["temporal_arithmetic"]
        indexed = {case["name"]: case for case in cases}
        if len(indexed) != len(cases):
            raise ValueError("duplicate shared case names")
    except (OSError, KeyError, TypeError, ValueError) as error:
        return [f"ERROR: {label}: cannot read owned fixtures: {error}"]
    errors = []
    plain = operation.split(":", 1)[-1]
    for name in names:
        case = indexed.get(name)
        if case is None:
            errors.append(f"ERROR: {label}: missing {name}")
            continue
        finding = case.get("run_issue") or case.get("check_issue")
        if (
            case.get("csv") is not None
            or not isinstance(finding, dict)
            or not all(
                isinstance(finding.get(field), str) and finding[field]
                for field in ("phase", "condition", "rule", "spec_path")
            )
        ):
            errors.append(f"ERROR: {label}: {name} lacks a refusal contract")
        elif plain not in finding["spec_path"] and plain not in case.get("spec", ""):
            errors.append(f"ERROR: {label}: {name} never names {plain}")
    return errors


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


def check(operation, names):
    errors = []
    if not isinstance(names, list) or not names:
        return [f"ERROR: coverage.{operation}: no negative example"]
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
        paths = contract.get("spec_paths") or []
        body = spec.read_text(encoding="utf-8") if spec.is_file() else ""
        if (
            plain not in " ".join(paths)
            and plain not in body
            and not (spec.is_file() and names_merged(plain, spec, paths))
        ):
            errors.append(f"ERROR: coverage.{operation}: {name} never names {plain}")
    return errors


def declared_coverage():
    """Collect declarations without maintaining a second list of case names."""
    coverage = {}

    def add(operation, owner, name):
        row = coverage.setdefault(operation, {"benchmarks": [], "shared_cases": []})
        row[owner].append(name)

    for case in directories(EXAMPLES):
        for operation in load_metadata(case).get("covers", []):
            add(operation, "benchmarks", case.name)
    document = json.loads(SHARED.read_text(encoding="utf-8"))
    cases = document["families"]["original-specifications"]["temporal_arithmetic"]
    for case in cases:
        operations = case.get("covers", [])
        if not (
            isinstance(operations, list)
            and all(
                isinstance(operation, str) and operation for operation in operations
            )
            and len(set(operations)) == len(operations)
        ):
            raise ValueError(f"shared case {case.get('name')}: invalid covers")
        for operation in operations:
            add(operation, "shared_cases", case["name"])
    return coverage


def main():
    try:
        coverage = declared_coverage()
    except (OSError, ValueError, KeyError, TypeError, yaml.YAMLError) as error:
        print(f"ERROR: cannot read vocabulary coverage metadata: {error}")
        return 1
    operations = registered_operations()
    errors = []
    for operation in sorted(operations):
        if operation not in coverage:
            errors.append(f"ERROR: coverage.{operation}: unmapped operation")
            continue
        entry = coverage[operation]
        if entry["benchmarks"]:
            errors.extend(check(operation, entry["benchmarks"]))
        if entry["shared_cases"]:
            errors.extend(check_shared(operation, entry["shared_cases"]))
    for operation in sorted(set(coverage) - set(operations)):
        errors.append(f"ERROR: coverage.{operation}: unknown operation")
    for error in errors:
        print(error)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
