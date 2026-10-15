"""Publish already selected package bytes after rechecking the remote tag."""

import argparse
import subprocess
from pathlib import Path

from tools.paths import ROOT


def publish(root, destination, tag, repository):
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
    assets = (destination / "asset-paths.txt").read_text().splitlines()
    if len(assets) != 5 or not all(Path(path).is_file() for path in assets):
        raise ValueError("publication requires the five selected package assets")
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
            f"yamaa {tag.removeprefix('yamaa-v')}",
            "--notes-file",
            str(destination / "release-notes.md"),
            *assets,
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
