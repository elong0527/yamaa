"""Replay committed predicate truth and explicit native syntax controls.

All former differential inputs remain robustness observations, not semantic truth.
"""

import random
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
        "A = -9223372036854775809",
        "A = +0009223372036854775808",
        "FALSE AND A IN (1, +9223372036854775808)",
        "'\U0001f600' = -9223372036854775809",
        "A BETWEEN 0 AND 9999999999999999999999999999999",
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
    """Replay explicit UTF-8/LF frames through the retained typed fixture."""
    from native_syntax_checks import replay

    return replay("predicate", sources)


def cases():
    """Retain every shared case, reserved identifier and independent call control."""
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
    return cases


def main():
    """Check independent contract truth without importing a reference parser."""
    contract = cases()
    sources = [case["text"] for case in contract] + samples()
    responses = probe(sources)
    for case, response in zip(contract, responses, strict=False):
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
    from native_syntax_checks import exact_controls

    controls = exact_controls("predicate")
    print(
        f"Predicate typed syntax: {len(contract)} independent contract checks, "
        f"{controls} complete authored controls; {len(sources)} robustness observations "
        "(historical reference comparisons are not rerun or claimed as semantic truth)"
    )


if __name__ == "__main__":
    main()
