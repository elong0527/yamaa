"""The files a release publishes, declared once.

Each asset is one file in one native artifact, which one native job uploads.
The selector downloads those artifacts from the qualifying native run and
requires each to hold exactly the assets declared in it; the publisher uploads
exactly these files; the release rehearsal runs the selector; and the asset
table in docs/release.md lists them, which tools/tests/test_release_artifacts.py
keeps equal to `declared()`. So every asset the native run built is published,
or publication fails, and nothing is rebuilt.
"""

import re
from dataclasses import dataclass

from tools.ci import native

# Placeholders in a declared file name. In a wheel's platform tag, `*` stands
# for the platform version, such as the 11_0 of macosx_11_0_arm64.
PLACEHOLDERS = {
    "<Python version>": lambda identity: re.escape(identity.python),
    "<R version>": lambda identity: re.escape(identity.r),
    "*": lambda identity: "[0-9]+_[0-9]+",
}

# The asset kinds that PyPI receives as well: the wheels and the source archive.
PYTHON_KINDS = frozenset({"wheel", "sdist"})

# The platform that each Python build lane builds its wheel for, and the
# platform tag of that wheel. tools/ci/manylinux.py builds the Linux wheels.
WHEEL_PLATFORMS = {
    "ubuntu-24.04": ("Linux x86_64", "manylinux_2_28_x86_64"),
    "macos-15": ("macOS arm64", "macosx_*_arm64"),
    "windows-2025": ("Windows x64", "win_amd64"),
    "ubuntu-24.04-arm": ("Linux aarch64", "manylinux_2_28_aarch64"),
}


@dataclass(frozen=True)
class Asset:
    """One file that a release publishes, and the native artifact holding it."""

    # What the selector checks inside the file: "wheel", "sdist" or "r-source".
    kind: str
    title: str
    # The file name, with the placeholders above.
    file: str
    artifact: str
    # The native job that uploads the artifact.
    producer: str

    def pattern(self, identity):
        """Return the pattern of this asset's file name in release `identity`."""
        pieces = re.split("(" + "|".join(map(re.escape, PLACEHOLDERS)) + ")", self.file)
        return re.compile(
            "".join(
                PLACEHOLDERS[piece](identity)
                if piece in PLACEHOLDERS
                else re.escape(piece)
                for piece in pieces
            )
        )


def declared(coverage=None):
    """List the release assets of the native matrix, in publication order."""
    coverage = native.coverage() if coverage is None else coverage
    oldest = min(
        native.PYTHONS, key=lambda version: tuple(map(int, version.split(".")))
    )
    abi = "cp" + oldest.replace(".", "")
    assets = []
    for lane in coverage["build"]["include"]:
        platform, python = lane["os"], lane["python"]
        label, tag = WHEEL_PLATFORMS[platform]
        assets.append(
            Asset(
                "wheel",
                f"Python wheel for {label}",
                f"yamaa-<Python version>-{abi}-abi3-{tag}.whl",
                f"native-python-{platform}-{python}",
                f"native-dist ({platform}, {python})",
            )
        )
    source, python = native.SOURCE_PLATFORM, native.BUILD_PYTHON
    if f"native-python-{source}-{python}" not in {asset.artifact for asset in assets}:
        raise ValueError(f"no Python build lane runs on the source platform {source}")
    assets += [
        Asset(
            "sdist",
            "Python source archive",
            "yamaa-<Python version>.tar.gz",
            f"native-python-{source}-{python}",
            f"native-dist ({source}, {python})",
        ),
        Asset(
            "r-source",
            "R source package",
            "yamaa_<R version>.tar.gz",
            f"native-r-{source}",
            f"native-r-dist ({source})",
        ),
    ]
    return assets


def artifacts(assets):
    """Map each release artifact to the one native job that uploads it."""
    producers = {}
    for asset in assets:
        if producers.setdefault(asset.artifact, asset.producer) != asset.producer:
            raise ValueError(f"{asset.artifact}: declared with two producing jobs")
    return producers


def match(paths, assets, identity):
    """Pair each asset with the one path that carries its file name.

    Raise ValueError unless every asset has exactly one path and every path is
    one of the assets.
    """
    names = [path.name for path in paths]
    if len(set(names)) != len(names):
        raise ValueError("release file names are ambiguous")
    pairs = []
    for asset in assets:
        pattern = asset.pattern(identity)
        found = [path for path in paths if pattern.fullmatch(path.name)]
        if len(found) != 1:
            raise ValueError(
                f"missing or ambiguous release asset: {asset.title} ({asset.file})"
            )
        pairs.append((asset, found[0]))
    claimed = [path.name for _, path in pairs]
    if len(set(claimed)) != len(claimed):
        raise ValueError("one file matches two release assets")
    undeclared = sorted(set(names) - set(claimed))
    if undeclared:
        raise ValueError("no release asset declares: " + ", ".join(undeclared))
    return pairs
