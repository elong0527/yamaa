"""Retain a replay index for every original performance tuple, including gaps.

This index selects evidence; only the independent auditor admits observations.
Its source bundle and relative paths allow replay after Actions runners disappear.
"""

from __future__ import annotations

import argparse
import datetime
import itertools
import json
import re
import subprocess
from pathlib import Path

PLATFORMS = ("ubuntu-24.04", "macos-15", "windows-latest")
PYTHONS = ("3.12", "3.13", "3.14")
ROLES = (
    ("candidate", "candidate-evidence"),
    ("baseline", "baseline-evidence"),
    ("producer-root-build-baseline", "producer-root-build-baseline"),
)
VARIANTS = (
    "lookup-1",
    "lookup-4",
    "aggregate-10",
    "aggregate-100",
    "window-10",
    "window-100",
    "callback-1",
    "producer-1",
    "producer-3",
)
# Preserve the original adapter import for existing retained R-only campaigns.
R_VARIANTS = VARIANTS


def variant_cases(variant):
    if variant not in VARIANTS:
        raise ValueError("unexpected performance variant")
    family, dimension = variant.split("-")
    return [f"{family}-{rows}-{dimension}" for rows in (1000, 10000, 100000)]


def producer_job(item, variant=None):
    if item["host"] == "python":
        return f"performance-python ({item['os']}, {item['python']}, {item['form']}{', ' + variant if variant else ''})"
    return f"performance-r ({item['os']}, source{', ' + variant if variant else ''})"


def write(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=True) + "\n")


def api(endpoint, pages=False):
    command = ["gh", "api", endpoint]
    if pages:
        command += ["--paginate", "--slurp"]
    return json.loads(subprocess.check_output(command, text=True))


def without_digests(value):
    if isinstance(value, dict):
        return {
            key: without_digests(item) for key, item in value.items() if key != "digest"
        }
    if isinstance(value, list):
        return [without_digests(item) for item in value]
    return value


def tuples():
    for host, version, form in itertools.product(
        PLATFORMS, PYTHONS, ("wheel", "source")
    ):
        yield (
            {"host": "python", "os": host, "python": version, "form": form},
            f"performance-python-{host}-{version}-{form}",
        )
    for host in PLATFORMS:
        yield (
            {"host": "r", "os": host, "python": None, "form": "source"},
            f"performance-r-{host}-source",
        )


def timestamp(value):
    if not isinstance(value, str):
        return None
    try:
        parsed = datetime.datetime.fromisoformat(value)
    except ValueError:
        return None
    return parsed if parsed.utcoffset() is not None else None


def artifact_matches_attempt(artifact, jobs, item, attempt, variant=None):
    created = timestamp(artifact.get("created_at"))
    if created is None:
        return False
    producer = producer_job(item, variant)
    matches = 0
    for job in jobs:
        if (
            job.get("name") != producer
            or type(job.get("run_attempt")) is not int
            or job["run_attempt"] != attempt
            or job.get("status") != "completed"
        ):
            continue
        started, completed = (
            timestamp(job.get("started_at")),
            timestamp(job.get("completed_at")),
        )
        if (
            started is not None
            and completed is not None
            and started <= created <= completed
        ):
            matches += 1
    return matches == 1


def manifest(
    run,
    artifacts,
    repository,
    revision,
    attempt,
    native=False,
    *,
    jobs,
    r_variants=False,
    python_variants=False,
):
    if (
        run["repository"]["full_name"] != repository
        or run["head_repository"]["full_name"] != repository
        or run["head_sha"] != revision
        or run["run_attempt"] != attempt
        or run["path"] != ".github/workflows/performance.yml"
        or run["event"] not in {"push", "workflow_dispatch"}
    ):
        raise ValueError("performance run differs from the exact source/attempt")

    def original_entry(item, name, variant=None):
        matches = [
            artifact
            for artifact in artifacts
            if artifact["name"] == name
            and artifact_matches_attempt(artifact, jobs, item, attempt, variant)
        ]
        # Missing/ambiguous evidence remains a named gap rather than shrinking
        # the approved matrix or selecting an arbitrary artifact.
        identity = matches[0]["id"] if len(matches) == 1 else None
        roles = ROLES
        if variant is not None:
            applicable = {"candidate"}
            if variant.startswith(("lookup-", "aggregate-", "window-")):
                applicable.add("baseline")
            elif variant.startswith("producer-"):
                applicable.add("producer-root-build-baseline")
            roles = [pair for pair in ROLES if pair[0] in applicable]
        return {
            "artifact_id": identity,
            "artifact_name": name,
            "run_attempt": attempt,
            "archive": f"{name}.zip",
            "campaign": f"{name}/machine-campaign.json",
            "collections": [
                {"role": role, "path": f"{name}/{directory}"}
                for role, directory in roles
            ],
        }

    entries = []
    for item, name in tuples():
        if (item["host"] == "r" and r_variants) or (
            item["host"] == "python" and python_variants
        ):
            entries.append(
                {
                    "tuple": item,
                    "segments": [
                        {
                            "variant": variant,
                            "cases": variant_cases(variant),
                            **original_entry(item, f"{name}-{variant}", variant),
                        }
                        for variant in VARIANTS
                    ],
                }
            )
        else:
            entries.append({"tuple": item, **original_entry(item, name)})
    result = {
        "protocol": "public-performance-audit-input/1",
        "repository": repository,
        "source_revision": revision,
        "run": "run.json",
        "artifacts": "artifacts.json",
        "jobs": "jobs.json",
        "source_bundle": "source.bundle",
        "entries": entries,
    }
    if native:
        result.update(
            {
                "native_run": "native-run.json",
                "native_jobs": "native-jobs.json",
                "native_artifacts": "native-artifacts.json",
            }
        )
    return result


