"""The files a release publishes, declared once.

Each asset is one file in one native artifact, which one native job uploads.
The selector downloads those artifacts from the qualifying native run and
requires each to hold exactly the assets declared in it; the publisher uploads
exactly these files; the release rehearsal runs the selector;
tools/ci/check_pypi_release.py requires PyPI to serve exactly the Python ones,
unchanged; and the release body's download section lists them. So every
asset the native run built is published, or publication fails, and nothing is
rebuilt.
"""

import re
from dataclasses import dataclass

from tools.ci import native

# Placeholders in a declared file name. In a wheel's platform tag, `*` stands
# for the platform version, such as the 11_0 of macosx_11_0_arm64. The Cargo
# version is the engine's, which `yamaa --version` prints, and the release's
# own, as its tag and title spell it.
PLACEHOLDERS = {
    "<Python version>": lambda identity: re.escape(identity.python),
    "<R version>": lambda identity: re.escape(identity.r),
    "<Cargo version>": lambda identity: re.escape(identity.cargo),
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

# The platform that each R binary lane builds for, the extension of the
# package that R CMD INSTALL --build writes there, and the start of the R
# platform that the package's Built field names.
R_BINARY_PLATFORMS = {
    "macos-15": ("macOS arm64", "tgz", "aarch64-apple-darwin"),
    "windows-2025": ("Windows x64", "zip", "x86_64-w64-mingw32"),
}

# The platform that each command-line lane builds its binary for; the lane
# names the Rust target (tools/ci/native.py).
CLI_PLATFORMS = {
    "ubuntu-24.04": "Linux x86_64",
    "macos-15": "macOS arm64",
    "windows-2025": "Windows x64",
    "ubuntu-24.04-arm": "Linux aarch64",
}


@dataclass(frozen=True)
class Asset:
    """One file that a release publishes, and the native artifact holding it."""

    # What the selector checks inside the file: "wheel", "sdist", "r-source",
    # "r-binary", "cli" or "conformance".
    kind: str
    title: str
    # The file name, with the placeholders above.
    file: str
    artifact: str
    # The native job that uploads the artifact.
    producer: str
    # For an R binary package, the start of the Built field of its
    # DESCRIPTION: the R version and platform that it was built for.
    built: str | None = None

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
    # One R binary package per platform and R series. Its name is the one that
    # R CMD INSTALL --build writes with the series added after a hyphen, so
    # that both series' packages fit in one release: install.packages() takes
    # the package name from what comes before the underscore.
    for platform in coverage["r_binary"]["os"]:
        label, extension, r_platform = R_BINARY_PLATFORMS[platform]
        for version in coverage["r_binary"]["r"]:
            series = native.r_series(version)
            assets.append(
                Asset(
                    "r-binary",
                    f"R binary package for {label}, R {series}",
                    f"yamaa_<R version>-{series}.{extension}",
                    f"native-r-binary-{platform}-{version}",
                    f"native-r-binary ({platform}, R {version})",
                    f"R {version}; {r_platform}",
                )
            )
    # One command-line archive per target (#3030 D12), named with the engine's
    # version and the target, as tools/packaging/cli_archive.py writes it.
    for lane in coverage["cli"]["include"]:
        platform, target = lane["os"], lane["target"]
        assets.append(
            Asset(
                "cli",
                f"Command-line binary for {CLI_PLATFORMS[platform]}",
                f"yamaa-<Cargo version>-{target}.zip",
                f"native-cli-{target}",
                f"native-cli ({platform}, {target})",
            )
        )
    # The conformance archive (#3690): the closed grammars and the evaluation
    # set of the expression language as committed, named with the release's
    # version in its Cargo spelling, as tools/packaging/conformance_archive.py
    # writes it. Its files are the same for every platform, so the source
    # platform builds it once.
    assets.append(
        Asset(
            "conformance",
            "Grammars and expression evaluation set",
            "yamaa-<Cargo version>-conformance.zip",
            f"native-conformance-{source}",
            f"native-conformance ({source})",
        )
    )
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
