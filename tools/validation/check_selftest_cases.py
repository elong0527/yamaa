"""Check the selftest case list, `benchmarks/selftest.yaml` (#3030 D28, D105).

`selftest` runs a few existing benchmarks through the installed engine, so
their expected bytes stay independently authored; the list names them, and
every package ships them. A listed case must run without project code and
keep its expected files through the 1.0 conversion, so this check refuses:

- a name that is not a benchmark under `benchmarks/`;
- a benchmark that calls a project function: it carries
  `python/environment.yaml` or `r/environment.yaml`, or a project it passes
  links environments or function files (benchmarks/AGENTS.md);
- a benchmark the 1.0 conversion gate excepts (`tools/migrate/excepted.py`);
  this refusal goes when that list empties.

The set needs a negative case, so that selftest runs the issue path as well
as the bytes. Its adapter claims are checked against the cases' files: each
adapter of `ADAPTERS` is shown by a listed case or named under `uncovered`
with its reason, never both.
"""

import argparse
import re
import sys
from pathlib import Path

import yaml
from tools.benchmark import entries, load_metadata
from tools.migrate.excepted import EXCEPTED
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader

LIST = "benchmarks/selftest.yaml"
NAME = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*")
ENVIRONMENT_FILES = ("python/environment.yaml", "r/environment.yaml")
# A project's links to project code: in 0.5, `environments` names each host's
# environment file; in 1.0, `functions` names the function files.
PROJECT_CODE = ("environments", "functions")
# The adapters #3062 names, each with the files that show a case exercising
# it: a reader by an input, a writer by an expected file. `odm`, `parquet`
# and `readstat` are the optional features `info()` reports.
ADAPTERS = {
    "csv": (("input", ".csv"), ("expected", ".csv")),
    "dataset-json": (("expected", ".json"),),
    "define-xml": (("expected", "define.xml"),),
    "odm": (("input", ".xml"),),
    "parquet": (("input", ".parquet"), ("expected", ".parquet")),
    "readstat": (("input", ".sas7bdat"), ("input", ".xpt")),
    "xpt": (("expected", ".xpt"),),
}
READ_ERRORS = (OSError, UnicodeError, ValueError, yaml.YAMLError)


def load(path):
    return yaml.load(path.read_text(encoding="utf-8"), Loader=UniqueKeyLoader)


def adapters(case):
    """The adapters a case's input and expected files show it exercising."""
    names = {
        directory: [
            path.name.lower()
            for path in (case / directory).rglob("*")
            if path.is_file()
        ]
        for directory in ("input", "expected")
    }
    return {
        adapter
        for adapter, evidence in ADAPTERS.items()
        if any(
            name.endswith(suffix)
            for directory, suffix in evidence
            for name in names[directory]
        )
    }


def project_code(case):
    """Why a case calls a project function: its environment files, and each
    project it passes, in an entry or its submission, that links code."""
    found = [name for name in ENVIRONMENT_FILES if (case / name).is_file()]
    metadata = load_metadata(case)
    projects = {
        call["environment"]
        for calls in entries(case, metadata).values()
        for call in calls
        if call["environment"] is not None
    }
    if "define" in metadata:
        projects.add(metadata["define"]["environment"])
    for project in sorted(projects):
        document = load(case / project)
        if isinstance(document, dict):
            found.extend(
                f"{project} links {field}"
                for field in PROJECT_CODE
                if document.get(field)
            )
    return found


def check(root):
    """The list's problems under the repository `root`, as ERROR lines."""
    try:
        document = load(root / LIST)
    except READ_ERRORS as error:
        return [f"ERROR: {LIST}: {error}"]
    if not isinstance(document, dict):
        return [f"ERROR: {LIST}: expected a mapping"]
    errors = [
        f"ERROR: {LIST}.{field}: unknown field"
        for field in sorted(set(document) - {"cases", "uncovered"}, key=str)
    ]
    cases = document.get("cases")
    if not (
        isinstance(cases, list)
        and cases
        and all(isinstance(name, str) for name in cases)
    ):
        return errors + [f"ERROR: {LIST}.cases: expected a list of benchmark names"]
    if cases != sorted(set(cases)):
        errors.append(f"ERROR: {LIST}.cases: expected unique names in sorted order")
    uncovered = document.get("uncovered", {})
    if not isinstance(uncovered, dict):
        errors.append(f"ERROR: {LIST}.uncovered: expected adapters with reasons")
        uncovered = {}
    for adapter, reason in uncovered.items():
        if adapter not in ADAPTERS:
            errors.append(
                f"ERROR: {LIST}.uncovered.{adapter}: unknown adapter, "
                f"expected one of {', '.join(ADAPTERS)}"
            )
        if not (
            isinstance(reason, str)
            and reason
            and reason == reason.strip()
            and "\n" not in reason
        ):
            errors.append(
                f"ERROR: {LIST}.uncovered.{adapter}: expected a one-line reason"
            )
    shown = {}
    negative = False
    for name in cases:
        case = root / "benchmarks" / name
        if not (NAME.fullmatch(name) and (case / "benchmark.yaml").is_file()):
            errors.append(f"ERROR: {LIST}.cases: no benchmark named {name!r}")
            continue
        if name in EXCEPTED:
            errors.append(
                f"ERROR: {LIST}.cases: {name} is excepted from the 1.0 "
                f"conversion gate ({EXCEPTED[name]})"
            )
        try:
            code = project_code(case)
        except READ_ERRORS as error:
            errors.append(f"ERROR: {LIST}.cases: {name}: {error}")
            continue
        if code:
            errors.append(
                f"ERROR: {LIST}.cases: {name} calls a project function: "
                + "; ".join(code)
            )
        negative = negative or (case / "expected" / "error.yaml").is_file()
        for adapter in sorted(adapters(case)):
            shown.setdefault(adapter, name)
    if not negative:
        errors.append(
            f"ERROR: {LIST}.cases: no negative case (expected/error.yaml) "
            "runs the issue path"
        )
    for adapter in ADAPTERS:
        if adapter in uncovered and adapter in shown:
            errors.append(
                f"ERROR: {LIST}.uncovered.{adapter}: {shown[adapter]} covers it"
            )
        elif adapter not in uncovered and adapter not in shown:
            errors.append(
                f"ERROR: {LIST}.uncovered: no listed case covers {adapter}; "
                "list one that does or name it here"
            )
    return errors


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root", type=Path, default=ROOT, help="Repository root directory"
    )
    args = parser.parse_args(argv)
    errors = check(args.root)
    for error in errors:
        print(error)
    if errors:
        return 1
    document = load(args.root / LIST)
    size = sum(
        path.stat().st_size
        for name in document["cases"]
        for path in (args.root / "benchmarks" / name).rglob("*")
        if path.is_file()
    )
    print(
        f"PASS: {LIST} names {len(document['cases'])} selftest cases, {size} bytes; "
        f"not covered: {', '.join(document.get('uncovered') or {}) or 'none'}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
