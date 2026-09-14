"""Command-line entry point for the shared reference comparator."""

from __future__ import annotations

import argparse
import os
import tempfile
from pathlib import Path

from conformance.comparison import compare_case
from conformance.schema import main as schema_main


def _write_atomic(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(dir=path.parent, prefix=f".{path.name}.")
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as output:
            output.write(text)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    except BaseException:
        Path(temporary).unlink(missing_ok=True)
        raise


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="python -m conformance")
    subparsers = parser.add_subparsers(dest="command", required=True)

    compare = subparsers.add_parser("compare")
    compare.add_argument("--run-root", type=Path, required=True)
    compare.add_argument("--expected-root", type=Path, required=True)
    compare.add_argument("--runtime", action="append", dest="runtimes")
    compare.add_argument("--summary", type=Path)

    schema = subparsers.add_parser("schema")
    schema.add_argument("action", choices=("check", "write"))

    args = parser.parse_args(argv)
    if args.command == "schema":
        return schema_main([f"--{args.action}"])

    runtimes = tuple(args.runtimes or ("r", "python"))
    summary = compare_case(args.run_root, args.expected_root, runtimes)
    if args.summary is not None:
        _write_atomic(args.summary, summary.model_dump_json(indent=2) + "\n")
    labels = ", ".join(summary.runtimes)
    print(f"{summary.status.upper()}: {summary.example} ({labels})")
    for failure in summary.failures:
        print(f"  - {failure}")
    return 0 if summary.status == "pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())
