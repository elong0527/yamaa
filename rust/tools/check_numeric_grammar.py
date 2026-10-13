"""Replay independent numeric grammar truth without a reference parser.

Deterministic former differential inputs remain robustness observations; their
historical comparisons do not establish current semantic truth.
"""

import random
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parents[1]


def syntax_shape(node):
    """Render typed syntax in the contract's host-independent shape notation."""
    kind = node["kind"]
    if kind == "number":
        return f"({node['type']} {node['value']})"
    if kind == "null":
        return "null"
    if kind == "identifier":
        return f"(id {node['name']})"
    if kind == "unary":
        operator = "pos" if node["operator"] == "+" else "neg"
        return f"({operator} {syntax_shape(node['value'])})"
    if kind == "binary":
        return (
            f"({node['operator']} {syntax_shape(node['left'])} "
            f"{syntax_shape(node['right'])})"
        )
    if kind == "call":
        arguments = " ".join(syntax_shape(arg) for arg in node["arguments"])
        return f"(call {node['name']} {arguments})"
    raise AssertionError(f"unexpected syntax node: {node!r}")


def contract_cases(contract):
    """Check every committed case plus every vocabulary/arity/reserved-word entry."""
    cases = list(contract["cases"])
    for name, arity in contract["vocabulary"]["function"].items():
        minimum, maximum = arity["min_arguments"], arity["max_arguments"]
        counts = {0, minimum - 1, minimum, minimum + 1, 8}
        if maximum is not None:
            counts.add(maximum + 1)
        for count in sorted(counts):
            arguments = ", ".join(["A"] * count)
            accepted = count >= minimum and (maximum is None or count <= maximum)
            cases.append(
                {
                    "id": f"vocabulary-{name}-{count}",
                    "text": f"{name.lower()}({arguments})",
                    "parse": "accept" if accepted else "reject",
                    "condition": "prohibited_function",
                }
            )
    for word in contract["prohibited"]:
        cases.append(
            {
                "id": f"reserved-{word}",
                "text": f"A {word.lower()} B",
                "parse": "reject",
                "condition": "prohibited_construct",
            }
        )
        cases.append(
            {"id": f"qualified-{word}", "text": f"A.{word}", "parse": "accept"}
        )
    return cases


def differential_sources():
    """Exercise deterministic lexical/syntax conflicts and valid nested expressions."""
    rng = random.Random(1585)
    atoms = ["A", "a", "LB.X", "NULL", "0001", "1E-3", "1.0", "1e9999"]
    sources = list(atoms)
    for _ in range(300):
        left, right = rng.choice(sources[-40:]), rng.choice(atoms)
        sources.append(
            rng.choice([f"({left}) - {right}", f"ABS({left})", f"MOD({left}, {right})"])
        )
    tokens = atoms + [
        "+",
        "-",
        "*",
        "/",
        "(",
        ")",
        ",",
        "ROUND",
        "AND",
        ">",
        "'",
        "#",
        ".",
        "1e+",
        "A.",
        "--",
    ]
    for _ in range(2000):
        separator = rng.choice(["", " ", "\u2003", "\x1c"])
        sources.append(separator.join(rng.choices(tokens, k=rng.randrange(1, 10))))
    return sources


def main():
    """Replay independent numeric truth with the retained typed syntax fixture."""
    from check_numeric_syntax import main as check

    check()


if __name__ == "__main__":
    main()
