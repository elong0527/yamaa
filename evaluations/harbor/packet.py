"""Assemble what a full prompt is written from, and nothing it is graded on.

The writer of a full prompt gets what a statistician hands a programmer:
the benchmark's data contract (README), its specifications without their
checks, the project functions they call, the language rules, the inputs the
evaluated agent also gets, and the prompt's skeleton (`scaffold.py`). The
writer never gets `expected/`, anything computed from it, an earlier prompt
or solution, or what a solver did with one. Nothing here reads `expected/`::

    python -m evaluations.harbor.packet benchmarks/<case> --out <dir>

writes `<dir>/sources/` (mounted at `/app/sources` in a writing task) and
`<dir>/input/` (mounted at `/app/input`), replacing earlier ones and leaving
anything else in `<dir>` alone.
"""

from __future__ import annotations

import argparse
import csv
import json
import re
import shutil
from pathlib import Path

import yaml
from evaluations.harbor import scaffold
from evaluations.harbor.grade import HOLDOUT_INPUT_SUBJECTS

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
RULES = ROOT / "rules"
PROGRAMS = ROOT / "rust" / "tests" / "project-programs" / "python" / "src"
# The writer can run the same checks as the verifier.
CHECKS = ("check.py", "brief.py")
# Verifications check the fixture (row counts among them), not the request.
CHECK_KEYS = frozenset({"verifications"})
# An operation name such as `first_available`; a prompt states values, not
# the operations that compute them.
OPERATION = re.compile(r"[a-z]+(?:_[a-z]+)+")
ISO_DATE = re.compile(r"\b\d{4}-\d{2}-\d{2}\b")
NAME = re.compile(r"[A-Za-z0-9_][A-Za-z0-9_-]*")


def trimmed(node: object) -> object:
    """The specification tree without its checks."""
    if isinstance(node, dict):
        return {k: trimmed(v) for k, v in node.items() if k not in CHECK_KEYS}
    if isinstance(node, list):
        return [trimmed(v) for v in node]
    return node


def paths(node: object, prefix: str = "") -> list[str]:
    """Every dotted path a trace may cite: mapping keys, and list entries by
    name or id (`columns.DTHDT.derivation`), else by position."""
    found = [prefix] if prefix else []
    if isinstance(node, dict):
        children = list(node.items())
    elif isinstance(node, list):
        children = [
            (v.get("name") or v.get("id") or i if isinstance(v, dict) else i, v)
            for i, v in enumerate(node)
        ]
    else:
        return found
    for key, value in children:
        found += paths(value, f"{prefix}.{key}" if prefix else str(key))
    return found


def specifications(benchmark: Path) -> dict[str, dict]:
    """Every specification and input schema, trimmed, by relative path."""
    files = sorted(benchmark.glob("spec*.yaml")) + sorted(
        (benchmark / "input").glob("*.schema.yaml")
    )
    return {
        path.relative_to(benchmark).as_posix(): trimmed(
            yaml.safe_load(path.read_text(encoding="utf-8"))
        )
        for path in files
    }


def functions(benchmark: Path) -> dict[str, str]:
    """Project function declarations without their tests, by file name, and
    the Python source that implements them.

    A project lists its function files under `functions` (#3030 D8); each is
    written as `<name>.yaml`, and its `implementations.python` names the
    module.
    """
    project = benchmark / "project.yaml"
    document = (
        yaml.safe_load(project.read_text(encoding="utf-8"))
        if project.is_file()
        else None
    )
    found, modules = (
        _function_files(project, document)
        if isinstance(document, dict)
        else ({}, set())
    )
    for module in sorted(modules):
        source = PROGRAMS / module / "__init__.py"
        if source.is_file():
            found[f"{module}.py"] = source.read_text(encoding="utf-8")
    return found


def _function_files(project: Path, document: dict) -> tuple[dict[str, str], set]:
    """A project's function files without their tests, and the Python modules
    their implementations name."""
    found, modules = {}, set()
    for listed in document.get("functions") or []:
        function = yaml.safe_load((project.parent / listed).read_text(encoding="utf-8"))
        declared = {k: v for k, v in function.items() if k != "tests"}
        found[f"{function['name']}.yaml"] = yaml.safe_dump(
            declared, sort_keys=False, allow_unicode=True
        )
        python = (function.get("implementations") or {}).get("python")
        if python:
            modules.add(python.partition(".")[0])
    return found, modules


def _keys(node: object) -> set[str]:
    if isinstance(node, dict):
        return set(map(str, node)).union(*map(_keys, node.values()))
    if isinstance(node, list):
        return set().union(*map(_keys, node))
    return set()


def _input_files(benchmark: Path) -> list[Path]:
    return sorted(
        p
        for p in (benchmark / "input").rglob("*")
        if p.is_file() and p.suffix not in (".yaml", ".yml")
    )


