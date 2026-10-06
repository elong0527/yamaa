"""Reconcile supplemental contracts and retain actual installed-suite evidence.

Component and compile probes remain separate from complete benchmark runs. Merely
listing a contract or one of its consumers never qualifies its behavior.
"""

from __future__ import annotations

import argparse
import json
import platform
import re
import subprocess
import sys
from pathlib import Path


def _path(root, name):
    """Resolve a repository-relative identity without allowing it to escape."""
    if not isinstance(name, str) or not name or Path(name).is_absolute():
        raise ValueError(f"invalid relative path: {name!r}")
    path = (root / name).resolve()
    if not path.is_relative_to(root.resolve()):
        raise ValueError(f"path leaves root: {name}")
    return path


def _keys(value, expected):
    if not isinstance(value, dict) or set(value) != set(expected):
        raise ValueError(f"expected exactly these fields: {sorted(expected)}")


def load_catalog(root, catalog_path):
    """Reject missing/new contracts and suites instead of freezing the denominator."""
    catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
    _keys(catalog, ("version", "contracts", "suites", "support_files"))
    if catalog["version"] != "1":
        raise ValueError("unsupported supplemental catalog version")
    if not all(
        isinstance(catalog[key], list)
        for key in ("contracts", "suites", "support_files")
    ):
        raise ValueError("contracts, suites and support_files must be lists")
    contracts, suites, ids, staged, supporting = set(), set(), set(), set(), set()
    for contract in catalog["contracts"]:
        _keys(contract, ("path", "family", "level", "consumers"))
        name = contract["path"]
        if name in contracts or not _path(root, name).is_file():
            raise ValueError(f"duplicate or missing contract: {name}")
        contracts.add(name)
        if contract["level"] not in ("component", "compile"):
            raise ValueError(f"invalid supplemental level: {name}")
        if not isinstance(contract["family"], str) or not contract["family"]:
            raise ValueError(f"missing semantic family: {name}")
        consumers = contract["consumers"]
        if (
            not isinstance(consumers, list)
            or not consumers
            or len(set(consumers)) != len(consumers)
        ):
            raise ValueError(f"missing or duplicate contract consumers: {name}")
        for consumer in consumers:
            if not _path(root, consumer).is_file():
                raise ValueError(f"missing contract consumer: {consumer}")
    for suite in catalog["suites"]:
        _keys(suite, ("id", "runtime", "path", "staged_path", "level"))
        if suite["runtime"] not in ("python", "r") or suite["level"] not in (
            "component",
            "compile",
        ):
            raise ValueError(f"invalid suite route or level: {suite['id']}")
        if (
            not isinstance(suite["id"], str)
            or re.fullmatch(r"(?:python|r)/[a-z][a-z0-9_-]*", suite["id"]) is None
            or not suite["id"].startswith(suite["runtime"] + "/")
        ):
            raise ValueError("suite identity must be host/simple-name")
        if suite["id"] in ids or suite["path"] in suites:
            raise ValueError(f"duplicate suite: {suite['id']}")
        if not _path(root, suite["path"]).is_file():
            raise ValueError(f"missing suite: {suite['path']}")
        _path(root, suite["staged_path"])
        stage = suite["runtime"], suite["staged_path"]
        if stage in staged:
            raise ValueError(f"duplicate staged suite path: {stage}")
        staged.add(stage)
        ids.add(suite["id"])
        suites.add(suite["path"])
    for support in catalog["support_files"]:
        _keys(support, ("path", "role"))
        name = support["path"]
        if name in supporting or name in contracts or not _path(root, name).is_file():
            raise ValueError(f"duplicate or missing support file: {name}")
        if support["role"] not in ("input", "documentation"):
            raise ValueError(f"invalid support file role: {name}")
        supporting.add(name)
    discovered_fixtures = {
        p.relative_to(root).as_posix()
        for p in (root / "rust/crates").glob("*/tests/fixtures/**/*")
        if p.is_file()
    }
    discovered_suites = {
        p.relative_to(root).as_posix()
        for pattern in ("rust/tests/installed_*.py", "R/yamaanative/tests/*.R")
        for p in root.glob(pattern)
    }
    for label, expected, actual in (
        ("fixture files", contracts | supporting, discovered_fixtures),
        ("suites", suites, discovered_suites),
    ):
        if expected != actual:
            raise ValueError(
                f"{label} differ: uncataloged={sorted(actual - expected)}, stale={sorted(expected - actual)}"
            )
    return catalog


