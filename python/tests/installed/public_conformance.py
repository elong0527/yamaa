"""Run original benchmark entries with the installed Python public API.

Run with python -I outside the checkout. Expected files are never copied into
execution staging. The companion comparator is the only truth reader.

The runner stages each case below --output and records it there. With
--output-as, another path that reaches that directory, such as a Windows
network share or a drive mapped to it (#3030 D15), every call names the
staged case through that path instead.
"""

from __future__ import annotations

import argparse
import datetime as dt
import importlib.abc
import json
import os
import platform
import shutil
import struct
import sys
from pathlib import Path, PurePosixPath

PROTOCOL = "public-conformance/1"
REFERENCE_MODULES = (
    "yamaa._reference_domain",
    "yamaa.application",
    "yamaa.adapters",
    "yamaa.expressions",
    "yamaa.io",
    "yamaa.models",
    "yamaa.odm",
    "yamaa.planning",
    "yamaa.runtime",
    "yamaa.schema",
    "yamaa.specification",
    "yamaa.submission",
    "yamaa.verification",
)


class NoReference(importlib.abc.MetaPathFinder):
    def find_spec(self, fullname, path=None, target=None):
        if any(
            fullname == name or fullname.startswith(name + ".")
            for name in REFERENCE_MODULES
        ):
            raise AssertionError(
                "public runner imported reference semantics: " + fullname
            )


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )


def cell(value, kind):
    if value is None:
        return None
    if kind == "str":
        return value.encode("utf-8").hex()
    if kind == "float":
        return struct.pack(">d", value).hex()
    if kind == "int":
        return str(value)
    if kind == "date":
        return str((value - dt.date(1970, 1, 1)).days)
    if kind == "datetime":
        return str(
            int(
                (
                    value.replace(tzinfo=dt.UTC)
                    - dt.datetime(1970, 1, 1, tzinfo=dt.UTC)
                ).total_seconds()
            )
        )
    raise ValueError("unknown public table type: " + kind)


def read_yaml(path):
    """Read a project or specification document; inputs, never truth."""
    import yaml12

    return yaml12.parse_yaml(path.read_text(encoding="utf-8"))


def domain_of(path):
    """The `domain` a project names a listed specification by: the one the
    composition gives, a layer's own before the fragments it includes, the
    last listed first, and those before its parent chain (LAYER-3)."""
    seen = set()
    while path.resolve() not in seen:
        seen.add(path.resolve())
        document = read_yaml(path)
        if "domain" in document:
            return document["domain"]
        for written in reversed(document.get("include") or []):
            fragment = read_yaml(path.parent / written)
            if "domain" in fragment:
                return fragment["domain"]
        path = path.parent / document["parent"]
    raise ValueError(f"{path}: cyclic parent")


def stage_lock(case, project, lock):
    """Supply the packaging lock a project names for Python in
    `environments.python.lock`, relative to the project file."""
    document = read_yaml(case / project)
    linked = (document.get("environments") or {}).get("python")
    written = linked.get("lock") if isinstance(linked, dict) else None
    if written is None:
        return
    target = (case / project).parent / PurePosixPath(written)
    if not target.exists():
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(lock, target)


def relative(paths, case):
    """The paths a save returned, relative to the case's root, so a record
    reads the same on every machine, container and operating system.

    save() returns absolute paths with forward slashes. A path that has a
    backslash, or that is not below the case, stays as returned, for the
    comparator to refuse.
    """
    if paths is None:
        return None

    def one(path):
        if "\\" in path:
            return path
        try:
            return Path(path).relative_to(case).as_posix()
        except ValueError:
            return path

    return [one(path) for path in paths]


