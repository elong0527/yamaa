"""Transport immutable original campaigns through bounded trusted reader jobs.

Only the independent unit reader and aggregate admit raw observations or budgets.
This adapter retains original API snapshots and ZIP bytes without publishing or
replacing source evidence, and records source/reader/aggregate revisions apart.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import zipfile
from pathlib import Path, PurePosixPath, PureWindowsPath

import finalize_public_performance as original
from index_public_performance import api, without_digests, write
from retain_public_performance import (
    admit_release,
    archive_segments,
    download,
    require,
    upload,
)


def checkout():
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    require(head == os.environ["GITHUB_SHA"], "replay checkout differs from trusted source")
    return head


def request(path):
    if path is None:
        return None
    value = json.loads(path.read_text())
    if value.get("protocol") == "public-performance-replay/1":
        return value
    require(
        set(value) == {"protocol", "repository", "source_revision", "run_id", "run_attempt", "reader"}
        and value["protocol"] == "public-performance-replay/2",
        "invalid versioned original reader replay request",
    )
    reader = value["reader"]
    require(
        isinstance(reader, dict)
        and set(reader) == {"run_id", "run_attempt", "revision"}
        and type(reader["run_id"]) is int and reader["run_id"] > 0
        and type(reader["run_attempt"]) is int and reader["run_attempt"] > 0
        and isinstance(reader["revision"], str)
        and re.fullmatch(r"[0-9a-f]{40}", reader["revision"]),
        "invalid qualified original reader identity",
    )
    return value


def selected(path, output):
    value = request(path)
    selected_path = path
    if value is not None and value["protocol"] == "public-performance-replay/2":
        selected_path = output / "source-request.json"
        write(selected_path, {**{key: item for key, item in value.items() if key != "reader"},
                              "protocol": "public-performance-replay/1"})
    identity, _ = original.selected_campaign(selected_path)
    return identity, None if value is None else value.get("reader")


def prepare(args):
    args.output.mkdir(parents=True, exist_ok=False)
    head = checkout()
    identity, reader = selected(args.request, args.output)
    completion = args.output / "completion-run.json"
    run = original.completion_snapshot(identity, completion)
    require(run["conclusion"] == "success", "original collection/storage attempt failed")
    release, receipt, tag = original.release_receipt(identity, args.output)
    require(not subprocess.check_output(["git", "ls-remote", "--tags", "origin", f"refs/tags/{tag}"]),
            "unpublished source evidence label is a Git tag")
    write(args.output / "original-release.json", without_digests(release))
    index_ids = {row["original_artifact_id"] for row in receipt["assets"]
                 if row["original_artifact_name"] == "performance-index"}
    require(len(index_ids) == 1, "missing/ambiguous original source index")
    archive = args.output / "original-index.zip"
    original.retained_zip(identity, release, receipt, next(iter(index_ids)),
                          "performance-index", archive)
    input_root = args.output / "input"
    input_root.mkdir()
    original.extract_index(archive, input_root)
    manifest = original.completed_manifest(input_root, identity, completion)
    import audit_public_performance

    code = audit_public_performance.main([
        "--catalog", "--input", str(manifest), "--output", str(args.output / "catalog")
    ])
    require(code == 0, "original replay catalog refused")
    record = {"protocol": "public-performance-replay-preparation/1",
              "original": identity, "release_label": tag,
              "current_revision": head, "current_run_id": int(os.environ["GITHUB_RUN_ID"]),
              "current_run_attempt": int(os.environ["GITHUB_RUN_ATTEMPT"]),
              "mode": "units" if reader is None else "budgets", "reader": reader,
              "accepted": False}
    write(args.output / "preparation.json", record)
    if os.environ.get("GITHUB_OUTPUT"):
        catalog = json.loads((args.output / "catalog/performance-replay-catalog.json").read_text())
        with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
            output.write(f"mode={record['mode']}\n")
            units = [{"key": item["key"], "selector": item["selector"]}
                     for item in catalog["units"]]
            output.write(f"units={json.dumps(units, separators=(',', ':'))}\n")


def prepared(root):
    record = json.loads((root / "preparation.json").read_text())
    require(
        record["protocol"] == "public-performance-replay-preparation/1"
        and record["current_revision"] == checkout()
        and record["current_run_id"] == int(os.environ["GITHUB_RUN_ID"])
        and record["current_run_attempt"] == int(os.environ["GITHUB_RUN_ATTEMPT"]),
        "prepared replay source/run/attempt differs from this trusted workflow",
    )
    release = json.loads((root / "original-release.json").read_text())
    receipt = json.loads((root / "original-retention.json").read_text())
    admit_release(release, record["release_label"], record["original"]["source_revision"])
    return record, release, receipt


def unit(args):
    record, release, receipt = prepared(args.input)
    require(record["mode"] == "units", "budget replay cannot regenerate unit proofs")
    import audit_public_performance

    return audit_public_performance.main([
        "--input", str(args.input / "input/completed-manifest.json"),
        "--unit", str(args.selector), "--check-unit", "--output", str(args.output)
    ], entry_provider=original.durable_provider(record["original"], release, receipt))


def reader_prefix(reader):
    return f"reader-{reader['run_id']}-attempt-{reader['run_attempt']}-auditor-{reader['revision']}"


def snapshot_reader(root, repository, reader):
    prefix = f"repos/{repository}/actions/runs/{reader['run_id']}"
    write(root / "reader-run.json", api(f"{prefix}/attempts/{reader['run_attempt']}"))
    write(root / "reader-jobs.json", [job for page in api(
        f"{prefix}/attempts/{reader['run_attempt']}/jobs?per_page=100", True
    ) for job in page["jobs"]])
    write(root / "reader-artifacts.json", without_digests([artifact for page in api(
        f"{prefix}/artifacts?per_page=100", True
    ) for artifact in page["artifacts"]]))


def retain_file(repository, release, tag, revision, path, work):
    rows = []
    for segment, offset, count in archive_segments(path):
        release, row = upload(repository, release, tag, revision, segment, work)
        row.update(byte_offset=offset, segment_count=count, original_zip_bytes=path.stat().st_size)
        rows.append(row)
        (work / f"retrieved-{row['asset_id']}").unlink()
        if segment != path:
            segment.unlink()
    return release, rows


def reassemble(rows, repository, release, path):
    require(bool(rows), "empty verified metadata segment set")
    rows = sorted(rows, key=lambda row: row["byte_offset"])
    size, count = rows[0]["original_zip_bytes"], rows[0]["segment_count"]
    require(type(size) is int and size > 0 and type(count) is int and count > 0
            and len(rows) == count, "incomplete verified metadata segments")
    assets = {asset["id"]: asset for asset in release["assets"]}
    offset, seen = 0, set()
    with path.open("xb") as output:
        for number, row in enumerate(rows):
            asset = assets.get(row["asset_id"])
            require(type(row["asset_id"]) is int and row["asset_id"] > 0
                    and row["asset_id"] not in seen and asset is not None
                    and type(row["byte_offset"]) is int and row["byte_offset"] == offset
                    and type(row["segment_count"]) is int and row["segment_count"] == count
                    and row["original_zip_bytes"] == size and row["bytes_equal"] is True
                    and type(row["bytes"]) is int and row["bytes"] > 0
                    and asset["size"] == row["bytes"] and asset["name"] == row["name"]
                    and asset["state"] == "uploaded", "metadata segment identity/order differs")
            seen.add(row["asset_id"])
            part = path.with_name(path.name + f".part-{number}")
            download(repository, f"releases/assets/{row['asset_id']}", part, binary=True)
            require(part.stat().st_size == row["bytes"], "truncated verified metadata segment")
            with part.open("rb") as source:
                shutil.copyfileobj(source, output, 1_048_576)
            offset += row["bytes"]
            part.unlink()
    require(path.stat().st_size == offset == size, "reassembled metadata size differs")


def execution_receipt(record, reader, output, code):
    report = json.loads((output / "audit/performance-audit.json").read_text())
    value = {"protocol": "public-performance-aggregate-execution/1",
             "original": record["original"], "reader": reader,
             "aggregate": {"revision": record["current_revision"],
                           "run_id": record["current_run_id"],
                           "run_attempt": record["current_run_attempt"],
                           "completion": "pending-original-terminal-API-verification"},
             "scope": "collection-only" if record["mode"] == "units" else "numeric-evidence",
             "exit_code": code, "numeric_evidence_accepted": report["accepted"],
             "completed_workflow_acceptance": False}
    write(output / "aggregate-execution.json", value)
    return value


def source_asset_identity(release):
    """Freeze the preparation-time draft identity before additive uploads."""
    return {"release_id": release["id"],
            "assets": {asset["name"]: asset["id"] for asset in release["assets"]}}


def unchanged_source_assets(record, before):
    identity = record["original"]
    release = api(f"repos/{identity['repository']}/releases/{before['release_id']}")
    admit_release(release, record["release_label"], identity["source_revision"])
    assets = {asset["name"]: asset["id"] for asset in release["assets"]}
    require(release["id"] == before["release_id"]
            and all(assets.get(name) == identifier for name, identifier in before["assets"].items()),
            "original unpublished evidence asset identities changed")
    require(not subprocess.check_output(["git", "ls-remote", "--tags", "origin",
                                         f"refs/tags/{record['release_label']}"]),
            "replay storage created a Git tag")
    return release


def aggregate(args):
    import aggregate_public_performance as independent

    record, release, _ = prepared(args.input)
    original_assets = source_asset_identity(release)
    args.output.mkdir(parents=True, exist_ok=False)
    reader = record["reader"] or {"run_id": record["current_run_id"],
                                 "run_attempt": record["current_run_attempt"],
                                 "revision": record["current_revision"]}
    identity = record["original"]
    catalog = json.loads((args.input / "catalog/performance-replay-catalog.json").read_text())
    # Keep a self-contained immutable original index, including both snapshots.
    shutil.copytree(args.input / "input", args.output / "original")
    if record["mode"] == "units":
        snapshot_reader(args.output, identity["repository"], reader)
        run, jobs, artifacts = [json.loads((args.output / name).read_text())
                                for name in ("reader-run.json", "reader-jobs.json", "reader-artifacts.json")]
        independent.validate_reader_run(run, identity["repository"], reader["revision"])
        entries = []
        for item in catalog["units"]:
            matches = []
            for artifact in artifacts:
                if artifact["name"] != "performance-unit-" + item["key"]:
                    continue
                entry = {"selector": item["selector"], "artifact_id": artifact["id"],
                         "artifact_name": artifact["name"], "archive": artifact["name"] + ".zip"}
                try:
                    independent.admit_fragment_artifact(entry, run, jobs, artifacts, item)
                except (ValueError, KeyError, TypeError):
                    continue
                matches.append(entry)
            require(len(matches) == 1, "missing/ambiguous successful original reader unit")
            entries.append(matches[0])
        payload = {"protocol": independent.PROTOCOL,
                   "original_manifest": "original/completed-manifest.json",
                   "reader_run": "reader-run.json", "reader_jobs": "reader-jobs.json",
                   "reader_artifacts": "reader-artifacts.json", "fragments": entries}
        write(args.output / "aggregate-input.json", payload)
        # Download/retain each authentic fragment ZIP before aggregation. All
        # source draft assets are additive and independently retrieved by bytes.
        retained = {"protocol": "public-performance-reader-retention/1",
                    "original": identity, "reader": reader, "assets": []}
        for entry in entries:
            path = args.output / (reader_prefix(reader) + f"-artifact-{entry['artifact_id']}.zip")
            download(identity["repository"], f"actions/artifacts/{entry['artifact_id']}/zip", path)
            (artifact,) = [value for value in artifacts if value["id"] == entry["artifact_id"]]
            require(path.stat().st_size == artifact["size_in_bytes"], "truncated original reader ZIP")
            release, rows = retain_file(identity["repository"], release,
                                        record["release_label"], identity["source_revision"], path, args.output)
            for row in rows:
                row.update(original_artifact_id=entry["artifact_id"],
                           original_artifact_name=entry["artifact_name"],
                           original_run_attempt=reader["run_attempt"])
            retained["assets"].extend(rows)
            write(args.output / "reader-retention.json", retained)
            path.unlink()
        provider = original.durable_provider(
            {**identity, "run_attempt": reader["run_attempt"]}, release, retained)
        arguments = ["--input", str(args.output / "aggregate-input.json"),
                     "--reviewed-auditor-revision", reader["revision"],
                     "--output", str(args.output / "audit"), "--check-collection"]
        code = independent.main(arguments, entry_provider=provider)
        execution_receipt(record, reader, args.output, code)
        # Store authentic snapshots and qualified/refused aggregate unchanged;
        # current reader run is still in progress until this job completes.
        metadata = args.output.parent / (reader_prefix(reader) + "-metadata.zip")
        with zipfile.ZipFile(metadata, "x", compression=zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(args.output.rglob("*")):
                if path.is_file():
                    archive.write(path, path.relative_to(args.output).as_posix())
        release, rows = retain_file(identity["repository"], release, record["release_label"],
                                    identity["source_revision"], metadata, args.output)
        storage_path = args.output / (reader_prefix(reader) + "-metadata-storage.json")
        write(storage_path, {"protocol": "public-performance-reader-metadata-storage/1",
                             "original": identity, "reader": reader,
                             "archive_name": metadata.name, "assets": rows})
        upload(identity["repository"], release, record["release_label"],
               identity["source_revision"], storage_path, args.output)
        unchanged_source_assets(record, original_assets)
        return code
    require(args.budgets is not None and args.budgets.is_file(), "reviewed budgets are required")
    # Reuse the SAME authentic qualified fragments, never run unit capture just
    # to add a budget decision. Both original snapshots remain unchanged.
    prefix = reader_prefix(reader)
    name = prefix + "-metadata-storage.json"
    matches = [asset for asset in release["assets"] if asset["name"] == name]
    require(len(matches) == 1 and matches[0]["state"] == "uploaded", "original reader metadata storage receipt absent")
    path = args.output / name
    download(identity["repository"], f"releases/assets/{matches[0]['id']}", path, binary=True)
    require(path.stat().st_size == matches[0]["size"], "truncated original reader metadata storage receipt")
    storage = json.loads(path.read_text())
    require(storage["protocol"] == "public-performance-reader-metadata-storage/1"
            and storage["original"] == identity and storage["reader"] == reader
            and storage["archive_name"] == prefix + "-metadata.zip",
            "original reader metadata storage source/attempt differs")
    archive = args.output / storage["archive_name"]
    reassemble(storage["assets"], identity["repository"], release, archive)
    metadata_root = args.output / "reader-original"
    metadata_root.mkdir()
    safe_metadata(archive, metadata_root)
    retained = json.loads((metadata_root / "reader-retention.json").read_text())
    require(retained["original"] == identity and retained["reader"] == reader,
            "retained reader source/run/attempt differs")
    completion = api(f"repos/{identity['repository']}/actions/runs/{reader['run_id']}/attempts/{reader['run_attempt']}")
    write(metadata_root / "reader-completion-run.json", completion)
    payload = json.loads((metadata_root / "aggregate-input.json").read_text())
    payload["reader_completion_run"] = "reader-completion-run.json"
    write(metadata_root / "completed-aggregate-input.json", payload)
    shutil.copyfile(args.budgets, args.output / "performance-budgets.json")
    provider = original.durable_provider(
        {**identity, "run_attempt": reader["run_attempt"]}, release, retained)
    code = independent.main([
        "--input", str(metadata_root / "completed-aggregate-input.json"),
        "--reviewed-auditor-revision", reader["revision"], "--budgets", str(args.budgets),
        "--output", str(args.output / "audit"), "--check"
    ], entry_provider=provider)
    execution_receipt(record, reader, args.output, code)
    numeric = args.output.parent / (
        f"aggregate-{record['current_run_id']}-attempt-{record['current_run_attempt']}-"
        f"revision-{record['current_revision']}-numeric.zip")
    with zipfile.ZipFile(numeric, "x", compression=zipfile.ZIP_DEFLATED) as archive:
        for name in ("aggregate-execution.json", "performance-budgets.json",
                     "audit/performance-audit.json", "audit/performance-audit.md"):
            archive.write(args.output / name, name)
        archive.write(metadata_root / "reader-completion-run.json", "reader-completion-run.json")
        archive.write(args.output / "original/completion-run.json", "source-completion-run.json")
    release, rows = retain_file(identity["repository"], release, record["release_label"],
                                identity["source_revision"], numeric, args.output)
    storage_path = args.output / (numeric.stem + "-storage.json")
    write(storage_path, {"protocol": "public-performance-numeric-storage/1",
                         "original": identity, "reader": reader,
                         "aggregate": {"revision": record["current_revision"],
                                       "run_id": record["current_run_id"],
                                       "run_attempt": record["current_run_attempt"]},
                         "release_id": release["id"], "release_label": record["release_label"],
                         "archive_name": numeric.name, "assets": rows,
                         "completed_workflow_acceptance": False})
    upload(identity["repository"], release, record["release_label"],
           identity["source_revision"], storage_path, args.output)
    unchanged_source_assets(record, original_assets)
    return code


def retain_refusal(args):
    """Append failed reader metadata/actual retrieval bytes without qualifying it."""
    record, release, _ = prepared(args.input)
    original_assets = source_asset_identity(release)
    identity = record["original"]
    release = api(f"repos/{identity['repository']}/releases/{release['id']}")
    admit_release(release, record["release_label"], identity["source_revision"])
    stem = (f"refused-reader-{record['current_run_id']}-attempt-{record['current_run_attempt']}-"
            f"revision-{record['current_revision']}")
    archive_path = args.output.parent / (stem + ".zip")
    with zipfile.ZipFile(archive_path, "x", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(args.output.rglob("*")):
            if path.is_file():
                # Only authored metadata/reports and original failed transport
                # bytes live here; the raw SDK never executes in a reader job.
                archive.write(path, path.relative_to(args.output).as_posix())
        archive.write(args.input / "preparation.json", "preparation.json")
    release, rows = retain_file(identity["repository"], release, record["release_label"],
                                identity["source_revision"], archive_path, args.output)
    storage_path = args.output / (stem + "-storage.json")
    write(storage_path, {"protocol": "public-performance-reader-refusal-storage/1",
                         "original": identity,
                         "reader": record["reader"] or {
                             "revision": record["current_revision"],
                             "run_id": record["current_run_id"],
                             "run_attempt": record["current_run_attempt"]},
                         "replay": {"revision": record["current_revision"],
                                    "run_id": record["current_run_id"],
                                    "run_attempt": record["current_run_attempt"]},
                         "release_id": release["id"], "release_label": record["release_label"],
                         "archive_name": archive_path.name, "assets": rows, "accepted": False})
    upload(identity["repository"], release, record["release_label"],
           identity["source_revision"], storage_path, args.output)
    unchanged_source_assets(record, original_assets)


def safe_metadata(path, destination):
    seen = set()
    allowed = {"aggregate-input.json", "reader-run.json", "reader-jobs.json",
               "reader-artifacts.json", "reader-retention.json", "aggregate-execution.json"}
    members = allowed | {"original/" + name for name in original.INDEX_FILES}
    members |= {"original/completion-run.json", "original/completed-manifest.json",
                "audit/performance-audit.json", "audit/performance-audit.md"}
    with zipfile.ZipFile(path) as archive:
        for member in archive.infolist():
            name = member.orig_filename
            relative = PurePosixPath(name)
            require(
                name not in seen and "\0" not in name and "\\" not in name
                and not relative.is_absolute() and ".." not in relative.parts
                and not any(PureWindowsPath(part).drive for part in relative.parts)
                and name in members
                and not member.is_dir()
                and (member.external_attr >> 16) & 0o170000 != 0o120000,
                "unsafe original reader metadata member",
            )
            seen.add(name)
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            with archive.open(member) as source, target.open("xb") as output:
                shutil.copyfileobj(source, output, 1_048_576)
    required = members - {"audit/performance-audit.md"}
    require(required.issubset(seen), "missing original reader/native/source/API proof snapshots")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("prepare", "unit", "aggregate"), required=True)
    parser.add_argument("--input", type=Path)
    parser.add_argument("--request", type=Path)
    parser.add_argument("--selector", type=Path)
    parser.add_argument("--budgets", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if args.mode == "prepare":
            prepare(args)
            return 0
        require(args.input is not None, "unit/aggregate replay requires prepared input")
        if args.mode == "unit":
            require(args.selector is not None, "unit replay requires its exact selector")
            return unit(args)
        return aggregate(args)
    except (ValueError, KeyError, TypeError, OSError, subprocess.CalledProcessError, zipfile.BadZipFile) as error:
        args.output.mkdir(parents=True, exist_ok=True)
        write(args.output / "replay-refusal.json", {"accepted": False, "reason": str(error)})
        if args.mode == "aggregate" and args.input is not None:
            try:
                retain_refusal(args)
            except (ValueError, KeyError, TypeError, OSError, subprocess.CalledProcessError, zipfile.BadZipFile) as retention_error:
                write(args.output / "refusal-storage-error.json",
                      {"accepted": False, "reason": str(retention_error)})
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