def native_snapshot(root, repository, run_id, attempt):
    prefix = f"repos/{repository}/actions/runs/{run_id}"
    run = api(f"{prefix}/attempts/{attempt}")
    jobs = [
        item
        for page in api(f"{prefix}/attempts/{attempt}/jobs?per_page=100", True)
        for item in page["jobs"]
    ]
    artifacts = without_digests(
        [
            item
            for page in api(f"{prefix}/artifacts?per_page=100", True)
            for item in page["artifacts"]
        ]
    )
    for name, value in (
        ("native-run.json", run),
        ("native-jobs.json", jobs),
        ("native-artifacts.json", artifacts),
    ):
        if (root / name).exists():
            raise ValueError("cannot replace original native admission metadata")
        write(root / name, value)


def index(args):
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repository):
        raise ValueError("invalid repository")
    if not re.fullmatch(r"[0-9a-f]{40}", args.revision):
        raise ValueError("invalid exact source revision")
    if args.run_id <= 0 or args.attempt <= 0:
        raise ValueError("invalid performance run/attempt")
    native = bool(args.native_run_id or args.native_attempt)
    if native and not (
        args.native_run_id.isdecimal()
        and int(args.native_run_id) > 0
        and args.native_attempt.isdecimal()
        and int(args.native_attempt) > 0
    ):
        raise ValueError("invalid original native run/attempt")
    head = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=args.checkout, text=True
    ).strip()
    if head != args.revision:
        raise ValueError("retained source checkout differs from the exact candidate")
    root = args.input.resolve()
    prefix = f"repos/{args.repository}/actions/runs/{args.run_id}"
    run = api(f"{prefix}/attempts/{args.attempt}")
    artifact_pages = api(f"{prefix}/artifacts?per_page=100", pages=True)
    artifacts = without_digests(
        [item for page in artifact_pages for item in page["artifacts"]]
    )
    jobs = [
        item
        for page in api(
            f"{prefix}/attempts/{args.attempt}/jobs?per_page=100", pages=True
        )
        for item in page["jobs"]
    ]
    root.mkdir(parents=True, exist_ok=True)
    for name, value in (
        ("run.json", run),
        ("artifacts.json", artifacts),
        ("jobs.json", jobs),
    ):
        if (root / name).exists():
            raise ValueError("cannot replace an original metadata snapshot")
        write(root / name, value)
    if native:
        native_snapshot(root, args.repository, args.native_run_id, args.native_attempt)
    value = manifest(
        run,
        artifacts,
        args.repository,
        args.revision,
        args.attempt,
        native,
        jobs=jobs,
        r_variants=getattr(args, "r_variants", False),
        python_variants=getattr(args, "python_variants", False),
    )
    if (root / "source.bundle").exists() or (root / "manifest.json").exists():
        raise ValueError("cannot replace an original performance index")
    # Include complete candidate/baseline histories. Direct commit identities
    # and retained source bytes, rather than content digests, pin the replay.
    subprocess.run(
        [
            "git",
            "bundle",
            "create",
            str(root / "source.bundle"),
            "HEAD",
            args.baseline_ref,
        ],
        cwd=args.checkout,
        check=True,
    )
    subprocess.run(
        ["git", "bundle", "verify", str(root / "source.bundle")],
        cwd=args.checkout,
        check=True,
    )
    write(root / "manifest.json", value)
    return value


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--checkout", type=Path, required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--run-id", type=int, required=True)
    parser.add_argument("--attempt", type=int, required=True)
    parser.add_argument("--baseline-ref", default="yamaa-py-0.3.0")
    parser.add_argument("--native-run-id", default="")
    parser.add_argument("--native-attempt", default="")
    parser.add_argument("--r-variants", action="store_true")
    parser.add_argument("--python-variants", action="store_true")
    index(parser.parse_args())
