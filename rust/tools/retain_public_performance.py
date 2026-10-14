"""Retain original performance ZIPs as additive unpublished evidence assets.

Only the explicit trusted campaign pushes can write. No tag, publication,
replacement or deletion operation is used; every uploaded asset is downloaded
again and compared directly with its original Actions bytes.
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import re
import shutil
import subprocess
import urllib.request
from pathlib import Path

from index_public_performance import (
    R_VARIANTS,
    api,
    producer_job,
    tuples,
    without_digests,
    write,
)
from prepare_r_rollback import StripRedirectCredentials

TRUSTED_REFS = {
    "refs/heads/qualification/issue-1998",
    "refs/heads/qualification/issue-2142-frontier",
}
ASSET_BYTES = 1024 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def admit_release(release, tag, revision):
    require(
        release["tag_name"] == tag
        and release["target_commitish"] == revision
        and release["draft"] is True
        and release["published_at"] is None,
        "evidence storage differs from the unpublished exact-source draft",
    )
    names = [asset["name"] for asset in release["assets"]]
    require(len(names) == len(set(names)), "ambiguous existing evidence asset")


def download(repository, endpoint, destination, binary=False):
    request = urllib.request.Request(
        f"https://api.github.com/repos/{repository}/{endpoint}",
        headers={
            "Authorization": "Bearer " + os.environ["GH_TOKEN"],
            "Accept": "application/octet-stream"
            if binary
            else "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
        },
        method="GET",
    )
    opener = urllib.request.build_opener(StripRedirectCredentials())
    with opener.open(request, timeout=90) as source, destination.open("xb") as output:
        shutil.copyfileobj(source, output, length=1024 * 1024)


def equal_files(left, right):
    if left.stat().st_size != right.stat().st_size:
        return False
    with left.open("rb") as original, right.open("rb") as retained:
        while True:
            chunk = original.read(1024 * 1024)
            if chunk != retained.read(1024 * 1024):
                return False
            if not chunk:
                return True


def verify_original_download(path, artifact):
    require(
        path.stat().st_size == artifact["size_in_bytes"],
        "original Actions ZIP download is truncated or differs from its recorded size",
    )


def producer_name(name):
    for lane, artifact_name in tuples():
        if name == artifact_name:
            return producer_job(lane)
        if lane["host"] == "r":
            for variant in R_VARIANTS:
                if name == f"{artifact_name}-{variant}":
                    return producer_job(lane, variant)
    return {
        "performance-native-admission": "Admit exact-source full native qualification",
        "performance-index": "Retain exact original campaign index",
        "performance-audit": "Independent retained campaign replay",
        "performance-audit-record": "Independent retained campaign replay",
    }.get(name)


def original_attempt(artifact, jobs):
    created = datetime.datetime.fromisoformat(
        artifact["created_at"].replace("Z", "+00:00")
    )
    matches = []
    for job in jobs:
        if job["name"] != producer_name(artifact["name"]) or not job.get(
            "completed_at"
        ):
            continue
        start = datetime.datetime.fromisoformat(
            job["started_at"].replace("Z", "+00:00")
        )
        end = datetime.datetime.fromisoformat(
            job["completed_at"].replace("Z", "+00:00")
        )
        if start <= created <= end:
            matches.append(job)
    return matches[0]["run_attempt"] if len(matches) == 1 else None


def archive_segments(original, maximum=ASSET_BYTES):
    """Preserve contiguous original bytes below GitHub's per-asset size limit."""
    size = original.stat().st_size
    if size <= maximum:
        yield original, 0, 1
        return
    count = (size + maximum - 1) // maximum
    with original.open("rb") as source:
        for number in range(count):
            path = original.with_name(f"{original.name}.part-{number + 1:04d}")
            remaining = min(maximum, size - number * maximum)
            with path.open("xb") as target:
                while remaining:
                    chunk = source.read(min(1024 * 1024, remaining))
                    require(bool(chunk), "original ZIP ended during byte segmentation")
                    target.write(chunk)
                    remaining -= len(chunk)
            yield path, number * maximum, count


