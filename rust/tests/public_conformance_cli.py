"""Run the benchmark corpus through an installed `yamaa` command-line binary.

The command line's public conformance runner (#3060). It reads the plan that
`python -m tools.conformance.public_conformance plan` writes, stages each
benchmark without its expected files, runs the binary with `--format json` for
each call the plan gives the command line, and records the exit status,
standard error and the JSON document of each run: its `status`, its issues and
the `paths` its save wrote, in publication order. As the Python and R runners
do (#3312), the record keeps those paths relative to the case's root, so it
reads the same on every machine, container and operating system; the document
stands in for the standard output it was read from, whose paths are absolute.
Run it with python -I: it needs the standard library alone, and the comparator
stays the only reader of expected truth.

The plan excludes, for the command line, each benchmark whose project supplies
project functions: the binary has no project-function implementation, so a
specification that calls one fails before it reads data (#3030 section 2.3).
Each excluded benchmark still gets a record that names the reason.

The binary runs `yamaa run PROJECT --spec NAME`, the plan's call for a
benchmark with a project. It still refuses `yamaa run SPEC` as not
implemented (D96), until the facade reads a PATH's `type:` header; such a run
is recorded as `not_implemented`. Any other run must exit with status 0 or 1
and print one JSON document, or its record is an error. The comparator
judges no command-line record yet.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
from pathlib import Path

PROTOCOL = "public-conformance/1"
# What the binary writes to standard error, with exit status 2, for a command
# it does not implement yet.
NOT_IMPLEMENTED = " is not implemented yet\n"
# Seconds one run may take before it is an error.
TIMEOUT = 600


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )


def invoke(binary, arguments, cwd):
    """Run the binary once: its exit status and both streams as text."""
    result = subprocess.run(
        [binary, *arguments],
        cwd=cwd,
        capture_output=True,
        timeout=TIMEOUT,
        check=False,
    )
    return {
        "exit_status": result.returncode,
        "stdout": result.stdout.decode("utf-8", "replace"),
        "stderr": result.stderr.decode("utf-8", "replace"),
    }


def observe(example, fixtures, work, binary):
    """The record of one planned benchmark."""
    record = {"example": example["name"], "excluded": None, "calls": [], "error": None}
    planned = example["cli"]
    if planned["excluded"]:
        record["excluded"] = planned["excluded"]
        return record
    case = work / example["name"]
    shutil.copytree(
        fixtures / example["name"],
        case,
        symlinks=True,
        ignore=shutil.ignore_patterns("expected", "README.md", "benchmark.yaml"),
    )
    for call in planned["calls"]:
        if call["arguments"] is None:
            raise ValueError(
                f"the plan gives {call['specification']} no command line: "
                "its specification names no domain for --spec"
            )
        ran = invoke(binary, ["--format", "json", *call["arguments"]], case)
        stdout = ran.pop("stdout")
        ran["not_implemented"] = ran["exit_status"] == 2 and ran["stderr"].endswith(
            NOT_IMPLEMENTED
        )
        ran["document"] = (
            None if ran["not_implemented"] else document(ran, stdout, case)
        )
        record["calls"].append({**call, **ran})
    return record


def document(ran, stdout, case):
    """The JSON document a run printed: exactly one, on one line, after
    which it exited with status 0 or 1. Its `paths` are kept relative to
    `case`."""
    if ran["exit_status"] not in (0, 1):
        raise ValueError(
            f"yamaa exited with status {ran['exit_status']}: {ran['stderr']}"
        )
    if not stdout.endswith("\n") or "\n" in stdout[:-1]:
        raise ValueError("yamaa printed no single JSON line")
    printed = json.loads(stdout)
    if isinstance(printed, dict) and isinstance(printed.get("paths"), list):
        printed["paths"] = relative(printed["paths"], case)
    return printed


def relative(paths, case):
    """The paths a save wrote, relative to the case's root.

    The binary prints absolute paths with forward slashes (D38). A path that
    has a backslash, or that is not below the case, stays as printed, for the
    comparator to refuse, as the Python and R runners keep it.
    """

    def one(path):
        if not isinstance(path, str) or "\\" in path:
            return path
        try:
            return Path(path).relative_to(case).as_posix()
        except ValueError:
            return path

    return [one(path) for path in paths]


def state(record):
    """excluded, not_implemented, observed or error."""
    if record["error"]:
        return "error"
    if record["excluded"]:
        return "excluded"
    if record["calls"] and all(call["not_implemented"] for call in record["calls"]):
        return "not_implemented"
    return "observed"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--fixtures", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source-revision", required=True)
    args = parser.parse_args(argv)
    fixtures, output = args.fixtures.resolve(), args.output.resolve()
    binary = args.binary.resolve()
    if output.exists() or output.is_relative_to(fixtures):
        parser.error("output must be a fresh directory outside the fixture tree")
    if not binary.is_file():
        parser.error(f"no yamaa binary at {binary}")
    plan = json.loads((args.plan / "plan.json").read_text(encoding="utf-8"))
    if plan["protocol"] != PROTOCOL:
        parser.error("unknown plan protocol")
    version = invoke(str(binary), ["--version"], None)
    if version["exit_status"] != 0:
        parser.error(f"yamaa --version failed: {version['stderr']}")
    write_json(
        output / "run.json",
        {
            "protocol": PROTOCOL,
            "runtime": "cli",
            "version": version["stdout"],
            "source_revision": args.source_revision,
        },
    )
    counts = dict.fromkeys(("excluded", "not_implemented", "observed", "error"), 0)
    for example in plan["examples"]:
        try:
            record = observe(example, fixtures, output / "work", str(binary))
        except Exception as error:  # noqa: BLE001 -- retain one record for every benchmark
            record = {
                "example": example["name"],
                "excluded": None,
                "calls": [],
                "error": f"{type(error).__name__}: {error}",
            }
        write_json(output / "records" / (example["name"] + ".json"), record)
        counts[state(record)] += 1
        print(example["name"], state(record), flush=True)
    print(json.dumps({"runtime": "cli", **counts}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