def installed_metadata(runtime, staged_root):
    """Read versions and location from the actual installed host before its probes."""
    if runtime == "python":
        code = """
import importlib.metadata as metadata
import json, platform
from pathlib import Path
import yamaa, yamaa_native
for module, name in ((yamaa, "yamaa"), (yamaa_native, "yamaa-native")):
    distribution = metadata.distribution(name)
    if distribution.files is None or not any(
        distribution.locate_file(f).resolve() == Path(module.__file__).resolve()
        for f in distribution.files
    ):
        raise RuntimeError("module is not owned by installed distribution: " + name)
print(json.dumps({
    "runtime_version": platform.python_version(),
    "host_package_version": metadata.version("yamaa"),
    "binding_package_version": metadata.version("yamaa-native"),
    "core_version": yamaa_native.engine_info()["core_version"],
    "package_location": str(Path(yamaa_native.__file__).resolve()),
}))
"""
        return json.loads(
            subprocess.check_output(
                [sys.executable, "-I", "-c", code], cwd=staged_root, text=True
            )
        )
    code = """
suppressPackageStartupMessages(library(yamaanative))
writeLines(c(as.character(getRversion()), as.character(packageVersion("yamaanative")),
             engine_info()$core_version, normalizePath(find.package("yamaanative"))))
"""
    lines = subprocess.check_output(
        ["Rscript", "--vanilla", "-e", code], cwd=staged_root, text=True
    ).splitlines()
    if len(lines) != 4 or not all(lines):
        raise ValueError("R installation metadata was malformed")
    return dict(
        zip(
            (
                "runtime_version",
                "host_package_version",
                "core_version",
                "package_location",
            ),
            lines,
            strict=True,
        )
    )


def run_suites(root, catalog, *, runtime, staged_root, output, evidence):
    """Run existing staged suites in fresh processes and preserve failed/unrun rows."""
    if staged_root.resolve().is_relative_to(root.resolve()):
        raise ValueError("installed suites must run outside the checkout")
    metadata = installed_metadata(runtime, staged_root)
    if Path(metadata["package_location"]).is_relative_to(root.resolve()):
        raise ValueError("installed package resolved into the checkout")
    selected = [s for s in catalog["suites"] if s["runtime"] == runtime]
    # Check all staged script bytes before running the first test. No content digest.
    for suite in selected:
        original = _path(root, suite["path"])
        staged = _path(staged_root, suite["staged_path"])
        if original.read_bytes() != staged.read_bytes():
            raise ValueError(
                f"staged suite differs from tested revision: {suite['id']}"
            )
    output.mkdir(parents=True, exist_ok=True)
    rows, failed = [], False
    for suite in selected:
        row = {
            **suite,
            "backend": "rust",
            "result": "not_exercised",
            "exit_code": None,
            "log": None,
        }
        rows.append(row)
        if failed:
            continue
        log = output / (suite["id"].replace("/", "-") + ".log")
        command = [sys.executable, "-E", "-s"] if runtime == "python" else ["Rscript"]
        command.append(str(_path(staged_root, suite["staged_path"])))
        with log.open("wb") as stream:
            try:
                completed = subprocess.run(
                    command,
                    cwd=staged_root,
                    stdout=stream,
                    stderr=subprocess.STDOUT,
                    check=False,
                )
                row["exit_code"] = completed.returncode
                row["result"] = "pass" if completed.returncode == 0 else "failure"
            except OSError as error:
                stream.write(str(error).encode("utf-8"))
                row["result"] = "infrastructure_failure"
        row["log"] = log.name
        failed = row["result"] != "pass"
        print(suite["id"], row["result"], flush=True)
    report = {
        "version": "1",
        "scope": "supplemental_suites",
        "evidence": evidence,
        "installation": metadata,
        "platform": platform.platform(),
        "catalog": catalog,
        "suites": rows,
    }
    (output / "supplemental.json").write_text(
        json.dumps(report, indent=2) + "\n", encoding="utf-8"
    )
    return int(failed)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--catalog", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--runtime", choices=("python", "r"))
    parser.add_argument("--staged-root", type=Path)
    parser.add_argument("--source-revision", required=True)
    parser.add_argument("--artifact-reference")
    parser.add_argument("--host-artifact-reference")
    parser.add_argument("--package-form", choices=("wheel", "source"))
    parser.add_argument("--evidence", required=True)
    args = parser.parse_args(argv)
    root = args.root.resolve()
    actual = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
    ).strip()
    if args.source_revision != actual:
        raise ValueError("source revision differs from current checkout")
    catalog = load_catalog(root, args.catalog)
    evidence = {
        "source_revision": actual,
        "artifact_reference": args.artifact_reference,
        "host_artifact_reference": args.host_artifact_reference,
        "package_form": args.package_form,
        "run": args.evidence,
    }
    if args.runtime:
        if (
            args.staged_root is None
            or not args.artifact_reference
            or args.package_form is None
        ):
            parser.error(
                "suite execution requires staged root, package form and installed artifact reference"
            )
        if args.runtime == "python" and not args.host_artifact_reference:
            parser.error("Python suite execution requires its host artifact reference")
        return run_suites(
            root,
            catalog,
            runtime=args.runtime,
            staged_root=args.staged_root.resolve(),
            output=args.output,
            evidence=evidence,
        )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(
            {
                "version": "1",
                "scope": "supplemental_catalog",
                "evidence": evidence,
                "catalog": catalog,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
