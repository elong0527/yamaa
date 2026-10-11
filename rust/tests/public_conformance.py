"""Run original benchmark entries with the installed Python public API.

Run with python -I outside the checkout. Expected files are never copied into
execution staging. The companion comparator is the only truth reader.
"""

from __future__ import annotations

import argparse
import datetime as dt
import importlib.abc
import json
import platform
import shutil
import struct
import sys
from pathlib import Path

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
                    value.replace(tzinfo=dt.timezone.utc)
                    - dt.datetime(1970, 1, 1, tzinfo=dt.timezone.utc)
                ).total_seconds()
            )
        )
    raise ValueError("unknown public table type: " + kind)


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
            "engine_version": _native.engine_info()["core_version"],
            "source_revision": args.source_revision,
            "platform": platform.platform(),
            "package_form": args.package_form,
            "evidence": args.evidence,
            "table_encoding": "str:utf8-hex,float:ieee754-be,int:decimal,date:epoch-days,datetime:epoch-seconds",
        },
    )
    for example in plan["examples"]:
        name = example["name"]
        record = {"example": name, "builds": [], "error": None}
        try:
            if not example["excluded"]:
                case = output / "work" / name
                shutil.copytree(
                    fixtures / name,
                    case,
                    symlinks=True,
                    ignore=shutil.ignore_patterns(
                        "expected", "run.py", "run.R", "README.md"
                    ),
                )
                for entry in example["entries"]["python"]:
                    environment = entry["environment"]
                    if environment is not None:
                        lock = case / Path(environment).parent / "uv.lock"
                        if not lock.exists():
                            shutil.copyfile(args.python_lock, lock)
                    built = yamaa.domain(
                        case / entry["specification"],
                        environment=None if environment is None else case / environment,
                    )
                    tables = {
                        slot: table(getattr(built, slot))
                        for slot in ("output", "verification_log", "warning_log")
                    }
                    saved = False if built.issues.height else built.save()
                    issues = built.issues.to_dicts()
                    for issue in issues:
                        issue["context"] = json.loads(issue["context"])
                    record["builds"].append(
                        {**entry, "issues": issues, "tables": tables, "saved": saved}
                    )
        except Exception as error:  # noqa: BLE001 -- retain one record for every benchmark
            record["error"] = f"{type(error).__name__}: {error}"
        write_json(output / "records" / (name + ".json"), record)
        print(name, "error" if record["error"] else "observed", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
