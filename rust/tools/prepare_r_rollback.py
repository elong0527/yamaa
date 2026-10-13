"""Verify the retained pre-cutover R artifact before a fresh-library install."""

import argparse
import json
import shutil
import tarfile
from pathlib import Path

PRIOR_RUN = 38394553441
PRIOR_SOURCE = "a1a9c2d216075f331ac108aa4d3638573f103cea"
PRIOR_VERSION = "0.3.0"
ARTIFACTS = {
    "native-r-ubuntu-24.04": 11811851297,
    "native-r-macos-15": 11812627529,
    "native-r-windows-latest": 11812047748,
}


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
    with tarfile.open(archive, "r:gz") as contents:
        description = contents.extractfile("yamaa/DESCRIPTION").read().decode("utf-8")
        if (
            "Package: yamaa\n" not in description
            or "Version: 0.3.0\n" not in description
        ):
            raise ValueError("retained archive package/version does not match")
        required = (
            "yamaa/src/rust/Cargo.lock",
            "yamaa/src/rust/Cargo.toml",
            "yamaa/src/rust/rust-toolchain.toml",
        )
        for name in required:
            if not contents.getmember(name).isfile():
                raise ValueError("retained source is missing its locked Rust workspace")
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
    parser.add_argument("--artifact-name", required=True, choices=ARTIFACTS)
    parser.add_argument("--artifact-id", required=True, type=int)
    args = parser.parse_args()
    prepare(
        args.source_directory,
        args.proof_directory,
        args.artifact_name,
        args.artifact_id,
    )


if __name__ == "__main__":
    main()
