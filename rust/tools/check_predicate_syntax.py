"""Replay committed R004 truth and supplemental reference syntax observations.

Expected shapes/acceptance come from the unchanged grammar. Reference comparisons
are additional evidence, not generated truth or regex/evaluator qualification.
"""

import json
import random
import subprocess
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parents[1]


def quote(value):
    """Render the grammar's single-quoted shape without host string escapes."""
    return "'" + value.replace("'", "''") + "'"


def operand(node):
    """Preserve literal spelling and the closed operand vocabulary."""
    if node["kind"] == "identifier":
        return f"(id {node['name']})"
    kind = node["type"]
    if kind is None:
        return "null"
    value = (
        quote(node["value"]) if kind in {"str", "date", "datetime"} else node["value"]
    )
    return f"({kind} {value})"


def shape(node):
    """Render portable syntax in the committed contract's notation."""
    kind = node["kind"]
    if kind in {"and", "or"}:
        return f"({kind} {shape(node['left'])} {shape(node['right'])})"
    if kind == "not":
        return f"(not {shape(node['value'])})"
    if kind == "boolean":
        return "true" if node["value"] else "false"
    if kind == "comparison":
        return f"({node['operator']} {operand(node['left'])} {operand(node['right'])})"
    if kind == "null_test":
        return f"({'is-not-null' if node['negated'] else 'is-null'} {operand(node['value'])})"
    if kind == "call":
        return (
            f"(str-contains {operand(node['source'])} (str {quote(node['pattern'])}))"
        )
    prefix = "not-" if node["negated"] else ""
    result = f"({prefix}{kind} {operand(node['value'])}"
    if kind == "in":
        result += " " + " ".join(operand(item) for item in node["values"])
    elif kind == "between":
        result += f" {operand(node['lower'])} {operand(node['upper'])}"
    elif kind == "like":
        result += " " + operand(node["pattern"])
        if node["escape"] is not None:
            result += f" (escape {quote(node['escape'])})"
    else:
        raise AssertionError(kind)
    return result + ")"


def samples():
    """Deterministic adversarial combinations; these do not redefine the grammar."""
    rng = random.Random(1720)
    atoms = [
        "TRUE",
        "A = 1",
        "A.B NOT IN (1, NULL, -2)",
        "A BETWEEN B AND C",
        "A LIKE 'x%'",
        "A IS NOT NULL",
        "str_contains(A, 'x')",
        "'\U0001f600' = 'a'",
        "A = DATE '2024-02-29'",
        "A = 1e9999",
    ]
    result = []
    for _ in range(1000):
        left, right = rng.choice(atoms), rng.choice(atoms)
        result.append(
            rng.choice(
                [
                    f"NOT ({left}) OR {right}",
                    f"{left} AND {right}",
                    f"({left})",
                    f"({left}",
                    f"{left} @",
                    f"{left} {right}",
                ]
            )
        )
    result += [
        "A = " + "9" * 5000,
        "A\u2003=\u001c1",
        "str_contains(A, '('",
        "A = '\0'",
        "A = DATETIME '2025-01-01T01:02:03'",
        "A = DATE '2025-02-30'",
        "A = DATETIME '2025-01-01T24:00'",
        "A = 'a''b'",
        "A = 1e+",
        "A = + 1",
        "A. = 1",
    ]
    return result


def probe(sources):
    """Replay UTF-8 JSON through Rust using only Python's standard library."""
    run = subprocess.run(
        [
            "cargo",
            "run",
            "--offline",
            "--quiet",
            "-p",
            "yamaa-adapters",
            "--example",
            "predicate_syntax_probe",
        ],
        cwd=WORKSPACE,
        input="".join(
            json.dumps({"protocol": "predicate-syntax/1", "expression": text}) + "\n"
            for text in sources
        ),
        text=True,
        encoding="utf-8",
        errors="strict",
        capture_output=True,
        check=True,
    )
    # JSON-lines frames use LF only; U+0085/U+2028/U+2029 may occur inside JSON strings.
    responses = [json.loads(line) for line in run.stdout.split("\n") if line]
    assert len(responses) == len(sources)
    return responses


def main():
    """Check independent contract truth first, then exact common-reference syntax."""
    from yamaa.expressions.predicates import (
        PredicateError,
        parse_predicate,
        predicate_identifiers,
    )

    import yaml

    grammar = yaml.safe_load(
        (WORKSPACE.parent / "yaml/grammar/predicate.yaml").read_text(encoding="utf-8")
    )
    cases = list(grammar["cases"])
    cases.append(
        {
            "text": "AEDECOD LIKE 'a' ESCAPE ''",
            "parse": "reject",
            "condition": "invalid_predicate",
        }
    )
    for name in grammar["reserved"]:
        cases.append(
            {"text": f"D.{name} = 1", "parse": "accept", "identifiers": [f"D.{name}"]}
        )
    # Hand-authored call truth supplements the shared vectors that predate R call support.
    for text, expected, names in [
        ("str_contains(A, 'x')", "(str-contains (id A) (str 'x'))", ["A"]),
        ("NOT STR_CONTAINS(NULL, '^$')", "(not (str-contains null (str '^$')))", []),
        (
            "str_contains = 'column'",
            "(= (id str_contains) (str 'column'))",
            ["str_contains"],
        ),
    ]:
        cases.append(
            {"text": text, "parse": "accept", "shape": expected, "identifiers": names}
        )
    sources = [case["text"] for case in cases] + samples()
    responses = probe(sources)
    for case, response in zip(cases, responses, strict=False):
        actual = response["outcome"]
        assert actual["status"] == (
            "parsed" if case["parse"] == "accept" else "invalid"
        ), (case, actual)
        if case["parse"] == "accept":
            if "shape" in case:
                assert shape(actual["ast"]) == case["shape"], (case, actual)
            assert sorted(actual["identifiers"]) == sorted(case["identifiers"]), (
                case,
                actual,
            )
        else:
            assert actual["condition"] == case["condition"], (case, actual)
    escape_requirements = 0
    for text, response in zip(sources, responses, strict=True):
        assert response["protocol"] == "predicate-syntax/1"
        actual = response["outcome"]
        try:
            ast = parse_predicate(text)
        except PredicateError as error:
            assert (
                actual["status"] == "invalid" and actual["condition"] == error.condition
            ), (text, actual)
            assert actual["position"] == {
                "byte": len(text[: error.position].encode()),
                "character": error.position,
            }, (text, actual, error.position)
            # These explicit ESCAPE cases own REQ-0191; Python currently reports REQ-0188.
            if text in {
                "AEDECOD LIKE 'a' ESCAPE ''",
                "AEDECOD LIKE 'a' ESCAPE '!!'",
                "AEDECOD LIKE 'HEAD!' ESCAPE '!'",
            }:
                assert (
                    actual["requirement"] == "REQ-0191"
                    and error.requirement == "REQ-0188"
                )
                escape_requirements += 1
            else:
                assert actual["requirement"] == error.requirement, (
                    text,
                    actual,
                    error.requirement,
                )
        else:
            assert actual == {
                "status": "parsed",
                "ast": ast,
                "identifiers": list(predicate_identifiers(ast)),
            }, (text, actual, ast)
    assert escape_requirements == 3
    print(
        f"Predicate syntax: {len(cases)} independent contract checks; {len(sources)} reference observations; three explicit ESCAPE requirement differences retained"
    )


if __name__ == "__main__":
    main()
