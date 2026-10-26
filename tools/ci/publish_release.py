"""Publish already selected package bytes after rechecking the remote tag."""

import argparse
import subprocess
from pathlib import Path

from tools.ci import release_assets
from tools.packaging import release_identity
from tools.paths import ROOT


def publish(root, destination, tag, repository):
    """Create the GitHub release of `tag` from the files the selector chose.

    The files must be exactly the declared release assets of the release the
    tree prepares. The release is titled `yamaa v<Cargo version>`, and a
    release candidate is created as a pre-release.
    """
    revision = (destination / "source-revision.txt").read_text().strip()
    subprocess.run(
        [
            "git",
            "fetch",
            "--no-tags",
            "origin",
            f"refs/tags/{tag}:refs/release-publication/checked-tag",
        ],
        cwd=root,
        check=True,
    )
    current = subprocess.check_output(
        ["git", "rev-parse", "refs/release-publication/checked-tag^{commit}"],
        cwd=root,
        text=True,
    ).strip()
    if current != revision:
        raise ValueError("remote tag no longer names the qualified source revision")
    identity = release_identity.read(root)
    if tag != identity.tag:
        raise ValueError(f"{tag} does not name the prepared release {identity.tag}")
    assets = [
        Path(line)
        for line in (destination / "asset-paths.txt").read_text().splitlines()
    ]
    if not all(path.is_file() for path in assets):
        raise ValueError("a selected release asset is missing")
    release_assets.match(assets, release_assets.declared(), identity)
    subprocess.run(
        [
            "gh",
            "release",
            "create",
            tag,
            "--repo",
            repository,
            "--verify-tag",
            "--title",
            f"yamaa v{identity.cargo}",
            "--notes-file",
            str(destination / "release-notes.md"),
            *(["--prerelease"] if identity.candidate is not None else []),
            *map(str, assets),
        ],
        cwd=root,
        check=True,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--destination", type=Path, required=True)
    args = parser.parse_args()
    publish(ROOT, args.destination.resolve(), args.tag, args.repository)


if __name__ == "__main__":
    main()
