"""Preserve reviewed prior rollback bytes as additive unpublished draft assets."""

import argparse
import importlib
import io
import json
import os
import shutil
import subprocess
import sys
import urllib.error
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[2]
RECORD = ROOT / "rust/qualification/prior-python-rollback-run.json"
BUNDLE = "prior-python-rollback-38460509078.zip"
MANIFEST = "prior-python-rollback-38460509078.json"
TRUSTED_REF = "refs/heads/qualification/issue-1999"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def extract(data, destination):
    """Read only regular, uniquely named, relative members from hosted artifacts."""
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        names = archive.namelist()
        require(len(names) == len(set(names)), "duplicate artifact member")
        for member in archive.infolist():
            path = PurePosixPath(member.filename)
            require(
                not path.is_absolute() and ".." not in path.parts
                and "\\" not in member.filename,
                "unsafe artifact member",
            )
            require((member.external_attr >> 16) & 0o170000 != 0o120000,
                    "artifact symlink refused")
            target = destination.joinpath(*path.parts)
            if member.is_dir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(archive.read(member))


def admit_run(run, artifacts, expected):
    require(
        run["id"] == expected["run_id"] and run["head_sha"] == expected["head"]
        and run["run_attempt"] == expected["run_attempt"]
        and run["status"] == "completed" and run["conclusion"] == "success",
        "prior rehearsal run/head/attempt did not pass",
    )
    actual = {item["name"]: item for item in artifacts}
    require(len(actual) == len(artifacts) == len(expected["artifacts"]),
            "prior artifact inventory differs")
    for approved in expected["artifacts"]:
        item = actual.get(approved["name"], {})
        require(
            item.get("id") == approved["id"]
            and item.get("size_in_bytes") == approved["bytes"]
            and item.get("expired") is False
            and item.get("workflow_run", {}).get("id") == expected["run_id"]
            and item["workflow_run"].get("head_sha") == expected["head"],
            "prior artifact identity differs: " + approved["name"],
        )


def download_artifact(artifact_id, prior):
    # The Actions ZIP endpoint requires the JSON API media type on its initial
    # redirect; release assets instead require application/octet-stream.
    request = urllib.request.Request(
        f"https://api.github.com/repos/{prior.REPOSITORY}/actions/artifacts/{artifact_id}/zip",
        headers={"Authorization": "Bearer " + os.environ["GH_TOKEN"],
                 "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28"},
        method="GET",
    )
    try:
        with urllib.request.build_opener(prior.StripRedirectCredentials()).open(request, timeout=90) as response:
            return response.read()
    except urllib.error.HTTPError as error:
        raise RuntimeError(f"GitHub artifact retrieval failed with HTTP {error.code}") from None
    except urllib.error.URLError:
        raise RuntimeError("GitHub artifact retrieval failed during transport") from None


def load_prior_tools(directory, expected):
    # The original audit remains the authority for the historical replay semantics.
    # Compare its exact retained source before importing it into this verifier.
    for name in ("replay.py", "audit.py"):
        original = subprocess.check_output([
            "git", "show", f'{expected["head"]}:.rollback-python/{name}',
        ], cwd=ROOT)
        require((directory / name).read_bytes() == original,
                "historical rollback verifier source differs")
    sys.path.insert(0, str(directory))
    return importlib.import_module("replay"), importlib.import_module("audit")


def assemble(raw, assets, output, prior, audit, expected):
    extracted = output / "rehearsal-artifacts"
    extracted.mkdir()
    for item in expected["artifacts"]:
        data = (raw / (item["name"] + ".zip")).read_bytes()
        require(len(data) == item["bytes"], "original artifact ZIP length differs")
        destination = output / "original-summary" if item["name"] == "prior-python-rollback-summary" else extracted / item["name"]
        extract(data, destination)
    source = prior.archive_entries(subprocess.check_output([
        "git", "archive", "--prefix=source/", prior.SOURCE,
    ], cwd=ROOT), "source/")
    retained = prior.archive_entries(
        (assets / prior.REPOSITORY_ARCHIVE).read_bytes(), "yamaa-reference-845a91ff/",
    )
    require(source == retained, "prior source entry bytes/modes/symlinks differ")
    prior.check_packaged_sources(assets)
    require(
        (extracted / "prior-wheel-ubuntu-24.04" / prior.LINUX_WHEEL).read_bytes()
        == (assets / prior.LINUX_WHEEL).read_bytes(),
        "retained Linux wheel bytes differ",
    )
    identity = {"run_id": str(expected["run_id"]), "run_attempt": str(expected["run_attempt"]), "preparation_head": expected["head"]}
    truth = {name: entry["bytes"] for name, entry in source.items() if entry["kind"] == "file"}
    lanes = audit.audit_receipts(extracted, truth,
                                (assets / "baseline-dependencies.txt").read_text().splitlines(), identity)
    summary = json.loads((output / "original-summary/summary.json").read_text())
    require(all(summary[key] == value for key, value in identity.items())
            and summary["rehearsal"] == "pass" and summary["required_lanes"] == 18
            and audit.exactly(summary["qualified_lanes"], lanes),
            "original complete rehearsal summary differs")
    uploads = output / "uploads"
    uploads.mkdir()
    # Fixed metadata makes retries reuse exactly the same bundle; each embedded
    # original Actions ZIP remains byte-for-byte unchanged, including raw tables.
    with zipfile.ZipFile(uploads / BUNDLE, "w", compression=zipfile.ZIP_STORED) as bundle:
        for item in sorted(expected["artifacts"], key=lambda value: value["name"]):
            name = item["name"] + ".zip"
            member = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            member.external_attr = 0o100644 << 16
            bundle.writestr(member, (raw / name).read_bytes())
    wheels = []
    for platform in prior.PLATFORMS:
        wheel = next((extracted / f"prior-wheel-{platform}").glob("*.whl"))
        if platform != "ubuntu-24.04":
            shutil.copyfile(wheel, uploads / wheel.name)
        wheels.append({"platform": platform, "name": wheel.name, "bytes": wheel.stat().st_size})
    prior.write_json(uploads / MANIFEST, {
        "schema_version": "prior-python-rollback-retention/1",
        "repository": prior.REPOSITORY, "source_revision": prior.SOURCE,
        "package_version": prior.PACKAGE_VERSION, "core_version": prior.CORE_VERSION,
        "rehearsal_run": expected["run_id"], "rehearsal_head": expected["head"],
        "release_id": prior.RELEASE, "release_tag": prior.TAG,
        "original_artifacts": expected["artifacts"], "bundle": BUNDLE,
        "qualified_lanes": lanes, "platform_wheels": wheels,
        "source_entries_compared": len(source), "retained_linux_wheel_bytes_equal": True,
        "scope": "18 prior wheel/source installations; three authored domains and six held Saves per lane",
        "publication": "unpublished draft; no release publication or overwrite",
        "pending_gates": ["full producer graph Save baseline", "published study lock restoration", "combined transition approval"],
    })
    return uploads


