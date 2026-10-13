"""Independent controls and transport checks for the retained typed syntax fixtures.

This module neither parses nor evaluates expressions. Authored complete outcomes
pin obligations that public APIs intentionally do not expose. Random inputs are
checked only for complete, well-formed observations, never called semantic truth.
"""

import json
import subprocess
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parents[1]
FAMILIES = ("numeric", "aggregate", "predicate")


def replay(family, sources):
    """Invoke existing uninstalled typed fixtures with explicit UTF-8/LF framing."""
    if family not in FAMILIES:
        raise ValueError(f"unknown syntax family {family}")
    run = subprocess.run(
        [
            "cargo",
            "run",
            "--offline",
            "--locked",
            "--quiet",
            "-p",
            "yamaa-adapters",
            "--example",
            family + "_syntax_fixture",
        ],
        cwd=WORKSPACE,
        input="".join(json.dumps(text) + "\n" for text in sources),
        text=True,
        encoding="utf-8",
        errors="strict",
        capture_output=True,
        check=True,
    )
    responses = [json.loads(line) for line in run.stdout.split("\n") if line]
    if len(responses) != len(sources):
        raise AssertionError("incomplete typed syntax observations")
    for text, response in zip(sources, responses, strict=True):
        outcome = response["outcome"]
        if outcome["status"] == "parsed":
            assert isinstance(outcome["ast"], dict), (text, response)
            assert all(isinstance(name, str) for name in outcome["identifiers"])
            if family == "aggregate":
                for field in ("star_datasets", "ungrouped_identifiers"):
                    assert all(isinstance(name, str) for name in outcome[field])
        elif outcome["status"] in {"invalid", "resource_limit", "unsupported"}:
            position = outcome["position"]
            character = position["character"]
            assert 0 <= character <= len(text), (text, response)
            assert position["byte"] == len(text[:character].encode("utf-8")), (
                text,
                response,
            )
            if outcome["status"] == "invalid":
                assert isinstance(outcome["context"], dict)
                assert outcome["requirement"].startswith("REQ-")
        else:
            raise AssertionError((text, response))
    return responses


def assert_contract(case, actual, shape):
    """Compare only independently committed grammar truth."""
    assert actual["status"] == ("parsed" if case["parse"] == "accept" else "invalid"), (
        case,
        actual,
    )
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


def identifier(name, position=None):
    """Spell an authored identifier node; never derive it from candidate syntax."""
    return {
        "kind": "identifier",
        "name": name,
        **({"position": position} if position is not None else {}),
    }


def invalid(condition, requirement, byte, character, context, message=None):
    """Spell every field of an independently expected diagnostic."""
    return {
        "status": "invalid",
        "condition": condition,
        "requirement": requirement,
        "position": {"byte": byte, "character": character},
        "context": context,
        **({"message": message} if message is not None else {}),
    }


