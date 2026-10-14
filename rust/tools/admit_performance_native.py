"""Wait for complete native qualification of the exact performance source.

The explicit qualification branch starts collection before merging. A successful
sampled PR run, different source or expired distribution cannot admit collection.
API observations are retained without artifact digest fields.
"""

from __future__ import annotations

import argparse
import itertools
import json
import os
import re
import subprocess
import time
from pathlib import Path

PLATFORMS = ("ubuntu-24.04", "macos-15", "windows-latest")
PYTHONS = ("3.12", "3.13", "3.14")
FORMS = ("wheel", "source")


def required_jobs():
    return {
        "native-coverage", "portable-math-exact", "public-cohort-complete-runs",
        "native-run-evidence-audit",
        *(f"native-rust ({host}, 3.14)" for host in PLATFORMS),
        *(f"native-dist ({host}, 3.14)" for host in PLATFORMS),
        *(f"native-r ({host})" for host in PLATFORMS),
        *(f"native-qualify ({host}, {version}, {form})"
          for host, version, form in itertools.product(PLATFORMS, PYTHONS, FORMS)),
    }


def required_artifacts():
    return {
        "native-run-audit", "native-public-cohort",
        *(f"native-python-{host}-3.14" for host in PLATFORMS),
        *(f"native-r-{host}" for host in PLATFORMS),
        *(f"native-r-qualification-{host}" for host in PLATFORMS),
        *(f"native-qualification-{host}-{version}-{form}"
          for host, version, form in itertools.product(PLATFORMS, PYTHONS, FORMS)),
    }


def validate(run, jobs, artifacts, repository, revision, branch):
    if (run["repository"]["full_name"] != repository
            or run["head_repository"]["full_name"] != repository
            or run["head_sha"] != revision or run["head_branch"] != branch
            or run["path"] != ".github/workflows/native.yml"
            or run["event"] not in {"push", "workflow_dispatch"}
            or run["status"] != "completed" or run["conclusion"] != "success"):
        raise ValueError("native run does not qualify the exact trusted source")
    for name in required_jobs():
        matches = [job for job in jobs if job["name"] == name]
        if (len(matches) != 1 or matches[0]["status"] != "completed"
                or matches[0]["conclusion"] != "success"):
            raise ValueError("missing or unsuccessful full native job: " + name)
    for name in required_artifacts():
        matches = [item for item in artifacts if item["name"] == name]
        if (len(matches) != 1 or matches[0]["expired"]
                or matches[0]["size_in_bytes"] <= 0
                or matches[0].get("workflow_run", {}).get("head_sha") != revision):
            raise ValueError("missing, expired or different-source native artifact: " + name)


def api(endpoint, pages=False):
    command = ["gh", "api", endpoint]
    if pages:
        command += ["--paginate", "--slurp"]
    return json.loads(subprocess.check_output(command, text=True))


def retain(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=True) + "\n")


def admit(args):
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repository):
        raise ValueError("invalid repository")
    if not re.fullmatch(r"[0-9a-f]{40}", args.revision):
        raise ValueError("invalid exact source revision")
    if args.run_id and not args.run_id.isdecimal():
        raise ValueError("invalid native run ID")
    if args.wait_seconds < 0:
        raise ValueError("invalid native wait bound")
    args.output.mkdir(parents=True, exist_ok=False)
    deadline = time.monotonic() + args.wait_seconds
    prefix = f"repos/{args.repository}/actions"
    last = None
    while True:
        if args.run_id:
            run = api(f"{prefix}/runs/{args.run_id}")
        else:
            listing = api(f"{prefix}/workflows/native.yml/runs?head_sha={args.revision}&per_page=100")
            matching = [run for run in listing["workflow_runs"]
                        if run["head_sha"] == args.revision and run["head_branch"] == args.branch
                        and run["event"] in {"push", "workflow_dispatch"}
                        and run["head_repository"]["full_name"] == args.repository]
            run = max(matching, key=lambda run: run["id"], default=None)
        if run:
            retain(args.output / "native-run.json", run)
            if run["status"] == "completed":
                if run["conclusion"] != "success":
                    raise ValueError("exact-source native qualification failed: " + run["html_url"])
                job_pages = api(f"{prefix}/runs/{run['id']}/attempts/{run['run_attempt']}/jobs?per_page=100", True)
                artifact_pages = api(f"{prefix}/runs/{run['id']}/artifacts?per_page=100", True)
                jobs = [job for page in job_pages for job in page["jobs"]]
                artifacts = [{key: value for key, value in item.items() if key != "digest"}
                             for page in artifact_pages for item in page["artifacts"]]
                retain(args.output / "native-jobs.json", jobs)
                retain(args.output / "native-artifacts.json", artifacts)
                validate(run, jobs, artifacts, args.repository, args.revision, args.branch)
                if os.environ.get("GITHUB_OUTPUT"):
                    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
                        output.write(f"run-id={run['id']}\nrevision={run['head_sha']}\n")
                print("Admitted complete exact-source native run " + run["html_url"], flush=True)
                return run
        state = (run["id"], run["status"]) if run else (None, "not-created")
        if state != last:
            print("Waiting for exact-source full native qualification: " + str(state), flush=True)
            last = state
        if time.monotonic() >= deadline:
            raise ValueError("timed out waiting for complete exact-source native qualification")
        time.sleep(min(30, max(0, deadline - time.monotonic())))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--branch", required=True)
    parser.add_argument("--run-id", default="")
    parser.add_argument("--wait-seconds", type=int, default=7200)
    parser.add_argument("--output", type=Path, required=True)
    admit(parser.parse_args())