def upload(repository, release, tag, revision, path, work):
    admit_release(release, tag, revision)
    matches = [asset for asset in release["assets"] if asset["name"] == path.name]
    reused = bool(matches)
    if not matches:
        # Omitting --clobber preserves all existing asset identities. This only
        # uploads to the already admitted draft; it cannot publish a release.
        subprocess.run(
            [
                "gh",
                "release",
                "upload",
                tag,
                str(path),
                "--repo",
                repository,
            ],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        release = api(f"repos/{repository}/releases/{release['id']}")
        admit_release(release, tag, revision)
        matches = [asset for asset in release["assets"] if asset["name"] == path.name]
    require(len(matches) == 1, "missing or ambiguous retained evidence asset")
    asset = matches[0]
    require(asset["state"] == "uploaded", "retained asset upload is incomplete")
    retrieved = work / f"retrieved-{asset['id']}"
    download(repository, f"releases/assets/{asset['id']}", retrieved, binary=True)
    require(equal_files(path, retrieved), "retained bytes differ; never overwrite")
    return release, {
        "asset_id": asset["id"],
        "name": asset["name"],
        "bytes": path.stat().st_size,
        "bytes_equal": True,
        "reused_without_overwrite": reused,
        "authenticated_api_reference": asset["url"],
    }


def retain(args):
    require(
        os.environ.get("GITHUB_EVENT_NAME") == "push"
        and os.environ.get("GITHUB_REF") in TRUSTED_REFS
        and os.environ.get("GITHUB_REPOSITORY") == args.repository
        and os.environ.get("GITHUB_SHA") == args.revision
        and os.environ.get("GITHUB_RUN_ID") == str(args.run_id)
        and os.environ.get("GITHUB_RUN_ATTEMPT") == str(args.attempt),
        "evidence uploads require the original trusted campaign push",
    )
    require(
        re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repository)
        and re.fullmatch(r"[0-9a-f]{40}", args.revision)
        and args.run_id > 0
        and args.attempt > 0,
        "invalid exact campaign identity",
    )
    args.output.mkdir(parents=True, exist_ok=False)
    tag = f"public-performance-evidence-{args.run_id}-attempt-{args.attempt}"
    # A draft uses a label in tag_name; GitHub creates its tag only upon
    # publication. Refuse an existing Git tag and verify again after storage.
    tag_command = ["git", "ls-remote", "--tags", "origin", f"refs/tags/{tag}"]
    require(not subprocess.check_output(tag_command), "evidence label is a Git tag")
    releases = [
        release
        for page in api(f"repos/{args.repository}/releases?per_page=100", True)
        for release in page
        if release["tag_name"] == tag
    ]
    require(len(releases) <= 1, "ambiguous unpublished evidence draft")
    if releases:
        release = releases[0]
    else:
        # Create an unpublished storage container through the releases endpoint.
        # No Git refs endpoint, release edit or publication command is used.
        body = {
            "tag_name": tag,
            "target_commitish": args.revision,
            "name": f"Public performance evidence: run {args.run_id}, attempt {args.attempt}",
            "draft": True,
            "prerelease": True,
            "body": (
                f"Original migration evidence for {args.revision}, performance run "
                f"{args.run_id}, attempt {args.attempt}. Raw failures and partial "
                "collections are retained. Storage does not accept budgets or "
                "approve transition/release publication. Keep unpublished."
            ),
        }
        release = json.loads(
            subprocess.check_output(
                [
                    "gh",
                    "api",
                    f"repos/{args.repository}/releases",
                    "--method",
                    "POST",
                    "--input",
                    "-",
                ],
                input=json.dumps(body),
                text=True,
            )
        )
    admit_release(release, tag, args.revision)
    original_ids = {asset["name"]: asset["id"] for asset in release["assets"]}
    pages = api(
        f"repos/{args.repository}/actions/runs/{args.run_id}/artifacts?per_page=100",
        True,
    )
    artifacts = without_digests(
        [
            artifact
            for page in pages
            for artifact in page["artifacts"]
            if producer_name(artifact["name"]) is not None
        ]
    )
    write(args.output / "original-artifacts.json", artifacts)
    require(bool(artifacts), "no original performance evidence was retained")
    jobs = [
        job
        for attempt in range(1, args.attempt + 1)
        for page in api(
            f"repos/{args.repository}/actions/runs/{args.run_id}/attempts/{attempt}/jobs?per_page=100",
            True,
        )
        for job in page["jobs"]
    ]
    write(args.output / "original-jobs.json", jobs)
    receipt = {
        "protocol": "public-performance-retention/1",
        "repository": args.repository,
        "source_revision": args.revision,
        "run_id": args.run_id,
        "run_attempt": args.attempt,
        "release_id": release["id"],
        "release_label": tag,
        "draft": True,
        "accepted": False,
        "scope": "durable authenticated original artifact bytes; no budget acceptance",
        "archive_reassembly": "group by original_artifact_id and concatenate verified asset bytes in byte_offset order; no recompression",
        "existing_assets_preserved": original_ids,
        "assets": [],
    }
    write(args.output / "retention.json", receipt)
    for artifact in sorted(artifacts, key=lambda item: item["id"]):
        require(
            artifact["expired"] is False
            and artifact["size_in_bytes"] > 0
            and artifact["workflow_run"]["id"] == args.run_id
            and artifact["workflow_run"]["head_sha"] == args.revision
            and re.fullmatch(r"[A-Za-z0-9_.-]+", artifact["name"]),
            "original artifact is expired, empty or belongs to another source/run",
        )
        producing_attempt = original_attempt(artifact, jobs)
        filename = (
            f"run-{args.run_id}-attempt-{producing_attempt or 'unavailable'}-artifact-{artifact['id']}-"
            f"{artifact['name']}.zip"
        )
        original = args.output / filename
        download(args.repository, f"actions/artifacts/{artifact['id']}/zip", original)
        verify_original_download(original, artifact)
        for segment, offset, count in archive_segments(original):
            release, retained = upload(
                args.repository, release, tag, args.revision, segment, args.output
            )
            retained.update(
                {
                    "original_artifact_id": artifact["id"],
                    "original_artifact_name": artifact["name"],
                    "original_artifact_created_at": artifact["created_at"],
                    "original_run_attempt": producing_attempt,
                    "original_zip_bytes": original.stat().st_size,
                    "byte_offset": offset,
                    "segment_count": count,
                }
            )
            receipt["assets"].append(retained)
            write(args.output / "retention.json", receipt)
            (args.output / f"retrieved-{retained['asset_id']}").unlink()
            if segment != original:
                segment.unlink()
        # Direct equality was verified; keep the compact receipt locally.
        original.unlink()
    final = api(f"repos/{args.repository}/releases/{release['id']}")
    admit_release(final, tag, args.revision)
    final_ids = {asset["name"]: asset["id"] for asset in final["assets"]}
    require(
        all(final_ids.get(name) == identity for name, identity in original_ids.items()),
        "existing evidence asset identities changed",
    )
    require(not subprocess.check_output(tag_command), "storage created a Git tag")
    receipt["retrieval_verified"] = True
    write(args.output / "retention.json", receipt)
    # Give the durable draft its own compact manifest, using a distinct name.
    manifest_path = (
        args.output / f"run-{args.run_id}-attempt-{args.attempt}-retention.json"
    )
    write(manifest_path, receipt)
    upload(args.repository, final, tag, args.revision, manifest_path, args.output)
    return receipt


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--run-id", type=int, required=True)
    parser.add_argument("--attempt", type=int, required=True)
    parser.add_argument("--output", type=Path, required=True)
    retain(parser.parse_args())
