"""Run original benchmark entries with the installed Python public API.

Run with python -I outside the checkout. Expected files are never copied into
execution staging. The companion comparator is the only truth reader.
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


SECTIONS = ("sdtm", "adam", "send")


def read_yaml(path):
    """Read a project or specification document; inputs, never truth."""
    import yaml12

    return yaml12.parse_yaml(path.read_text(encoding="utf-8"))


def domain_of(path):
    """The `domain` a project names a listed specification by: that of the
    layer nearest `path` that writes one, following each `parent`."""
    seen = set()
    while path.resolve() not in seen:
        seen.add(path.resolve())
        document = read_yaml(path)
        if "domain" in document:
            return document["domain"]
        path = path.parent / document["parent"]
    raise ValueError(f"{path}: cyclic parent")


def project_for(case, project, entry, specification):
    """The project file that lists `specification`, relative to the case.

    A project selects only a specification it lists. A metamorphic variant
    keeps its entry's project metadata through a copy beside the project
    that lists the variant in place of the entry.
    """
    original = (case / entry).resolve()
    if (case / specification).resolve() == original:
        return project
    document = read_yaml(case / project)
    base = PurePosixPath(project).parent
    target = PurePosixPath(os.path.relpath(case / specification, case / base))
    replaced = False
    for section in SECTIONS:
        if isinstance(document.get(section), dict):
            specs = document[section].get("specs") or []
            for index, spec in enumerate(specs):
                if (case / base / spec).resolve() == original:
                    specs[index], replaced = target.as_posix(), True
    if not replaced:
        raise ValueError(f"{project} does not list {entry}")
    derived = base / ("__project_" + PurePosixPath(specification).stem + ".yaml")
    # JSON is YAML; the copy resolves its links from the project's directory.
    (case / derived).write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
    return derived.as_posix()


def stage_lock(case, project, lock):
    """Supply the packaging lock a project names for Python: a 1.0 project
    names it in `environments.python.lock`, and a 0.5 project's environment
    file names one beside itself."""
    document = read_yaml(case / project)
    linked = (document.get("environments") or {}).get("python")
    if linked is None:
        return
    if "yamaa" in document:
        # A 1.0 project names its lock directly, relative to the project file.
        written = linked.get("lock") if isinstance(linked, dict) else None
        if written is None:
            return
        target = (case / project).parent / PurePosixPath(written)
    else:
        # A project's links resolve from the project file; the environment
        # file's lock resolves from the environment file.
        target = (case / project).parent / PurePosixPath(linked).parent / "uv.lock"
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


def table(frame):
    if frame is None:
        return None
    import polars as pl

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

    def observe(run, case):
        tables = {"output": table(run.output), "report": table(run.report)}
        # A save returns the paths it wrote, in publication order; a refused
        # save raises with the paths it wrote first. The record keeps them
        # relative to the case's root.
        saved, paths = False, None
        if not run.issues.height:
            try:
                paths = run.save()
            except yamaa.Error as error:
                paths = error.paths
            else:
                saved = True
        issues = run.issues.to_dicts()
        for issue in issues:
            issue["context"] = json.loads(issue["context"])
        return {
            "issues": issues,
            "tables": tables,
            "saved": saved,
            "paths": relative(paths, case),
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
        # An entry's `environment` names the project file it runs with.
        project = entry["environment"]
        if project is None:
            spec = yamaa.spec(case / specification)
        else:
            stage_lock(case, project, args.python_lock)
            # A project names a specification by its entry's `domain`, which
            # a layer may inherit from its parent.
            name = domain_of(case / entry["specification"])
            listing = project_for(case, project, entry["specification"], specification)
            try:
                spec = yamaa.project(case / listing).spec(name)
            except yamaa.Error as refusal:
                # A project that fails admission refuses to name its specs.
                # The spec-with-project object, which R's runner opens for
                # every project build, then refuses its runs with the
                # project's issues, which the record keeps. A refusal that
                # this spec's check does not reproduce exactly stays this
                # record's error.
                spec = yamaa.Spec(case / specification, os.fspath(case / listing))
                checked = spec.check().issues
                if not checked.height or (
                    checked.to_dicts() != refusal.issues.to_dicts()
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
