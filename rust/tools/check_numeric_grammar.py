"""Replay authoritative numeric grammar cases and compare reference diagnostics.

Expected syntax, shape, identifiers and vocabulary come from committed YAML,
never generated fixtures. A separate deterministic differential sample compares
positions and precedence against the current Python parser, without evaluating.
Run with the locked Python project environment (PyYAML and yamaa are required).
"""

import random
import subprocess
from pathlib import Path

from yamaa.expressions.numeric import NumericError, numeric_identifiers, parse_numeric

import yaml

WORKSPACE = Path(__file__).resolve().parents[1]


def reference_shape(node):
    """Render Python syntax in the contract's host-independent shape notation."""
    kind = node["kind"]
    if kind == "number":
        return f"({node['type']} {node['value']})"
    if kind == "null":
        return "null"
    if kind == "identifier":
        return f"(id {node['name']})"
    if kind == "unary":
        operator = "pos" if node["operator"] == "+" else "neg"
        return f"({operator} {reference_shape(node['value'])})"
    if kind == "binary":
        return (
            f"({node['operator']} {reference_shape(node['left'])} "
            f"{reference_shape(node['right'])})"
        )
    if kind == "call":
        arguments = " ".join(reference_shape(arg) for arg in node["arguments"])
        return f"(call {node['name']} {arguments})"
    raise AssertionError(f"unexpected reference node: {node!r}")


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


def run_probe(sources):
    """Build once and replay all sources through the Rust line protocol."""
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--offline",
            "--quiet",
            "-p",
            "yamaa-core",
            "--example",
            "numeric_grammar_probe",
        ],
        cwd=WORKSPACE,
        input="".join(text.encode().hex() + "\n" for text in sources),
        capture_output=True,
        text=True,
        check=True,
    )
    rows = [line.split("\t") for line in result.stdout.splitlines()]
    if len(rows) != len(sources):
        raise AssertionError(
            f"incomplete probe: {len(rows)} for {len(sources)} sources"
        )
    return rows


def decoded(value):
    """Decode a hex UTF-8 protocol field."""
    return bytes.fromhex(value).decode()


def compare_reference(text, actual):
    """Compare exact shapes, identifier order, and zero-based error coordinates."""
    try:
        node = parse_numeric(text)
    except NumericError as error:
        expected = [
            "reject",
            error.condition,
            error.requirement,
            str(len(text[: error.position].encode())),
            str(error.position),
        ]
    else:
        expected = [
            "accept",
            reference_shape(node).encode().hex(),
            ",".join(numeric_identifiers(node)).encode().hex(),
        ]
    if actual != expected:
        raise AssertionError(f"{text!r}: Rust {actual!r}, Python {expected!r}")


def main():
    """Fail CI when the core differs from shared truth or reference diagnostics."""
    contract = yaml.safe_load(
        (WORKSPACE.parent / "yaml/grammar/numeric.yaml").read_text()
    )
    cases = contract_cases(contract)
    sources = [case["text"] for case in cases] + differential_sources()
    actual = run_probe(sources)
    for case, row in zip(cases, actual, strict=False):
        if row[0] != case["parse"]:
            raise AssertionError(f"{case['id']}: {row!r}")
        if row[0] == "reject":
            if row[1] != case["condition"]:
                raise AssertionError(f"{case['id']}: {row!r}")
        else:
            if "shape" in case and decoded(row[1]) != " ".join(case["shape"].split()):
                raise AssertionError(f"{case['id']}: shape {decoded(row[1])!r}")
            if "identifiers" in case:
                names = decoded(row[2]).split(",") if row[2] else []
                if sorted(names) != sorted(case["identifiers"]):
                    raise AssertionError(f"{case['id']}: identifiers {names!r}")
    for text, row in zip(sources, actual, strict=True):
        compare_reference(text, row)
    print(
        f"Numeric grammar: {len(contract['cases'])} shared cases, {len(cases)} contract checks, {len(sources)} reference comparisons passed"
    )


if __name__ == "__main__":
    main()
