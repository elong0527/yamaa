#!/usr/bin/env python3
"""Check that example columns are declared after their dependencies."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


NAME = re.compile(r"^  - name: ([A-Za-z_][A-Za-z0-9_]*)\s*$", re.MULTILINE)
TOKEN = re.compile(r"\b[A-Za-z_][A-Za-z0-9_]*\b")


def column_blocks(text: str) -> list[tuple[str, str]]:
    """Return column names and their YAML blocks without needing a YAML package."""
    start = re.search(r"^columns:\s*$", text, re.MULTILINE)
    if not start:
        return []
    end = re.search(r"^[A-Za-z_][A-Za-z0-9_]*:\s*", text[start.end() :], re.MULTILINE)
    body_end = start.end() + (end.start() if end else len(text[start.end() :]))
    body = text[start.end() : body_end]
    matches = list(NAME.finditer(body))
    return [
        (match.group(1), body[match.end() : matches[index + 1].start()]
         if index + 1 < len(matches) else body[match.end() :])
        for index, match in enumerate(matches)
    ]


def dependencies(block: str, declared: set[str]) -> set[str]:
    """Collect declared column identifiers from the derivation portion of a block."""
    derivation = re.search(r"^    derivation:\s*$", block, re.MULTILINE)
    if not derivation:
        return set()
    value = block[derivation.end() :]
    # These mapping_from fields name columns in the external lookup dataset.
    value = re.sub(
        r"^        (?:dataset|key|value):.*$", "", value, flags=re.MULTILINE
    )
    # A qualified identifier belongs to a source dataset/lookup, not the output.
    unqualified = re.sub(
        r"\b[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_.]*\b", "", value
    )
    return set(TOKEN.findall(unqualified)) & declared


def expected_cycle(spec: Path) -> bool:
    error = spec.parent / "expected" / "error.yaml"
    return error.exists() and re.search(
        r"^condition:\s*dependency_cycle\s*$", error.read_text(), re.MULTILINE
    ) is not None


def check(spec: Path) -> list[str]:
    text = spec.read_text()
    blocks = column_blocks(text)
    names = [name for name, _ in blocks]
    declared = set(names)
    positions = {name: index for index, name in enumerate(names)}
    problems: list[str] = []
    output_match = re.search(r"^output:\n  columns: \[([^]]*)\]\s*$", text, re.MULTILINE)
    expected_output = [
        name for name, block in blocks
        if not re.search(r"^    output:\s*false\s*$", block, re.MULTILINE)
    ]
    if not output_match:
        problems.append("missing output.columns")
    else:
        output = [item.strip() for item in output_match.group(1).split(",") if item.strip()]
        if len(output) != len(set(output)):
            problems.append("output.columns contains a duplicate column")
        if set(output) != set(expected_output):
            problems.append("output.columns must list every output column exactly once")
    contract_problems = list(problems)
    graph: dict[str, set[str]] = {}
    for name, block in blocks:
        graph[name] = dependencies(block, declared) - {name}
        for dependency in sorted(graph[name]):
            if positions[dependency] > positions[name]:
                problems.append(f"{name} references later column {dependency}")

    def find_cycle() -> list[str]:
        visiting: list[str] = []
        visited: set[str] = set()
        def visit(node: str) -> list[str]:
            if node in visiting:
                start = visiting.index(node)
                return visiting[start:] + [node]
            if node in visited:
                return []
            visiting.append(node)
            for dependency in graph[node]:
                cycle = visit(dependency)
                if cycle:
                    return cycle
            visiting.pop()
            visited.add(node)
            return []
        for node in names:
            cycle = visit(node)
            if cycle:
                return cycle
        return []

    cycle = find_cycle()
    if cycle and expected_cycle(spec):
        return contract_problems
    if cycle:
        problems.append(f"dependency cycle: {' -> '.join(cycle)}")
    if expected_cycle(spec) and not cycle:
        problems.append("expected dependency_cycle example contains no detectable cycle")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("paths", nargs="*", type=Path)
    args = parser.parse_args()
    paths = args.paths or sorted(Path("yaml/examples").glob("*/spec.yaml"))
    failed = False
    for path in paths:
        for problem in check(path):
            failed = True
            print(f"{path}: {problem}")
    if failed:
        return 1
    print(f"Checked dependency order in {len(paths)} example specifications.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