def _input_rows(path: Path) -> tuple[list[str], list[dict]]:
    if path.suffix == ".parquet":
        import pyarrow.parquet as pq

        table = pq.read_table(path)
        return table.column_names, [
            {k: "" if v is None else str(v) for k, v in row.items()}
            for row in table.to_pylist()
        ]
    with path.open(newline="", encoding="utf-8") as handle:
        reader = csv.DictReader(handle)
        return list(reader.fieldnames or []), list(reader)


def unsaid(benchmark: Path) -> dict[str, str]:
    """Names a prompt never says, with the reason.

    Subject identifiers and complete dates of the fixture, and names only the
    specification uses (intermediate results, columns it declares but does
    not write, operations). A name the README or the inputs use, or that a
    specification states as part of a rule, is said freely.
    """
    readme = (benchmark / "README.md").read_text(encoding="utf-8")
    specs = specifications(benchmark)
    stated = readme + "".join(
        p.read_text(encoding="utf-8") for p in sorted(benchmark.glob("spec*.yaml"))
    )
    found, seen = {}, set(NAME.findall(readme))
    for path in _input_files(benchmark):
        columns, rows = _input_rows(path)
        seen |= set(columns) | {path.name.partition(".")[0].upper()}
        for row in rows:
            for column, value in row.items():
                value = (value or "").strip()
                if column in HOLDOUT_INPUT_SUBJECTS and len(value) >= 4:
                    found.setdefault(value, "a subject identifier in the inputs")
                for date in ISO_DATE.findall(value):
                    found.setdefault(date, "a date in the inputs")
    found = {name: reason for name, reason in found.items() if name not in stated}
    written = set()
    for spec in specs.values():
        seen |= set(spec.get("input") or {})
        written |= set((spec.get("output") or {}).get("columns") or [])
    internal = {}
    for spec in specs.values():
        for intermediate in spec.get("intermediates") or []:
            if isinstance(intermediate, dict) and intermediate.get("id"):
                internal[intermediate["id"]] = "an intermediate result"
        for column in spec.get("columns") or []:
            if isinstance(column, dict) and column.get("name") not in written:
                internal[column["name"]] = "a column the output does not hold"
        for name in sorted(_keys(spec)):
            if OPERATION.fullmatch(name):
                internal.setdefault(name, "a field or operation name")
    found.update(
        {
            name: f"{reason} of the specification"
            for name, reason in internal.items()
            if name not in seen and name not in written
        }
    )
    return found


def check_data(benchmark: Path) -> dict:
    """What `check.py` needs to check a prompt without the expected data."""
    return {
        "opening": scaffold.opening_start(benchmark),
        "domains": scaffold.domains(benchmark),
        "columns": scaffold.columns_paragraph(benchmark),
        "paths": scaffold.paths_paragraph(benchmark),
        "unsaid": unsaid(benchmark),
        "readme": (benchmark / "README.md").read_text(encoding="utf-8"),
        "specification": {
            name: paths(spec) for name, spec in specifications(benchmark).items()
        },
        "inputs": [
            p.relative_to(benchmark).as_posix() for p in _input_files(benchmark)
        ],
        "rules": sorted(p.relative_to(ROOT).as_posix() for p in RULES.rglob("*.md")),
    }


def write(benchmark: Path, out: Path) -> Path:
    """Write the writer's sources and inputs under `out`; the benchmark's
    files are the only thing that shapes them. An earlier `sources/` and
    `input/` there are replaced, and nothing else in `out` is touched."""
    sources = out / "sources"
    for written in (sources, out / "input"):
        if written.is_dir() and not written.is_symlink():
            shutil.rmtree(written)
        else:
            written.unlink(missing_ok=True)
    (sources / "specification").mkdir(parents=True)
    shutil.copyfile(benchmark / "README.md", sources / "README.md")
    for name, spec in specifications(benchmark).items():
        target = sources / "specification" / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(yaml.safe_dump(spec, sort_keys=False, allow_unicode=True))
    for name, text in functions(benchmark).items():
        (sources / "functions").mkdir(exist_ok=True)
        (sources / "functions" / name).write_text(text, encoding="utf-8")
    shutil.copytree(RULES, sources / "rules")
    (sources / "skeleton.md").write_text(scaffold.skeleton(benchmark))
    for name in CHECKS:
        shutil.copyfile(HERE / name, sources / name)
    (sources / "check.json").write_text(
        json.dumps(check_data(benchmark), indent=2) + "\n"
    )
    # Data files only, as the agent gets them (`build.build_task`).
    shutil.copytree(
        benchmark / "input",
        out / "input",
        ignore=shutil.ignore_patterns("*.yaml", "*.yml"),
    )
    return out


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("benchmark", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    print(write(args.benchmark.resolve(), args.out))


if __name__ == "__main__":
    main()
