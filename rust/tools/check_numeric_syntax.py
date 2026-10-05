"""Check shared numeric grammar truth and deterministic reference comparisons.

Committed YAML supplies independent acceptance, shapes and vocabulary. Python
comparisons additionally pin exact ASTs, number text, diagnostics and metadata;
they never regenerate fixtures or establish evaluator/numerical qualification.
"""

import json
import subprocess
from pathlib import Path

from check_numeric_grammar import contract_cases as numeric_contract_cases
from check_numeric_grammar import differential_sources as numeric_sources
from check_numeric_grammar import reference_shape as numeric_shape
from yamaa.expressions.numeric import (
    NumericError,
    numeric_identifiers,
    parse_numeric,
)

import yaml

WORKSPACE = Path(__file__).resolve().parents[1]


def cases():
    """Reuse every independent committed numeric case and vocabulary expansion."""
    contract = yaml.safe_load(
        (WORKSPACE.parent / "yaml/grammar/numeric.yaml").read_text(encoding="utf-8")
    )
    return numeric_contract_cases(contract)


def compare_reference(text, outcome):
    """Pin full syntax or diagnostic context against the unchanged reference."""
    try:
        ast = parse_numeric(text)
    except NumericError as error:
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
            "identifiers": list(numeric_identifiers(ast)),
        }
    if outcome != expected:
        raise AssertionError(f"{text!r}: actual {outcome!r}, expected {expected!r}")


def main():
    """Replay both independent truth and reference samples through real transport."""
    contract = cases()
    sources = [case["text"] for case in contract] + numeric_sources()
    run = subprocess.run(
        [
            "cargo",
            "run",
            "--offline",
            "--quiet",
            "-p",
            "yamaa-adapters",
            "--example",
            "numeric_syntax_probe",
        ],
        cwd=WORKSPACE,
        input="".join(
            json.dumps({"protocol": "numeric-syntax/1", "expression": text}) + "\n"
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
        assert response["protocol"] == "numeric-syntax/1"
        assert actual["status"] == (
            "parsed" if case["parse"] == "accept" else "invalid"
        ), (case, actual)
        if case["parse"] == "reject":
            assert actual["condition"] == case["condition"], (case, actual)
        else:
            if "shape" in case:
                assert numeric_shape(actual["ast"]) == " ".join(
                    case["shape"].split()
                ), (
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
        f"Numeric syntax transport: {len(contract)} independent contract checks and {len(sources)} complete reference comparisons passed"
    )


if __name__ == "__main__":
    main()
