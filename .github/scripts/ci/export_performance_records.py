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


def producer_snapshots(repository, run_id, attempt, revision, records, wait_minutes):
    # Preserve the first authentic run receipt. A later jobs/artifacts snapshot
    # must never manufacture a different status for this original receipt.
    run = api(repository, f"actions/runs/{run_id}")
    write_json(records / "run.json", run)
    validate_identity(
        run, repository, run_id, attempt, revision, ".github/workflows/performance.yml"
    )
    deadline = time.monotonic() + wait_minutes * 60
    current = run
    owners = {
        "performance-python (ubuntu-24.04, 3.12, wheel)": "performance-python-ubuntu-24.04-3.12-wheel",
        "performance-r (ubuntu-24.04, source)": "performance-r-ubuntu-24.04-source",
    }
    while True:
        jobs = pages(
            repository, f"actions/runs/{run_id}/attempts/{attempt}/jobs", "jobs"
        )
        artifacts = pages(repository, f"actions/runs/{run_id}/artifacts", "artifacts")
        write_json(records / "jobs.json", jobs)
        write_json(records / "artifacts.json", artifacts)
        if current["status"] == "completed":
            write_json(records / "completion-run.json", current)
            if current["conclusion"] != "success":
                raise ValueError(
                    "original performance attempt failed; retained actual outcome"
                )
        ready = True
        for job_name, artifact_name in owners.items():
            producers = [job for job in jobs["jobs"] if job["name"] == job_name]
            archives = [
                item for item in artifacts["artifacts"] if item["name"] == artifact_name
            ]
            if len(producers) > 1 or len(archives) > 1:
                raise ValueError("ambiguous original owner job/artifact")
            if (
                producers
                and producers[0]["status"] == "completed"
                and producers[0]["conclusion"] != "success"
            ):
                raise ValueError("original performance producer failed")
            ready = ready and bool(
                producers and archives and producers[0]["status"] == "completed"
            )
        if ready:
            return run
        if time.monotonic() >= deadline:
            raise ValueError(
                "original producers did not complete within the bounded wait"
            )
        print(
            "Waiting only for original successful producers; no measurements are started",
            flush=True,
        )
        time.sleep(60)
        current = api(repository, f"actions/runs/{run_id}")
        validate_identity(
            current,
            repository,
            run_id,
            attempt,
            revision,
            ".github/workflows/performance.yml",
        )


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
    producer_snapshots(
        repository, run_id, args.attempt, args.revision, records, args.wait_minutes
    )
    native_id = int(args.native_run_id)
    native = api(repository, f"actions/runs/{native_id}")
    validate_identity(
        native,
        repository,
        native_id,
        native["run_attempt"],
        args.revision,
        ".github/workflows/native.yml",
    )
    write_json(records / "native-run.json", native)
    write_json(
        records / "native-jobs.json",
        pages(
            repository,
            f"actions/runs/{native_id}/attempts/{native['run_attempt']}/jobs",
            "jobs",
        ),
    )
    write_json(
        records / "native-artifacts.json",
        pages(repository, f"actions/runs/{native_id}/artifacts", "artifacts"),
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
    if (records / "completion-run.json").is_file():
        payload["completion_run"] = "completion-run.json"
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
