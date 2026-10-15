"""Read an existing performance run with its unchanged, exact-source auditor.

The retained original Actions ZIPs are inputs, never executables. This helper
starts no measurement, installs no candidate, and changes no original receipt.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

SOURCE = "46ee4dcb0f7ffe5cd8a95b477d242cfadcf0c689"
NATIVE_RUN = "38657181937"


def write_json(path, payload):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n")


def api(repository, route):
    result = subprocess.run(
        ["gh", "api", f"repos/{repository}/{route}"],
        check=True,
        stdout=subprocess.PIPE,
    )
    return json.loads(result.stdout)


def pages(repository, route, key):
    items = []
    for page in range(1, 101):
        payload = api(repository, f"{route}?per_page=100&page={page}")
        items.extend(payload[key])
        if len(payload[key]) < 100:
            return {key: items}
    raise ValueError("metadata pagination exceeds bounded allowance")


def validate_identity(run, repository, run_id, attempt, revision, workflow):
    if (
        run["id"] != run_id
        or run["run_attempt"] != attempt
        or run["head_sha"] != revision
        or run["path"] != workflow
        or run["repository"]["full_name"] != repository
        or run["head_repository"]["full_name"] != repository
    ):
        raise ValueError("original run/source/attempt/repository/workflow mismatch")


def terminal_snapshot(repository, run_id, attempt, revision, records, wait_minutes):
    deadline = time.monotonic() + wait_minutes * 60
    while True:
        run = api(repository, f"actions/runs/{run_id}")
        write_json(records / "run.json", run)
        validate_identity(
            run,
            repository,
            run_id,
            attempt,
            revision,
            ".github/workflows/performance.yml",
        )
        if run["status"] == "completed":
            return run
        if time.monotonic() >= deadline:
            raise ValueError(
                "original performance run did not complete within the bounded wait"
            )
        print(
            "Waiting for the original terminal outcome; no measurements are started",
            flush=True,
        )
        time.sleep(60)


def bind_archives(payload, root, artifacts):
    """Bind supported archive inputs without changing any original identities."""
    artifacts = {item["id"]: item for item in artifacts["artifacts"]}
    for entry in payload["entries"]:
        artifact = artifacts[entry["artifact_id"]]
        archive = root / "raw" / f"{artifact['id']}.zip"
        if (
            artifact["name"] != entry["artifact_name"]
            or archive.stat().st_size != artifact["size_in_bytes"]
        ):
            raise ValueError("original artifact name or downloaded ZIP size differs")
        entry["archive"] = archive.relative_to(root).as_posix()
    return payload


def prepare(args):
    repository = os.environ["GITHUB_REPOSITORY"]
    requested = args.run_id or os.environ.get("GITHUB_REF_NAME", "").removeprefix(
        "evidence/performance-records-"
    )
    if (
        not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository)
        or not re.fullmatch(r"[1-9][0-9]*", requested)
        or not re.fullmatch(r"[1-9][0-9]*", args.native_run_id)
        or not re.fullmatch(r"[0-9a-f]{40}", args.revision)
        or args.attempt < 1
    ):
        raise ValueError("invalid own repository or original run/source/attempt")
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    records = root / "records"
    records.mkdir()
    source = args.source.resolve()
    revision = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=source, text=True
    ).strip()
    if revision != args.revision:
        raise ValueError("reader checkout differs from original frozen source")
    write_json(
        records / "reader.json",
        {
            "source_revision": revision,
            "reader_run_id": os.environ["GITHUB_RUN_ID"],
            "reader_run_attempt": os.environ["GITHUB_RUN_ATTEMPT"],
            "helper_revision": os.environ["GITHUB_SHA"],
            "operation": "unchanged exact-source 54-cell audit of original Actions ZIPs",
        },
    )
    run_id = int(requested)
    run = terminal_snapshot(
        repository, run_id, args.attempt, args.revision, records, args.wait_minutes
    )
    for prefix, identity, attempt in (
        ("", run_id, args.attempt),
        ("native-", int(args.native_run_id), None),
    ):
        original = run if not prefix else api(repository, f"actions/runs/{identity}")
        if prefix:
            validate_identity(
                original,
                repository,
                identity,
                original["run_attempt"],
                args.revision,
                ".github/workflows/native.yml",
            )
            write_json(records / "native-run.json", original)
            attempt = original["run_attempt"]
        write_json(
            records / f"{prefix}jobs.json",
            pages(
                repository, f"actions/runs/{identity}/attempts/{attempt}/jobs", "jobs"
            ),
        )
        write_json(
            records / f"{prefix}artifacts.json",
            pages(repository, f"actions/runs/{identity}/artifacts", "artifacts"),
        )
    if run["conclusion"] != "success":
        # Preserve the actual failed original audit as a failure, including its
        # available job log. Never turn its run metadata into a success receipt.
        jobs = json.loads((records / "jobs.json").read_text())["jobs"]
        for job in jobs:
            if job["name"] == "Audit issue 2288 Linux installed coverage":
                with (records / "original-audit.log").open("wb") as stream:
                    subprocess.run(
                        [
                            "gh",
                            "api",
                            f"repos/{repository}/actions/jobs/{job['id']}/logs",
                        ],
                        stdout=stream,
                        check=False,
                    )
        raise ValueError(
            "original performance attempt did not succeed; retained its actual failure"
        )
    subprocess.run(
        [
            sys.executable,
            str(source / "rust/tools/prepare_scoped_performance_audit.py"),
            "--root",
            str(records),
            "--repository",
            repository,
            "--revision",
            revision,
            "--output",
            str(records / "input.json"),
        ],
        check=True,
    )
    payload = json.loads((records / "input.json").read_text())
    artifacts = json.loads((records / "artifacts.json").read_text())
    (records / "raw").mkdir()
    for entry in payload["entries"]:
        artifact_id = entry["artifact_id"]
        with (records / "raw" / f"{artifact_id}.zip").open("xb") as destination:
            subprocess.run(
                [
                    "gh",
                    "api",
                    f"repos/{repository}/actions/artifacts/{artifact_id}/zip",
                ],
                stdout=destination,
                check=True,
            )
    write_json(records / "input.json", bind_archives(payload, records, artifacts))
    print("Original ZIPs and metadata ready for unchanged 54-cell replay", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", default="")
    parser.add_argument("--native-run-id", default=NATIVE_RUN)
    parser.add_argument("--attempt", type=int, default=1)
    parser.add_argument("--revision", default=SOURCE)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--wait-minutes", type=int, default=180, choices=range(1, 181))
    prepare(parser.parse_args())


if __name__ == "__main__":
    main()
