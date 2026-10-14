"""Verify the retained pre-cutover R artifact before a fresh-library install."""

import argparse
import json
import shutil
import tarfile
import os
import urllib.parse
import urllib.request
from pathlib import Path

PRIOR_RUN = 38394553441
PRIOR_SOURCE = "a1a9c2d216075f331ac108aa4d3638573f103cea"
PRIOR_VERSION = "0.3.0"
ARTIFACTS = {
    "native-r-ubuntu-24.04": 11811851297,
    "native-r-macos-15": 11812627529,
    "native-r-windows-latest": 11812047748,
}
RETAINED_RELEASE = 411979803
RETAINED_TAG = "yamaa-r-reference-a1a9c2d2"
RETAINED_ASSET = 637944247
RETAINED_BYTES = 3290128


class StripRedirectCredentials(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, file, code, message, headers, newurl):
        redirected = super().redirect_request(request, file, code, message, headers, newurl)
        if urllib.parse.urlparse(request.full_url).hostname != urllib.parse.urlparse(newurl).hostname:
            redirected.remove_header("Authorization")
        return redirected


def github(path, binary=False):
    request = urllib.request.Request("https://api.github.com/repos/elong0527/yamaa/" + path,
        headers={"Authorization": "Bearer " + os.environ["GH_TOKEN"],
                 "Accept": "application/octet-stream" if binary else "application/vnd.github+json",
                 "X-GitHub-Api-Version": "2022-11-28"}, method="GET")
    with urllib.request.build_opener(StripRedirectCredentials()).open(request, timeout=90) as response:
        data = response.read()
    return data if binary else json.loads(data)


def retained(source_directory, proof_directory):
    release = github(f"releases/{RETAINED_RELEASE}")
    if not (release["id"] == RETAINED_RELEASE and release["tag_name"] == RETAINED_TAG
            and release["target_commitish"] == PRIOR_SOURCE and release["draft"] is True):
        raise ValueError("R rollback draft release/source identity does not match")
    assets = [asset for asset in release["assets"] if asset["id"] == RETAINED_ASSET]
    if not (len(assets) == 1 and assets[0]["name"] == "yamaa_0.3.0.tar.gz"
            and assets[0]["size"] == RETAINED_BYTES and assets[0]["state"] == "uploaded"):
        raise ValueError("R rollback retained asset identity does not match")
    data = github(f"releases/assets/{RETAINED_ASSET}", binary=True)
    if len(data) != RETAINED_BYTES:
        raise ValueError("R rollback retained asset size does not match")
    source_directory.mkdir(parents=True, exist_ok=False)
    proof_directory.mkdir(parents=True, exist_ok=False)
    archive = source_directory / "yamaa_0.3.0.tar.gz"
    archive.write_bytes(data)
    check_archive(archive)
    receipt = {
        "artifact_reference": assets[0]["url"], "release_id": RETAINED_RELEASE,
        "release_tag": RETAINED_TAG, "asset_id": RETAINED_ASSET, "draft": True,
        "source_revision": PRIOR_SOURCE, "package_version": PRIOR_VERSION,
        "source_archive_bytes": len(data), "rehearsal": "pending",
        "status": "qualified installed pre-cutover source artifact; not a published release",
        "retention_proof_run": 38450032850,
        "retrieval": "authenticated exact retained release asset; no expiring Actions dependency",
    }
    (proof_directory / "artifact-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print("verified durable retained R source/release/asset/version before installation")


def check_archive(archive):
    with tarfile.open(archive, "r:gz") as contents:
        fields = contents.extractfile("yamaa/DESCRIPTION").read().decode("utf-8").splitlines()
        if "Package: yamaa" not in fields or "Version: 0.3.0" not in fields:
            raise ValueError("retained archive package/version does not match")
        for name in ("yamaa/src/rust/Cargo.lock", "yamaa/src/rust/Cargo.toml", "yamaa/src/rust/rust-toolchain.toml"):
            if not contents.getmember(name).isfile():
                raise ValueError("retained source is missing its locked Rust workspace")


def prepare(source_directory, proof_directory, artifact_name, artifact_id):
    if ARTIFACTS.get(artifact_name) != artifact_id:
        raise ValueError("R rollback artifact is outside the reviewed OS/source set")
    run = json.loads((proof_directory / "prior-run.json").read_text())
    artifact = json.loads((proof_directory / "prior-artifact.json").read_text())
    if (
        run["id"] != PRIOR_RUN
        or run["head_sha"] != PRIOR_SOURCE
        or run["run_attempt"] != 1
        or run["status"] != "completed"
        or run["conclusion"] != "success"
        or artifact["id"] != artifact_id
        or artifact["name"] != artifact_name
        or artifact["expired"]
        or artifact["workflow_run"]["id"] != PRIOR_RUN
        or artifact["workflow_run"]["head_sha"] != PRIOR_SOURCE
    ):
        raise ValueError("R rollback source/run/artifact identity does not match")
    archives = list(source_directory.rglob("yamaa_0.3.0.tar.gz"))
    if len(archives) != 1:
        raise ValueError("expected one retained R source archive")
    archive = archives[0]
    check_archive(archive)
    canonical = source_directory / "yamaa_0.3.0.tar.gz"
    if archive != canonical:
        shutil.copyfile(archive, canonical)
        if canonical.read_bytes() != archive.read_bytes():
            raise ValueError("normalizing artifact layout changed source bytes")
    receipt = {
        "artifact_reference": (
            f"github-actions:{PRIOR_RUN}/attempt-1/{artifact_name}/yamaa_0.3.0.tar.gz"
        ),
        "artifact_id": artifact_id,
        "source_revision": PRIOR_SOURCE,
        "package_version": PRIOR_VERSION,
        "source_archive_bytes": canonical.stat().st_size,
        "artifact_expires_at": artifact["expires_at"],
        "status": "qualified installed pre-cutover source artifact; not a published release",
        "evidence": run["html_url"],
        "rehearsal": "pending",
    }
    (proof_directory / "artifact-receipt.json").write_text(
        json.dumps(receipt, indent=2) + "\n", encoding="utf-8"
    )
    print("verified retained R source/run/artifact/version before installation")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source_directory", type=Path)
    parser.add_argument("proof_directory", type=Path)
    parser.add_argument("--artifact-name", choices=ARTIFACTS)
    parser.add_argument("--artifact-id", type=int)
    parser.add_argument("--retained-release", action="store_true")
    args = parser.parse_args()
    if args.retained_release:
        if args.artifact_name is not None or args.artifact_id is not None:
            parser.error("retained release and Actions artifact selectors are exclusive")
        retained(args.source_directory, args.proof_directory)
        return
    if args.artifact_name is None or args.artifact_id is None:
        parser.error("Actions artifact name and id are required")
    prepare(
        args.source_directory,
        args.proof_directory,
        args.artifact_name,
        args.artifact_id,
    )


if __name__ == "__main__":
    main()