def named_output(output, written):
    """The directory through which every call names the staged cases.

    That is `output`, unless `written` (--output-as) gives another path that
    reaches it, which is then made absolute but not resolved: resolving a
    mapped drive would name its share instead, and the engine would never see
    the drive. The record already written in `output` must read back through
    that path unchanged; otherwise it reaches another directory, and this
    raises ValueError.
    """
    if written is None:
        return output
    named = Path(os.path.abspath(written))
    record = (output / "run.json").read_bytes()
    try:
        reached = (named / "run.json").read_bytes() == record
    except OSError:
        reached = False
    if not reached:
        raise ValueError(f"--output-as {written} does not reach --output {output}")
    return named


def table(result):
    """A yamaa.Table's columns, with their types, and its cells as text."""
    if result is None:
        return None
    import polars as pl

    frame = result.to_polars()
    kinds = []
    for dtype in frame.dtypes:
        if dtype == pl.String:
            kinds.append("str")
        elif dtype == pl.Int64:
            kinds.append("int")
        elif dtype == pl.Float64:
            kinds.append("float")
        elif dtype == pl.Date:
            kinds.append("date")
        elif isinstance(dtype, pl.Datetime):
            kinds.append("datetime")
        else:
            raise ValueError(f"unexpected public type: {dtype}")
    return {
        "columns": list(map(list, zip(frame.columns, kinds))),
        "rows": [
            [cell(value, kind) for value, kind in zip(row, kinds)]
            for row in frame.iter_rows()
        ],
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--fixtures", type=Path, required=True)
    parser.add_argument("--python-lock", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--output-as",
        type=Path,
        help="another path that reaches --output, such as a network share or a "
        "drive mapped to it, through which every call names the staged cases",
    )
    parser.add_argument("--source-revision", required=True)
    parser.add_argument("--package-form", default="installed")
    parser.add_argument("--evidence")
    args = parser.parse_args(argv)
    fixtures, output = args.fixtures.resolve(), args.output.resolve()
    if output.exists() or output.is_relative_to(fixtures):
        parser.error("output must be a fresh directory outside the fixture tree")
    if not sys.flags.isolated:
        parser.error("installed public conformance requires python -I")
    if any(name in sys.modules for name in REFERENCE_MODULES):
        raise AssertionError("reference module was loaded before execution")
    sys.meta_path.insert(0, NoReference())
    import yamaa
    from yamaa import _native

    if Path(yamaa.__file__).resolve().is_relative_to(fixtures.parent):
        parser.error(
            "public runner requires an installed package outside the fixture checkout"
        )
    plan = json.loads((args.plan / "plan.json").read_text(encoding="utf-8"))
    if plan["protocol"] != PROTOCOL:
        parser.error("unknown plan protocol")
    write_json(
        output / "run.json",
        {
            "protocol": PROTOCOL,
            "runtime": "python",
            "runtime_version": platform.python_version(),
            "host_package_version": yamaa.__version__,
            "engine_version": _native._CORE_VERSION,
            "source_revision": args.source_revision,
            "platform": platform.platform(),
            "package_form": args.package_form,
            "evidence": args.evidence,
            "table_encoding": "str:utf8-hex,float:ieee754-be,int:decimal,date:epoch-days,datetime:epoch-seconds",
        },
    )
    try:
        named = named_output(output, args.output_as)
    except ValueError as error:
        parser.error(str(error))

    def through(case):
        """A staged case as every call names it."""
        return named / case.relative_to(output)

    def observe(run, case):
        tables = {"output": table(run.output), "report": table(run.report)}
        # A save returns the paths it wrote, in publication order; a refused
        # save raises with the paths it wrote first. The record keeps them
        # relative to the case's root: the case as the calls named it,
        # resolved, because the engine returns resolved paths, those of a
        # mapped drive on its share.
        saved, paths = False, None
        if not run.issues.num_rows:
            try:
                paths = run.save()
            except yamaa.Error as error:
                paths = error.paths
            else:
                saved = True
        issues = run.issues.to_pylist()
        for issue in issues:
            issue["context"] = json.loads(issue["context"])
        return {
            "issues": issues,
            "tables": tables,
            "saved": saved,
            "paths": relative(paths, through(case).resolve()),
        }

    def files(case):
        return {
            path: path.read_bytes()
            for path in sorted(case.rglob("*"))
            if path.is_file() and not path.is_symlink()
        }

    def plan_of(run):
        """How the run got its plan, from its private observations:
        compiled, refused or reused."""
        return json.loads(run._native.observations(True))["measurements"]["plan"]

    def build(case, entry, specification, repeat=False):
        # The runner stages and reads `case` itself; every call names it as
        # `called`, the same directory through --output-as when given.
        called = through(case)
        # An entry's `environment` names the project file it runs with.
        project = entry["environment"]
        if project is None:
            spec = yamaa.spec(called / specification)
        else:
            stage_lock(case, project, args.python_lock)
            try:
                if specification == entry["specification"]:
                    # A project names a specification by its entry's domain.
                    name = domain_of(case / entry["specification"])
                    spec = yamaa.project(called / project).spec(name)
                else:
                    # Variants keep the same project, whose specs may be
                    # inherited. Use the spec-with-project constructor, as
                    # the R runner does; the engine composes its metadata.
                    spec = yamaa.Spec(
                        called / specification, os.fspath(called / project)
                    )
            except yamaa.Error as refusal:
                # A project that fails admission refuses to name its specs.
                # The spec-with-project object, which R's runner opens for
                # every project build, then refuses its runs with the
                # project's issues, which the record keeps. A refusal that
                # this spec's check does not reproduce exactly stays this
                # record's error.
                spec = yamaa.Spec(called / specification, os.fspath(called / project))
                checked = spec.check().issues
                if not checked.num_rows or (
                    checked.to_pylist() != refusal.issues.to_pylist()
                ):
                    raise refusal
        first = spec.run()
        record = {
            "specification": specification,
            "environment": project,
            **observe(first, case),
        }
        if repeat:
            # The repeat pass (#2821): a second run of the same object reuses
            # what the first compiled and must equal it, saving the same bytes.
            record["plan"] = plan_of(first)
            before = files(case)
            second = spec.run()
            again = observe(second, case)
            after = files(case)
            changed = sorted(
                path.relative_to(case).as_posix()
                for path in before.keys() | after.keys()
                if before.get(path) != after.get(path)
            )
            record["repeat"] = {
                **again,
                "plan": plan_of(second),
                "changed_files": changed,
            }
        return record

    for example in plan["examples"]:
        name = example["name"]
        record = {"example": name, "builds": [], "variants": [], "error": None}
        try:
            if not example["excluded"]:
                case = output / "work" / name
                shutil.copytree(
                    fixtures / name,
                    case,
                    symlinks=True,
                    ignore=shutil.ignore_patterns(
                        "expected", "README.md", "benchmark.yaml"
                    ),
                )
                staged = args.plan / "variants" / name
                if staged.is_dir():
                    for source in sorted(staged.rglob("*")):
                        if source.is_file():
                            target = case / source.relative_to(staged)
                            target.parent.mkdir(parents=True, exist_ok=True)
                            shutil.copyfile(source, target)
                for entry in example["entries"]["python"]:
                    record["builds"].append(
                        build(case, entry, entry["specification"], repeat=True)
                    )
                variants = example.get("variants", {}).get("python", [])
                if variants:
                    # Variants declare the same output paths as their entry, so
                    # build them in a copy of the staged case: a variant's
                    # save() must not overwrite the entry's saved files, whose
                    # bytes the comparator checks against independent truth.
                    variant_case = output / "work" / (name + "__variants")
                    shutil.copytree(case, variant_case, symlinks=True)
                    for variant in variants:
                        entry = example["entries"]["python"][variant["entry"]]
                        record["variants"].append(
                            {
                                "kind": variant["kind"],
                                "entry": variant["entry"],
                                **build(variant_case, entry, variant["specification"]),
                            }
                        )
        except Exception as error:  # noqa: BLE001 -- retain one record for every benchmark
            record["error"] = f"{type(error).__name__}: {error}"
        write_json(output / "records" / (name + ".json"), record)
        print(name, "error" if record["error"] else "observed", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
