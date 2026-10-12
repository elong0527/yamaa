"""Seed the public ratchet from complete, replayed evidence for one full run.

Download the public-conformance-* artifacts from a completed native main or
dispatched run, and retain its REST run metadata with them. Each saved report is
recompared with independent truth before its pass can enter the intersection.
"""

from __future__ import annotations

import argparse
import contextlib
import io
import itertools
import re
import tempfile
from pathlib import Path

from audit_native_run import FORMS, PLATFORMS, PYTHONS, read, require, write
from public_conformance import PROTOCOL, compare, plan, platform_matches


def lanes():
    for platform, version, form in itertools.product(PLATFORMS, PYTHONS, FORMS):
        yield (
            f"public-conformance-python-{platform}-{version}-{form}",
            ("python", platform, version, form),
        )
    for platform in PLATFORMS:
        yield f"public-conformance-r-{platform}", ("r", platform, "4.6.1", "source")


def seed(fixtures, artifacts, run_file, output, previous=None):
    require(not output.exists(), "ratchet output must be a fresh file")
    run = read(run_file)
    require(
        run.get("status") == "completed"
        and run.get("event") in {"push", "workflow_dispatch"}
        and (run["event"] != "push" or run.get("head_branch") == "main")
        and run.get("path", "").split("@", 1)[0] == ".github/workflows/native.yml",
        "ratchet requires a completed full native main or dispatched run",
    )
    revision, evidence = run.get("head_sha"), run.get("html_url")
    require(
        isinstance(revision, str) and re.fullmatch(r"[0-9a-f]{40}", revision),
        "run source revision is missing",
    )
    require(
        isinstance(run.get("id"), int)
        and isinstance(run.get("run_attempt"), int)
        and isinstance(evidence, str)
        and evidence.endswith(f"/actions/runs/{run['id']}"),
        "run evidence identity is missing",
    )
    expected = dict(lanes())
    actual = {
        p.name for p in artifacts.iterdir() if p.name.startswith("public-conformance-")
    }
    require(
        actual == set(expected), "public inventory lane set is incomplete or unexpected"
    )
    planned = plan(fixtures)["examples"]
    names = {row["name"] for row in planned}
    passed = {"python": names.copy(), "r": names.copy()}
    versions, retained = set(), []
    with tempfile.TemporaryDirectory(prefix="yamaa-ratchet-replay-") as directory:
        for name, (runtime, platform, version, form) in expected.items():
            original = read(artifacts / name / "inventory.json")
            require(
                original.get("protocol") == PROTOCOL
                and original.get("runtime") == runtime
                and original.get("source_revision") == revision
                and original.get("evidence") == evidence
                and original.get("package_form") == form,
                f"{name}: inventory route or provenance differs",
            )
            require(
                platform_matches(runtime, platform, original.get("platform")),
                f"{name}: inventory platform differs",
            )
            require(
                isinstance(original.get("runtime_version"), str)
                and (
                    original["runtime_version"].startswith(version + ".")
                    if runtime == "python"
                    else original["runtime_version"] == version
                )
                and all(
                    original.get(k) for k in ("host_package_version", "engine_version")
                ),
                f"{name}: installed versions differ or are missing",
            )
            versions.add((original["host_package_version"], original["engine_version"]))
            replay_file = Path(directory) / (name + ".json")
            with contextlib.redirect_stdout(io.StringIO()):
                compare(fixtures, artifacts / name / "observed", replay_file)
            replay = read(replay_file)
            require(
                {k: v for k, v in original.items() if k != "ratchet"}
                == {k: v for k, v in replay.items() if k != "ratchet"},
                f"{name}: saved inventory differs from independent replay",
            )
            rows = replay["benchmarks"]
            require(
                len(rows) == len(names) and {r["example"] for r in rows} == names,
                f"{name}: benchmark denominator differs",
            )
            passed[runtime] &= {r["example"] for r in rows if r["verdict"] == "pass"}
            retained.append({"artifact": name, "summary": replay["summary"]})
    require(len(versions) == 1, "installed package/core versions differ between lanes")
    if previous is not None:
        old = read(previous)
        for runtime, required in passed.items():
            require(
                isinstance(old.get(runtime), list) and set(old[runtime]) <= required,
                f"{runtime}: the prior ratchet would shrink",
            )
    payload = {
        "protocol": "public-conformance-ratchet/1",
        "source_revision": revision,
        "evidence": evidence,
        "run_attempt": run["run_attempt"],
        "benchmarks": len(names),
        "python": sorted(passed["python"]),
        "r": sorted(passed["r"]),
        "inventories": retained,
        "exclusions": [
            {"example": row["name"], "reason": row["excluded"]}
            for row in planned
            if row["excluded"]
        ],
    }
    write(output, payload)
    return payload


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", type=Path, required=True)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--run", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--previous", type=Path)
    args = parser.parse_args()
    value = seed(args.fixtures, args.artifacts, args.run, args.output, args.previous)
    print(f"ratcheted {len(value['python'])} Python and {len(value['r'])} R benchmarks")


if __name__ == "__main__":
    main()