def controls(family):
    """Author literal ASTs, ordered metadata and owning diagnostic truth."""
    if family == "numeric":
        return [
            (
                "\u2003-(0001 + abs(B.x)) + B.x",
                {
                    "status": "parsed",
                    "identifiers": ["B.x"],
                    "ast": {
                        "kind": "binary",
                        "operator": "+",
                        "left": {
                            "kind": "unary",
                            "operator": "-",
                            "value": {
                                "kind": "binary",
                                "operator": "+",
                                "left": {
                                    "kind": "number",
                                    "type": "int",
                                    "value": "0001",
                                },
                                "right": {
                                    "kind": "call",
                                    "name": "ABS",
                                    "arguments": [identifier("B.x")],
                                },
                            },
                        },
                        "right": identifier("B.x"),
                    },
                },
            ),
            (
                "Z + A + Z",
                {
                    "status": "parsed",
                    "identifiers": ["Z", "A"],
                    "ast": {
                        "kind": "binary",
                        "operator": "+",
                        "left": {
                            "kind": "binary",
                            "operator": "+",
                            "left": identifier("Z"),
                            "right": identifier("A"),
                        },
                        "right": identifier("Z"),
                    },
                },
            ),
            (
                "01.00E+2",
                {
                    "status": "parsed",
                    "identifiers": [],
                    "ast": {"kind": "number", "type": "float", "value": "01.00E+2"},
                },
            ),
            (
                "\u2003\u00a0A + #",
                invalid("invalid_numeric_expression", "REQ-0439", 9, 6, {}),
            ),
            (
                "\u2003aNd",
                invalid(
                    "prohibited_construct", "REQ-0441", 3, 1, {"construct": "boolean"}
                ),
            ),
            (
                "abS(1, 2)",
                invalid(
                    "prohibited_function",
                    "REQ-0440",
                    0,
                    0,
                    {"function": "abS", "argument_count": 2},
                ),
            ),
            ("foo(1", invalid("invalid_numeric_expression", "REQ-0439", 5, 5, {})),
            (
                "foo(ABS())",
                invalid(
                    "prohibited_function",
                    "REQ-0440",
                    4,
                    4,
                    {"function": "ABS", "argument_count": 0},
                ),
            ),
        ]
    if family == "aggregate":
        summation = {
            "kind": "reduction",
            "name": "SUM",
            "text": "sUm( D.Z + 01.00E+2 )",
            "argument": {
                "kind": "binary",
                "operator": "+",
                "left": identifier("D.Z"),
                "right": {"kind": "number", "type": "float", "value": "01.00E+2"},
            },
        }
        count = {
            "kind": "reduction",
            "name": "COUNT",
            "text": "COUNT(E.*)",
            "argument": {"kind": "star", "dataset": "E"},
        }
        return [
            (
                "sUm( D.Z + 01.00E+2 ) + COUNT(E.*) + A + D.Z",
                {
                    "status": "parsed",
                    "identifiers": ["D.Z", "A"],
                    "star_datasets": ["E"],
                    "ungrouped_identifiers": ["A", "D.Z"],
                    "ast": {
                        "kind": "binary",
                        "operator": "+",
                        "left": {
                            "kind": "binary",
                            "operator": "+",
                            "left": {
                                "kind": "binary",
                                "operator": "+",
                                "left": summation,
                                "right": count,
                            },
                            "right": identifier("A"),
                        },
                        "right": identifier("D.Z"),
                    },
                },
            ),
            (
                "COUNT(Z.*) + COUNT(A.*) + COUNT(Z.*)",
                {
                    "status": "parsed",
                    "identifiers": [],
                    "star_datasets": ["Z", "A"],
                    "ungrouped_identifiers": [],
                    "ast": {
                        "kind": "binary",
                        "operator": "+",
                        "left": {
                            "kind": "binary",
                            "operator": "+",
                            "left": {
                                "kind": "reduction",
                                "name": "COUNT",
                                "text": "COUNT(Z.*)",
                                "argument": {"kind": "star", "dataset": "Z"},
                            },
                            "right": {
                                "kind": "reduction",
                                "name": "COUNT",
                                "text": "COUNT(A.*)",
                                "argument": {"kind": "star", "dataset": "A"},
                            },
                        },
                        "right": {
                            "kind": "reduction",
                            "name": "COUNT",
                            "text": "COUNT(Z.*)",
                            "argument": {"kind": "star", "dataset": "Z"},
                        },
                    },
                },
            ),
            (
                "\u2003SUM(COALESCE(MIN(A), MAX(B)))",
                invalid(
                    "nested_reduction",
                    "REQ-0502",
                    3,
                    1,
                    {"outer": "SUM", "inner": "MIN"},
                ),
            ),
            (
                "SUM(MIN(A)",
                invalid("invalid_aggregate_expression", "REQ-0499", 10, 10, {}),
            ),
            (
                "SUM(MIN(A)) > X",
                invalid(
                    "prohibited_construct",
                    "REQ-0512",
                    12,
                    12,
                    {"construct": "comparison"},
                ),
            ),
            (
                "abS(1,2)",
                invalid(
                    "prohibited_function",
                    "REQ-0500",
                    0,
                    0,
                    {"function": "abS", "argument_count": 2},
                ),
            ),
        ]
    if family == "predicate":
        result = [
            (
                "Z = A AND Z IS NULL",
                {
                    "status": "parsed",
                    "identifiers": ["Z", "A"],
                    "ast": {
                        "kind": "and",
                        "left": {
                            "kind": "comparison",
                            "operator": "=",
                            "left": identifier("Z", 0),
                            "right": identifier("A", 4),
                        },
                        "right": {
                            "kind": "null_test",
                            "value": identifier("Z", 10),
                            "negated": False,
                        },
                    },
                },
            ),
            (
                "NOT STR_CONTAINS(NULL, '^$')",
                {
                    "status": "parsed",
                    "identifiers": [],
                    "ast": {
                        "kind": "not",
                        "value": {
                            "kind": "call",
                            "function": "str_contains",
                            "source": {
                                "kind": "literal",
                                "type": None,
                                "value": None,
                                "position": 17,
                            },
                            "pattern": "^$",
                            "position": 4,
                        },
                    },
                },
            ),
        ]
        for text, site, message in [
            (
                "AEDECOD LIKE 'a' ESCAPE ''",
                24,
                "ESCAPE requires exactly one code point",
            ),
            (
                "AEDECOD LIKE 'a' ESCAPE '!!'",
                24,
                "ESCAPE requires exactly one code point",
            ),
            (
                "AEDECOD LIKE 'HEAD!' ESCAPE '!'",
                13,
                "LIKE pattern has a dangling escape",
            ),
        ]:
            result.append(
                (
                    text,
                    invalid("invalid_predicate", "REQ-0191", site, site, {}, message),
                )
            )
        for text, byte, character in [
            ("A = " + "9" * 5000, 4, 4),
            ("A = -9223372036854775809", 4, 4),
            ("A = +0009223372036854775808", 4, 4),
            ("FALSE AND A IN (1, +9223372036854775808)", 19, 19),
            ("'\U0001f600' = -9223372036854775809", 9, 6),
            ("A BETWEEN 0 AND 9999999999999999999999999999999", 16, 16),
        ]:
            result.append(
                (
                    text,
                    invalid(
                        "integer_overflow",
                        "REQ-0014",
                        byte,
                        character,
                        {},
                        "integer literal is outside the signed 64-bit range",
                    ),
                )
            )
        return result
    raise ValueError(family)


def exact_controls(family):
    """Compare complete observations to independent controls, with no normalization."""
    expected = controls(family)
    actual = replay(family, [text for text, _ in expected])
    for (text, truth), response in zip(expected, actual, strict=True):
        assert response == {"outcome": truth}, (text, truth, response)
    return len(expected)
