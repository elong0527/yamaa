#!/usr/bin/env python3
"""Move positive runners onto the public API only after both hosts qualify them."""

from __future__ import annotations

import argparse
import ast
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def qualified_examples(ratchet: Path) -> set[str]:
    data = json.loads(ratchet.read_text(encoding="utf-8"))
    if data.get("protocol") != "public-conformance-ratchet/1":
        raise ValueError("expected the committed public conformance ratchet")
    hosts = [data.get(host) for host in ("python", "r")]
    if any(
        not isinstance(cases, list) or any(not isinstance(case, str) for case in cases)
        for cases in hosts
    ):
        raise ValueError("both host ratchets must list benchmark names")
    return set(hosts[0]) & set(hosts[1])


def rewrite_runner(path: Path) -> str:
    source = path.read_text(encoding="utf-8")
    lines = source.splitlines(keepends=True)
    starts = [0]
    for line in lines:
        starts.append(starts[-1] + len(line))

    def offset(line: int, column: int) -> int:
        # Python's AST columns are UTF-8 byte positions.
        prefix = lines[line - 1].encode("utf-8")[:column].decode("utf-8")
        return starts[line - 1] + len(prefix)

    edits = []
    calls = 0
    for node in ast.walk(ast.parse(source)):
        if not (
            isinstance(node, ast.Call)
            and isinstance(node.func, ast.Attribute)
            and isinstance(node.func.value, ast.Name)
            and node.func.value.id == "yamaa"
            and node.func.attr in {"yamaa_domain", "domain"}
        ):
            continue
        calls += 1
        if (
            len(node.args) != 1
            or not isinstance(node.args[0], ast.Constant)
            or not isinstance(node.args[0].value, str)
            or any(keyword.arg != "environment" for keyword in node.keywords)
        ):
            raise ValueError(f"unsupported runner arguments: {path}")
        if node.func.attr == "yamaa_domain":
            end = offset(node.func.end_lineno, node.func.end_col_offset)
            edits.append((end - len("yamaa_domain"), end, "domain"))
        # These files declare host functions. Root environment.yaml files
        # used only by Define do not change a qualified domain invocation.
        if not node.keywords and (path.parent / "python/environment.yaml").is_file():
            if node.lineno != node.end_lineno:
                raise ValueError(f"environment insertion needs a one-line call: {path}")
            end = offset(node.end_lineno, node.end_col_offset) - 1
            edits.append((end, end, ', environment="python/environment.yaml"'))
    if not calls:
        raise ValueError(f"no domain invocation: {path}")
    for start, end, replacement in sorted(edits, reverse=True):
        source = source[:start] + replacement + source[end:]
    return source


def migrate(fixtures: Path, ratchet: Path, *, write: bool = False) -> list[Path]:
    qualified = qualified_examples(ratchet)
    changes = []
    # Prepare every edit before writing, so a malformed runner refuses the batch.
    for path in sorted(fixtures.glob("*/run.py")):
        if (
            path.parent.name not in qualified
            or (path.parent / "expected/error.yaml").exists()
        ):
            continue
        updated = rewrite_runner(path)
        if updated != path.read_text(encoding="utf-8"):
            changes.append((path, updated))
    if write:
        for path, updated in changes:
            path.write_text(updated, encoding="utf-8")
    return [path for path, _ in changes]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", type=Path, default=ROOT / "benchmarks")
    parser.add_argument(
        "--ratchet",
        type=Path,
        default=ROOT / "rust/qualification/public-conformance.json",
    )
    parser.add_argument(
        "--write", action="store_true", help="apply the qualified batch"
    )
    args = parser.parse_args()
    changed = migrate(args.fixtures, args.ratchet, write=args.write)
    for path in changed:
        print(path.relative_to(args.fixtures))
    print(
        f"{'Updated' if args.write else 'Would update'} {len(changed)} qualified runners"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
