"""Append original completion evidence and replay unchanged durable campaign ZIPs.

Only a completed trusted campaign or the explicit evidence replay push may use
this adapter. Independent admission and budget decisions belong to the auditor.
The original index, source bundle, raw samples and unpublished assets stay intact.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import tempfile
import zipfile
from contextlib import contextmanager
from pathlib import Path

from index_public_performance import api, write
from retain_public_performance import (
    admit_release,
    archive_segments,
    download,
    require,
    upload,
)

CAMPAIGN_BRANCH = "qualification/issue-1998"
REPLAY_REF = "refs/heads/evidence/issue-1998-replay"
TRUSTED_REPOSITORY = "elong0527/yamaa"
INDEX_FILES = {
    "manifest.json",
    "run.json",
    "jobs.json",
    "artifacts.json",
    "source.bundle",
    "native-run.json",
    "native-jobs.json",
    "native-artifacts.json",
}


def campaign_identity(run, repository):
    require(
        run["repository"]["full_name"] == repository
        and run["head_repository"]["full_name"] == repository
        and run["event"] == "push"
        and run["head_branch"] == CAMPAIGN_BRANCH
        and run["path"] == ".github/workflows/performance.yml"
        and type(run["id"]) is int
        and run["id"] > 0
        and type(run["run_attempt"]) is int
        and run["run_attempt"] > 0
        and isinstance(run["head_sha"], str)
        and re.fullmatch(r"[0-9a-f]{40}", run["head_sha"]),
        "completion is not the trusted original characterization push",
    )
    return {
        "repository": repository,
        "source_revision": run["head_sha"],
        "run_id": run["id"],
        "run_attempt": run["run_attempt"],
    }


def selected_campaign(request):
    repository = os.environ["GITHUB_REPOSITORY"]
    require(repository == TRUSTED_REPOSITORY, "untrusted campaign repository")
    event = os.environ["GITHUB_EVENT_NAME"]
    if event == "workflow_run":
        require(
            request is None, "completion event cannot replace its original identity"
        )
        payload = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
        require(payload["action"] == "completed", "original campaign is still running")
        return campaign_identity(payload["workflow_run"], repository), True
    require(
        event == "push"
        and os.environ["GITHUB_REF"] == REPLAY_REF
        and request is not None,
        "replay requires the explicit trusted evidence push",
    )
    value = json.loads(request.read_text())
    require(
        set(value)
        == {"protocol", "repository", "source_revision", "run_id", "run_attempt"}
        and value["protocol"] == "public-performance-replay/1"
        and value["repository"] == repository
        and type(value["run_id"]) is int
        and value["run_id"] > 0
        and type(value["run_attempt"]) is int
        and value["run_attempt"] > 0
        and isinstance(value["source_revision"], str)
        and re.fullmatch(r"[0-9a-f]{40}", value["source_revision"]),
        "invalid versioned original campaign replay identity",
    )
    return {key: value[key] for key in value if key != "protocol"}, False


def completion_snapshot(identity, destination):
    run = api(
        f"repos/{identity['repository']}/actions/runs/{identity['run_id']}"
        f"/attempts/{identity['run_attempt']}"
    )
    # Keep the original response even when it proves a refusal. This file is a
    # separate receipt; the in-progress original run.json is never rewritten.
    write(destination, run)
    require(
        campaign_identity(run, identity["repository"]) == identity,
        "completion source/run/attempt differs from the original campaign",
    )
    require(
        run["status"] == "completed" and run["conclusion"] is not None,
        "original campaign has not completed",
    )
    return run


def release_receipt(identity, work):
    repository = identity["repository"]
    tag = f"public-performance-evidence-{identity['run_id']}-attempt-{identity['run_attempt']}"
    releases = [
        release
        for page in api(f"repos/{repository}/releases?per_page=100", True)
        for release in page
        if release["tag_name"] == tag
    ]
    require(len(releases) == 1, "missing/ambiguous original unpublished evidence draft")
    release = api(f"repos/{repository}/releases/{releases[0]['id']}")
    admit_release(release, tag, identity["source_revision"])
    name = f"run-{identity['run_id']}-attempt-{identity['run_attempt']}-retention.json"
    matches = [asset for asset in release["assets"] if asset["name"] == name]
    require(
        len(matches) == 1 and matches[0]["state"] == "uploaded",
        "original durable retention receipt is absent",
    )
    destination = work / "original-retention.json"
    download(
        repository, f"releases/assets/{matches[0]['id']}", destination, binary=True
    )
    require(
        destination.stat().st_size == matches[0]["size"],
        "truncated original retention receipt",
    )
    receipt = json.loads(destination.read_text())
    require(
        receipt["protocol"] == "public-performance-retention/1"
        and all(receipt[key] == value for key, value in identity.items())
        and receipt["release_id"] == release["id"]
        and receipt["release_label"] == tag
        and receipt["draft"] is True
        and receipt["retrieval_verified"] is True,
        "retention receipt differs from the verified original campaign",
    )
    return release, receipt, tag


def retained_zip(identity, release, receipt, artifact_id, artifact_name, destination):
    rows = sorted(
        [
            row
            for row in receipt["assets"]
            if row["original_artifact_id"] == artifact_id
        ],
        key=lambda row: row["byte_offset"],
    )
    require(bool(rows), "original artifact has no retained ZIP segments")
    original_size = rows[0]["original_zip_bytes"]
    count, offset = rows[0]["segment_count"], 0
    assets = {asset["id"]: asset for asset in release["assets"]}
    require(
        type(artifact_id) is int
        and artifact_id > 0
        and type(original_size) is int
        and original_size > 0
        and type(count) is int
        and count > 0
        and len(rows) == count,
        "incomplete original ZIP segment set",
    )
    with destination.open("xb") as output:
        for number, row in enumerate(rows):
            asset = assets.get(row["asset_id"])
            require(
                row["original_artifact_name"] == artifact_name
                and row["original_run_attempt"] == identity["run_attempt"]
                and row["original_zip_bytes"] == original_size
                and row["segment_count"] == count
                and type(row["byte_offset"]) is int
                and row["byte_offset"] == offset
                and type(row["asset_id"]) is int
                and row["asset_id"] > 0
                and type(row["bytes"]) is int
                and row["bytes"] > 0
                and row["bytes_equal"] is True
                and asset is not None
                and asset["state"] == "uploaded"
                and asset["name"] == row["name"]
                and asset["size"] == row["bytes"],
                "retained ZIP identity, byte order, size or attempt differs",
            )
            segment = destination.with_name(f"{destination.name}.part-{number}")
            download(
                identity["repository"],
                f"releases/assets/{asset['id']}",
                segment,
                binary=True,
            )
            require(
                segment.stat().st_size == row["bytes"], "truncated retained ZIP segment"
            )
            with segment.open("rb") as source:
                shutil.copyfileobj(source, output, 1_048_576)
            offset += row["bytes"]
            segment.unlink()
    require(
        offset == original_size == destination.stat().st_size,
        "reassembled original ZIP size differs",
    )


def extract_index(archive_path, destination):
    seen = set()
    with zipfile.ZipFile(archive_path) as archive:
        for member in archive.infolist():
            original_name = member.orig_filename
            require(
                "\0" not in original_name
                and original_name in INDEX_FILES
                and original_name not in seen
                and not member.is_dir()
                and (member.external_attr >> 16) & 0o170000 != 0o120000,
                "unexpected/unsafe/duplicate original index member",
            )
            seen.add(original_name)
            with (
                archive.open(member) as source,
                (destination / original_name).open("xb") as output,
            ):
                shutil.copyfileobj(source, output, 1_048_576)
    require(seen == INDEX_FILES, "incomplete original native/source campaign index")


def durable_provider(identity, release, receipt):
    @contextmanager
    def original_archive(entry, manifest_root):
        with tempfile.TemporaryDirectory(
            prefix="yamaa-durable-performance-"
        ) as temporary:
            root = Path(temporary)
            require(
                Path(entry["archive"]).name == entry["archive"],
                "unsafe original archive name",
            )
            try:
                retained_zip(
                    identity,
                    release,
                    receipt,
                    entry["artifact_id"],
                    entry["artifact_name"],
                    root / entry["archive"],
                )
                yield root
            except Exception:
                # Preserve refused retrieval bytes independently of the private
                # extraction lifecycle. Never change the durable original assets.
                shutil.copytree(
                    root,
                    manifest_root.parent / f"refused-artifact-{entry['artifact_id']}",
                )
                raise

    return original_archive


def completed_manifest(input_root, identity, completion):
    manifest = json.loads((input_root / "manifest.json").read_text())
    require(
        manifest["repository"] == identity["repository"]
        and manifest["source_revision"] == identity["source_revision"]
        and manifest["run"] == "run.json"
        and "completion_run" not in manifest,
        "original campaign index identity differs",
    )
    original_run = json.loads((input_root / "run.json").read_text())
    require(
        campaign_identity(original_run, identity["repository"]) == identity,
        "original index run differs from completed campaign",
    )
    shutil.copyfile(completion, input_root / "completion-run.json")
    manifest["completion_run"] = "completion-run.json"
    destination = input_root / "completed-manifest.json"
    write(destination, manifest)
    return destination


def finalize(args):
    identity, collection_only = selected_campaign(args.request)
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    require(
        head == os.environ["GITHUB_SHA"],
        "auditor checkout differs from trusted workflow source",
    )
    args.output.mkdir(parents=True, exist_ok=False)
    record = {
        "protocol": "public-performance-replay-receipt/1",
        **identity,
        "auditor_revision": head,
        "replay_run_id": int(os.environ["GITHUB_RUN_ID"]),
        "replay_run_attempt": int(os.environ["GITHUB_RUN_ATTEMPT"]),
        "scope": "collection-only" if collection_only else "numeric-budget-acceptance",
        "accepted": False,
        "status": "preparing",
    }
    write(args.output / "replay-receipt.json", record)
    completion_snapshot(identity, args.output / "completion-run.json")
    release, receipt, tag = release_receipt(identity, args.output)
    original_assets = {asset["name"]: asset["id"] for asset in release["assets"]}
    require(
        not subprocess.check_output(
            ["git", "ls-remote", "--tags", "origin", f"refs/tags/{tag}"]
        ),
        "unpublished evidence label is a Git tag",
    )
    indices = {
        row["original_artifact_id"]
        for row in receipt["assets"]
        if row["original_artifact_name"] == "performance-index"
        and row["original_run_attempt"] == identity["run_attempt"]
    }
    require(len(indices) == 1, "missing/ambiguous original campaign index")
    index_id = next(iter(indices))
    original_zip = args.output / "original-index.zip"
    retained_zip(
        identity, release, receipt, index_id, "performance-index", original_zip
    )
    input_root = args.output / "input"
    input_root.mkdir()
    extract_index(original_zip, input_root)
    manifest_path = completed_manifest(
        input_root, identity, args.output / "completion-run.json"
    )
    arguments = ["--input", str(manifest_path), "--output", str(args.output / "audit")]
    if collection_only:
        arguments.append("--check-collection")
    else:
        require(
            args.budgets.is_file(),
            "same-original numeric replay requires a reviewed budget file",
        )
        retained_budget = args.output / "performance-budgets.json"
        shutil.copyfile(args.budgets, retained_budget)
        arguments += ["--budgets", str(retained_budget), "--check"]
    import audit_public_performance

    code = audit_public_performance.main(
        arguments, entry_provider=durable_provider(identity, release, receipt)
    )
    report = json.loads((args.output / "audit/performance-audit.json").read_text())
    record.update(
        {
            "status": "pass" if code == 0 else "fail",
            "accepted": report["accepted"],
            "original_index_artifact_id": index_id,
            "release_id": release["id"],
            "run_completion": report.get("run_completion", {"status": "unavailable"}),
        }
    )
    write(args.output / "replay-receipt.json", record)
    archive_path = args.output.parent / (
        f"run-{identity['run_id']}-attempt-{identity['run_attempt']}-"
        f"replay-{record['replay_run_id']}-attempt-{record['replay_run_attempt']}-"
        f"auditor-{head}.zip"
    )
    with zipfile.ZipFile(
        archive_path, "x", compression=zipfile.ZIP_DEFLATED
    ) as archive:
        for path in sorted(args.output.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(args.output).as_posix())
    final = api(f"repos/{identity['repository']}/releases/{release['id']}")
    admit_release(final, tag, identity["source_revision"])
    storage = {
        "protocol": "public-performance-replay-storage/1",
        **record,
        "assets": [],
    }
    for segment, offset, count in archive_segments(archive_path):
        final, retained = upload(
            identity["repository"],
            final,
            tag,
            identity["source_revision"],
            segment,
            args.output,
        )
        retained.update(
            {
                "byte_offset": offset,
                "segment_count": count,
                "replay_zip_bytes": archive_path.stat().st_size,
            }
        )
        storage["assets"].append(retained)
        write(args.output / "replay-storage.json", storage)
        (args.output / f"retrieved-{retained['asset_id']}").unlink()
        if segment != archive_path:
            segment.unlink()
    final = api(f"repos/{identity['repository']}/releases/{release['id']}")
    admit_release(final, tag, identity["source_revision"])
    final_assets = {asset["name"]: asset["id"] for asset in final["assets"]}
    require(
        all(final_assets.get(name) == value for name, value in original_assets.items()),
        "original unpublished evidence asset identities changed",
    )
    storage_path = args.output / f"{archive_path.stem}-storage.json"
    write(storage_path, storage)
    upload(
        identity["repository"],
        final,
        tag,
        identity["source_revision"],
        storage_path,
        args.output,
    )
    require(
        not subprocess.check_output(
            ["git", "ls-remote", "--tags", "origin", f"refs/tags/{tag}"]
        ),
        "completion storage created a Git tag",
    )
    return code


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--request", type=Path)
    parser.add_argument(
        "--budgets",
        type=Path,
        default=Path("rust/qualification/performance-budgets.json"),
    )
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        return finalize(args)
    except (
        ValueError,
        KeyError,
        TypeError,
        OSError,
        subprocess.CalledProcessError,
        zipfile.BadZipFile,
    ) as error:
        args.output.mkdir(parents=True, exist_ok=True)
        write(
            args.output / "replay-refusal.json",
            {"accepted": False, "reason": str(error)},
        )
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
