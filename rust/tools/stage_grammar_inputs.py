"""Stage authored shared grammar vectors as public specification checks.

Expected decisions stay in yaml/grammar; this staging step never asks an
implementation to generate truth. JSON is used as a YAML 1.2 document form.
"""

import argparse
import csv
import json
import re
import shutil
from pathlib import Path

import yaml12

REPOSITORY = Path(__file__).resolve().parents[2]
CONTRACTS = {
    "aggregate": "operations/aggregation",
    "numeric": "operations/computation",
    "predicate": "operations/predicates",
    "string-template": "operations/text",
}


def documents(root=REPOSITORY):
    for contract, rule in CONTRACTS.items():
        path = root / "yaml/grammar" / (contract + ".yaml")
        document = yaml12.parse_yaml(path.read_text(encoding="utf-8"))
        if document["contract"] != contract or document["rule"] != rule:
            raise ValueError("shared grammar contract identity differs: " + str(path))
        yield contract, document


def cases(root=REPOSITORY):
    """Expand vocabulary boundaries without importing a reference evaluator."""
    docs = dict(documents(root))
    for contract, document in docs.items():
        for case in document["cases"]:
            yield contract, case
        if contract in {"numeric", "aggregate"}:
            for name, arity in docs["numeric"]["vocabulary"]["function"].items():
                lower, upper = arity["min_arguments"], arity["max_arguments"]
                counts = {0, lower - 1, lower, lower + 1, 8}
                if upper is not None:
                    counts.add(upper + 1)
                for count in sorted(counts):
                    yield (
                        contract,
                        {
                            "id": f"arity-{name}-{count}",
                            "text": f"{name.lower()}({', '.join(['A'] * count)})",
                            "parse": "accept"
                            if count >= lower and (upper is None or count <= upper)
                            else "reject",
                            "condition": "prohibited_function",
                        },
                    )
            for word in docs["numeric"]["prohibited"]:
                yield (
                    contract,
                    {
                        "id": "prohibited-" + word,
                        "text": "A " + word.lower() + " B",
                        "parse": "reject",
                        "condition": "prohibited_construct",
                    },
                )
                yield (
                    contract,
                    {"id": "qualified-" + word, "text": "A." + word, "parse": "accept"},
                )
        if contract == "aggregate":
            for name, declaration in document["vocabulary"]["reducer"].items():
                for argument in ("A", "A+1", "ABS(A)", "D.*", "", "A,B", "SUM(A)"):
                    accepted = argument in {"A", "A+1", "ABS(A)"} or (
                        argument == "D.*" and "star" in declaration["argument"]
                    )
                    yield (
                        contract,
                        {
                            "id": f"reducer-{name}-{argument}",
                            "text": f"{name.lower()}({argument})",
                            "parse": "accept" if accepted else "reject",
                            "condition": "nested_reduction"
                            if argument == "SUM(A)"
                            else "invalid_aggregate_expression",
                        },
                    )
        if contract == "predicate":
            for word in document["reserved"]:
                yield (
                    contract,
                    {
                        "id": "reserved-" + word,
                        "text": word.lower() + " = 1",
                        "parse": "accept" if word == "NULL" else "reject",
                        "condition": "invalid_predicate",
                        "identifiers": [],
                    },
                )
                yield (
                    contract,
                    {
                        "id": "qualified-" + word,
                        "text": "D." + word + " = 1",
                        "parse": "accept",
                    },
                )
            production = next(
                p for p in document["productions"] if p["name"] == "compare"
            )
            for operator in re.findall(r'"([^"]*)"', production["definition"]):
                yield (
                    contract,
                    {
                        "id": "comparison-" + operator,
                        "text": "A " + operator + " 1",
                        "parse": "accept",
                    },
                )


def specification(contract, case):
    """Put the grammar into a real public API field; bindings are deliberately absent.

    Public acceptance checks assert that parser diagnostics are absent even when
    later name binding rejects a case. Exact dependencies and shapes are checked
    separately through the typed core replay.
    """
    spec = {
        "schema_version": "1.0",
        "domain": "GRAMMAR",
        "keys": ["ID"],
        "input": {"SRC": "input.csv"},
        "columns": [{"name": "ID", "type": "str", "derivation": {"literal": "one"}}],
        "output": {"path": "unwritten.csv", "columns": ["ID", "RESULT"]},
    }
    column = {
        "name": "RESULT",
        "type": "str" if contract in {"predicate", "string-template"} else "float",
    }
    if contract == "predicate":
        column["derivation"] = {
            "case": [
                {"when": case["text"], "then": {"literal": "yes"}},
                {"otherwise": {"literal": "no"}},
            ]
        }
    else:
        key, field = {
            "numeric": ("compute", "expr"),
            "aggregate": ("aggregate", "expr"),
            "string-template": ("str_template", "template"),
        }[contract]
        column["derivation"] = {key: {field: case["text"]}}
    spec["columns"].append(column)
    return spec


def stage(destination, root=REPOSITORY):
    destination.mkdir(parents=True, exist_ok=False)
    (destination / "input.csv").write_text("ID\none\n", encoding="utf-8")
    originals = destination / "grammar"
    originals.mkdir()
    for contract in CONTRACTS:
        shutil.copy2(root / "yaml/grammar" / (contract + ".yaml"), originals)
    with (destination / "cases.tsv").open("w", encoding="utf-8", newline="") as stream:
        writer = csv.writer(stream, delimiter="\t", lineterminator="\n")
        writer.writerow(
            (
                "contract",
                "id",
                "specification",
                "parse",
                "condition",
                "public_condition",
            )
        )
        for index, (contract, case) in enumerate(cases(root)):
            path = f"case-{index:04d}.yaml"
            (destination / path).write_text(
                json.dumps(specification(contract, case), ensure_ascii=False) + "\n",
                encoding="utf-8",
            )
            condition = case.get("condition", "")
            # Empty expression fields fail the public schema's REQ-0287 before parsing.
            public_condition = (
                "minimum_length"
                if not case["text"] and case["parse"] == "reject"
                else condition
            )
            writer.writerow(
                (contract, case["id"], path, case["parse"], condition, public_condition)
            )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination.resolve())
