"""Check that PyPI serves a release's Python packages with the release's bytes.

The owner uploads the wheels and the Python source archive of a GitHub release
to PyPI (#3030 D57), as CONTRIBUTING.md describes under "Releasing", and then
runs this check. It downloads every file that PyPI lists for the release's
Python version, and the asset of the same name from the GitHub release of the
tag, and compares their bytes. The release must hold exactly the assets that
tools/ci/release_assets.py declares, and PyPI exactly the Python packages among
them: a file that differs, that is missing on either side or that is extra
fails the check. PyPI and GitHub also list digests of their files; the check
ignores them and computes none.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import urllib.error
import urllib.request
from pathlib import PurePosixPath

from tools.ci import release_assets
from tools.packaging import release_identity

# PyPI's JSON API lists the files of one version of the project under "urls".
PYPI_VERSION = "https://pypi.org/pypi/yamaa/{version}/json"
# Seconds to wait for PyPI to answer.
TIMEOUT = 60


def require(condition, message):
    if not condition:
        raise ValueError(message)


def fetch(url):
    """Return the body that an HTTPS GET of `url` receives."""
    require(url.startswith("https://"), f"not an HTTPS URL: {url}")
    with urllib.request.urlopen(url, timeout=TIMEOUT) as response:
        return response.read()


def gh(*arguments):
    """Return the standard output of a GitHub CLI call."""
    return subprocess.run(["gh", *arguments], check=True, stdout=subprocess.PIPE).stdout


def release_files(repository, tag):
    """List the name and id of each asset of the GitHub release of `tag`."""
    release = json.loads(gh("api", f"repos/{repository}/releases/tags/{tag}"))
    require(
        isinstance(release, dict) and release.get("tag_name") == tag,
        f"GitHub returned a release other than {tag}",
    )
    assets = release.get("assets")
    require(isinstance(assets, list), f"the GitHub release of {tag} lists no assets")
    files = []
    for asset in assets:
        require(
            isinstance(asset, dict)
            and isinstance(asset.get("name"), str)
            and type(asset.get("id")) is int
            and asset["id"] > 0,
            f"the GitHub release of {tag} lists a malformed asset",
        )
        files.append((asset["name"], asset["id"]))
    return files


def asset_bytes(repository, identifier):
    """Return the bytes of one GitHub release asset."""
    return gh(
        "api",
        "-H",
        "Accept: application/octet-stream",
        f"repos/{repository}/releases/assets/{identifier}",
    )


def pypi_files(version):
    """Map each file name that PyPI lists for `version` to its download URL."""
    try:
        listing = json.loads(fetch(PYPI_VERSION.format(version=version)))
    except urllib.error.HTTPError as error:
        if error.code == 404:
            raise ValueError(f"PyPI serves no yamaa {version}") from None
        raise
    files = listing.get("urls") if isinstance(listing, dict) else None
    require(isinstance(files, list), f"PyPI lists no files for yamaa {version}")
    urls = {}
    for file in files:
        require(
            isinstance(file, dict)
            and isinstance(file.get("filename"), str)
            and isinstance(file.get("url"), str),
            f"PyPI lists a malformed file for yamaa {version}",
        )
        name = file["filename"]
        require(name not in urls, f"PyPI lists {name} twice")
        urls[name] = file["url"]
    return urls


def compare(repository, tag):
    """Compare what PyPI serves for the release of `tag` with its GitHub assets.

    Return the names of the Python packages compared, in declaration order.
    Raise ValueError unless the GitHub release holds exactly the declared
    assets and PyPI serves exactly its Python packages, each with the bytes
    that the release holds.
    """
    require(
        re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository),
        "invalid release repository",
    )
    # Any name but a release tag is refused here, before any request.
    identity = release_identity.from_tag(tag)
    files = release_files(repository, tag)
    try:
        pairs = release_assets.match(
            [PurePosixPath(name) for name, _ in files],
            release_assets.declared(),
            identity,
        )
    except ValueError as error:
        raise ValueError(f"the GitHub release of {tag}: {error}") from None
    names = [
        path.name for asset, path in pairs if asset.kind in release_assets.PYTHON_KINDS
    ]
    served = pypi_files(identity.python)
    problems = []
    missing = [name for name in names if name not in served]
    if missing:
        problems.append("PyPI does not serve " + ", ".join(missing))
    extra = sorted(set(served) - set(names))
    if extra:
        problems.append(
            "PyPI also serves "
            + ", ".join(extra)
            + ", which the release does not hold as Python packages"
        )
    require(not problems, "; ".join(problems))
    # One package at a time, so at most two files are held at once.
    ids = dict(files)
    different = [
        name
        for name in names
        if fetch(served[name]) != asset_bytes(repository, ids[name])
    ]
    require(
        not different,
        "PyPI serves other bytes than the GitHub release for " + ", ".join(different),
    )
    return names


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True)
    parser.add_argument(
        "--tag", required=True, help="release tag: " + release_identity.TAG_FORMS
    )
    args = parser.parse_args(argv)
    try:
        names = compare(args.repository, args.tag)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"{error}\n")
    print(
        f"PyPI serves the Python packages of {args.tag} unchanged:\n" + "\n".join(names)
    )


if __name__ == "__main__":
    main()