def retain(uploads, output, prior):
    require(os.environ.get("GITHUB_REF") == TRUSTED_REF
            and os.environ.get("GITHUB_EVENT_NAME") == "push",
            "draft uploads require the explicit trusted qualification branch push")
    before = prior.github(f"/repos/{prior.REPOSITORY}/releases/{prior.RELEASE}")
    prior.admit_release(before)
    original_ids = {item["name"]: item["id"] for item in before["assets"]}
    retained = []
    for path in sorted(uploads.iterdir()):
        current = prior.github(f"/repos/{prior.REPOSITORY}/releases/{prior.RELEASE}")
        prior.admit_release(current)
        matches = [item for item in current["assets"] if item["name"] == path.name]
        reused = bool(matches)
        if not matches:
            subprocess.run(["gh", "release", "upload", prior.TAG, str(path),
                            "--repo", prior.REPOSITORY], check=True, stdout=subprocess.DEVNULL)
            current = prior.github(f"/repos/{prior.REPOSITORY}/releases/{prior.RELEASE}")
            prior.admit_release(current)
            matches = [item for item in current["assets"] if item["name"] == path.name]
        require(len(matches) == 1 and matches[0]["state"] == "uploaded",
                "retained draft asset is ambiguous or incomplete")
        asset = matches[0]
        data = prior.github(f'/repos/{prior.REPOSITORY}/releases/assets/{asset["id"]}', binary=True)
        require(data == path.read_bytes(), "draft asset bytes differ; never overwrite existing asset")
        retained.append({"asset_id": asset["id"], "name": path.name, "bytes": len(data),
                         "bytes_equal": True, "reused_without_overwrite": reused,
                         "authenticated_api_reference": asset["url"]})
    after = prior.github(f"/repos/{prior.REPOSITORY}/releases/{prior.RELEASE}")
    prior.admit_release(after)
    final_ids = {item["name"]: item["id"] for item in after["assets"]}
    require(all(final_ids.get(name) == identity for name, identity in original_ids.items()),
            "existing draft asset identities changed")
    prior.write_json(output / "retention-receipt.json", {
        **prior.run_identity(), "status": "pass", "release_id": prior.RELEASE,
        "release_tag": prior.TAG, "draft": True, "assets": retained,
        "existing_assets_preserved": original_ids,
        "scope": "authenticated byte equality after additive draft retention; no transition approval",
    })


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prior-tools", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--upload", action="store_true")
    args = parser.parse_args()
    expected = json.loads(RECORD.read_text())
    prior, audit = load_prior_tools(args.prior_tools, expected)
    args.work.mkdir(parents=True, exist_ok=False)
    run = prior.github(f'/repos/{prior.REPOSITORY}/actions/runs/{expected["run_id"]}')
    artifacts = prior.github(f'/repos/{prior.REPOSITORY}/actions/runs/{expected["run_id"]}/artifacts?per_page=100')["artifacts"]
    admit_run(run, artifacts, expected)
    raw = args.work / "raw"
    raw.mkdir()
    for item in expected["artifacts"]:
        (raw / (item["name"] + ".zip")).write_bytes(download_artifact(item["id"], prior))
    assets = args.work / "assets"
    prior.download(assets)
    uploads = assemble(raw, assets, args.work, prior, audit, expected)
    if args.upload:
        retain(uploads, args.work, prior)
    print("All eighteen prior Python tuples re-admitted; original ZIP bytes preserved.")


if __name__ == "__main__":
    main()
