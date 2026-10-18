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

from tools.paths import ROOT

REPOSITORY = ROOT
CONTRACTS = {
    "expression": "formulas",
    "string-template": "text",
}
CONTEXTS = ("predicate", "compute", "aggregate")


def documents(root=REPOSITORY):
    for contract, rule in CONTRACTS.items():
        path = root / "yaml/grammar" / (contract + ".yaml")
        document = yaml12.parse_yaml(path.read_text(encoding="utf-8"))
        if document["contract"] != contract or document["rule"] != rule:
            raise ValueError("shared grammar contract identity differs: " + str(path))
        yield contract, document


def cases(root=REPOSITORY):
    """Yield every authored case, then the expression vocabulary boundaries."""
    docs = dict(documents(root))
    for contract, document in docs.items():
        for case in document["cases"]:
            yield contract, case
    for case in expansions(docs["expression"]):
        yield "expression", case


def expansions(document):
    """Expand the closed vocabularies into acceptance boundaries in each context.

    The typed core replay expands the same document the same way; no reference
    evaluator is imported to decide an outcome.
    """
    expanded = []

    def case(context, text, accepted, condition):
        expanded.append(
            {
                "id": f"{context}: {text}",
                "context": context,
                "text": text,
                "parse": "accept" if accepted else "reject",
                "condition": condition,
            }
        )

    for name, arity in document["vocabulary"]["function"].items():
        lower, upper = arity["min_arguments"], arity["max_arguments"]
        numeric = arity["result"] == "numeric"
        counts = {0, lower - 1, lower, lower + 1, 8}
        if upper is not None:
            counts.add(upper + 1)
        for count in sorted(counts):
            fits = count >= lower and (upper is None or count <= upper)

            def call(pattern):
                arguments = [
                    "'a'" if pattern and index > 0 else "A" for index in range(count)
                ]
                return f"{name.lower()}({', '.join(arguments)})"

            for context in CONTEXTS:
                if context == "predicate":
                    # A numeric call is a comparison operand; a truth-valued call
                    # is the predicate, and its pattern is a string literal.
                    text = f"{call(False)} = 1" if numeric else call(True)
                    case(context, text, fits, "prohibited_function")
                else:
                    case(context, call(False), fits and numeric, "prohibited_function")
    for word in document["reserved"]:
        literal = word == "NULL"
        truth = word in {"TRUE", "FALSE"}
        case("predicate", f"{word.lower()} = 1", literal, "invalid_expression")
        case("predicate", f"D.{word} = 1", True, "")
        for context in ("compute", "aggregate"):
            case(
                context,
                f"1 + {word.lower()}",
                literal,
                "prohibited_construct" if truth else "invalid_expression",
            )
            case(context, f"D.{word} + 1", True, "")
    production = next(p for p in document["productions"] if p["name"] == "compare")
    for operator in re.findall(r'"([^"]*)"', production["definition"]):
        case("predicate", f"A {operator} 1", True, "")
        for context in ("compute", "aggregate"):
            case(context, f"A {operator} 1", False, "prohibited_construct")
    for name, declaration in document["vocabulary"]["reducer"].items():
        star = "star" in declaration["argument"]
        for argument in ("A", "A+1", "ABS(A)", "D.*", "", "A,B", "SUM(A)"):
            text = f"{name.lower()}({argument})"
            syntax = argument in {"", "A,B"} or (argument == "D.*" and not star)

            def condition(admitted):
                return "invalid_expression" if syntax else admitted

            case(
                "aggregate",
                text,
                not syntax and argument != "SUM(A)",
                condition("nested_reduction"),
            )
            case("compute", text, False, condition("prohibited_function"))
            case("predicate", f"{text} = 1", False, condition("prohibited_function"))
    return expanded


def specification(contract, case):
    """Put the grammar into a real public API field; bindings are deliberately absent.

    Public acceptance checks assert that parser diagnostics are absent even when
    later name binding rejects a case. Exact dependencies and shapes are checked
    separately through the typed core replay.
    """
    # Every specification is pruned after composition (REQ-0638): no column
    # reads SRC, so declare it as the driver with base to keep it live.
    # Otherwise the unread input is discarded, the implicit driver is gone and
    # driver_unavailable replaces the grammar decision under test.
    spec = {
        "schema_version": "1.0",
        "domain": "GRAMMAR",
        "keys": ["ID"],
        "input": {"SRC": "input.csv"},
        "base": "SRC",
        "columns": [{"name": "ID", "type": "str", "derivation": {"literal": "one"}}],
        "output": {"path": "unwritten.csv", "columns": ["ID", "RESULT"]},
    }
    field = contract if contract == "string-template" else case["context"]
    column = {
        "name": "RESULT",
        "type": "str" if field in {"predicate", "string-template"} else "float",
    }
    if field == "predicate":
        column["derivation"] = {
            "case": [
                {"when": case["text"], "then": {"literal": "yes"}},
                {"otherwise": {"literal": "no"}},
            ]
        }
    elif field == "compute":
        column["derivation"] = {"compute": {"expr": case["text"]}}
    else:
        # A formula or template alone is written as its shorthand.
        key = {"aggregate": "aggregate", "string-template": "str_template"}[field]
        column["derivation"] = {key: case["text"]}
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
                "context",
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
                (
                    contract,
                    case.get("context", ""),
                    case["id"],
                    path,
                    case["parse"],
                    condition,
                    public_condition,
                )
            )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination.resolve())
