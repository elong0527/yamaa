"""Check shared aggregate grammar truth and deterministic reference comparisons.

Committed YAML supplies independent acceptance, shapes and vocabulary. Python
comparisons additionally pin exact ASTs, reducer text, diagnostics and metadata;
they never regenerate fixtures or establish evaluator/numerical qualification.
"""

import json
import random
import subprocess
from pathlib import Path

from check_numeric_grammar import contract_cases as numeric_contract_cases
from check_numeric_grammar import differential_sources as numeric_sources
from check_numeric_grammar import reference_shape as numeric_shape
from yamaa.expressions.aggregate import (
    AggregateError,
    aggregate_identifiers,
    aggregate_star_datasets,
    parse_aggregate,
    ungrouped_identifiers,
)

import yaml

WORKSPACE = Path(__file__).resolve().parents[1]


def shape(node):
    """Render the committed grammar's reducer and imported arithmetic notation."""
    kind = node["kind"]
    if kind == "reduction":
        return f"(reduce {node['name']} {shape(node['argument'])})"
    if kind == "star":
        return f"(star {node['dataset']})"
    if kind == "unary":
        return f"({'pos' if node['operator'] == '+' else 'neg'} {shape(node['value'])})"
    if kind == "binary":
        return f"({node['operator']} {shape(node['left'])} {shape(node['right'])})"
    if kind == "call":
        return (
            f"(call {node['name']} {' '.join(shape(arg) for arg in node['arguments'])})"
        )
    return numeric_shape(node)


def cases():
    """Load independent contract cases and expand each declared reducer form."""
    grammar = WORKSPACE.parent / "yaml/grammar"
    aggregate = yaml.safe_load((grammar / "aggregate.yaml").read_text(encoding="utf-8"))
    numeric = yaml.safe_load((grammar / "numeric.yaml").read_text(encoding="utf-8"))
    result = list(aggregate["cases"])
    for case in numeric_contract_cases(numeric)[len(numeric["cases"]) :]:
        # The imported grammar's ordinary numeric expressions remain valid here.
        result.append(
            {
                **case,
                "condition": case.get("condition", "").replace("numeric", "aggregate"),
            }
        )
    for name, declaration in aggregate["vocabulary"]["reducer"].items():
        for argument in ["A", "A+1", "ABS(A)", "D.*", "", "A,B", "SUM(A)"]:
            accepted = argument in {"A", "A+1", "ABS(A)"} or (
                argument == "D.*" and "star" in declaration["argument"]
            )
            result.append(
                {
                    "id": f"reducer-{name}-{argument}",
                    "text": f"{name.lower()}({argument})",
                    "parse": "accept" if accepted else "reject",
                    "condition": "nested_reduction"
                    if argument == "SUM(A)"
                    else "invalid_aggregate_expression",
                }
            )
    return result


def samples():
    """Exercise nested failures and written contexts with a reproducible seed."""
    rng = random.Random(1707)
    result = numeric_sources()
    atoms = [
        "SUM(A)",
        "COUNT(D.*)",
        "MIN(D.X)",
        "ONLY(NULL)",
        "SUM(ABS(B))",
        "A",
        "X",
        "1e9999",
    ]
    for _ in range(700):
        left, right = rng.choice(atoms), rng.choice(atoms)
        result.append(
            rng.choice(
                [
                    f"{left} + {right}",
                    f"SUM({left} + {right})",
                    f"COALESCE({left},{right})",
                    f"{left} > {right}",
                    f"COUNT({left}",
                    f"\u2003-{left} / {right}",
                ]
            )
        )
    result.extend(
        [
            "SUM(COALESCE(MIN(A), MAX(B)))",
            "SUM(MAX(MIN(A)))",
            "COUNT((D.*))",
            "COUNT(TRUE.*)",
            "COUNT(D .*)",
            "SUM(" + "9" * 5000 + ")",
        ]
    )
    return result


def compare_reference(text, outcome):
    """Pin full syntax or diagnostic context against the unchanged reference."""
    try:
        ast = parse_aggregate(text)
    except AggregateError as error:
        expected = {
            "status": "invalid",
            "condition": error.condition,
            "requirement": error.requirement,
            "position": {
                "byte": len(text[: error.position].encode()),
                "character": error.position,
            },
            "context": error.context,
        }
    else:
        expected = {
            "status": "parsed",
            "ast": ast,
            "identifiers": list(aggregate_identifiers(ast)),
            "star_datasets": list(aggregate_star_datasets(ast)),
            "ungrouped_identifiers": list(ungrouped_identifiers(ast)),
        }
    if outcome != expected:
        raise AssertionError(f"{text!r}: actual {outcome!r}, expected {expected!r}")


def main():
    """Replay both independent truth and reference samples through real transport."""
    contract = cases()
    sources = [case["text"] for case in contract] + samples()
    run = subprocess.run(
        [
            "cargo",
            "run",
            "--offline",
            "--quiet",
            "-p",
            "yamaa-adapters",
            "--example",
            "aggregate_grammar_probe",
        ],
        cwd=WORKSPACE,
        input="".join(
            json.dumps({"protocol": "aggregate-syntax/1", "expression": text}) + "\n"
            for text in sources
        ),
        text=True,
        capture_output=True,
        check=True,
    )
    responses = [json.loads(line) for line in run.stdout.splitlines()]
    assert len(responses) == len(sources)
    for case, response in zip(contract, responses, strict=False):
        actual = response["outcome"]
        assert response["protocol"] == "aggregate-syntax/1"
        assert actual["status"] == (
            "parsed" if case["parse"] == "accept" else "invalid"
        ), (case, actual)
        if case["parse"] == "reject":
            assert actual["condition"] == case["condition"], (case, actual)
        else:
            if "shape" in case:
                assert shape(actual["ast"]) == " ".join(case["shape"].split()), (
                    case,
                    actual,
                )
            if "identifiers" in case:
                assert sorted(actual["identifiers"]) == sorted(case["identifiers"]), (
                    case,
                    actual,
                )
    for text, response in zip(sources, responses, strict=True):
        compare_reference(text, response["outcome"])
    print(
        f"Aggregate grammar: {len(contract)} independent contract checks and {len(sources)} complete reference comparisons passed"
    )


if __name__ == "__main__":
    main()
