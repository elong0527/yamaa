"""Generate the component inventory that every package ships beside its notices.

Every package carries COMPONENTS.tsv beside LICENSE-THIRD-PARTY.txt: each
wheel in its .dist-info/licenses directory, the Python source archive at its
root, the R source package in inst/, from which R installs it at the package
root, and each command-line archive beside the executable. The wheels carry
the Python inventory, python/COMPONENTS.tsv, and the Python source archive the
same inventory without its rust-std row (source_inventory); the R source
package carries the R inventory, R/yamaa/inst/COMPONENTS.tsv. The R package's
configure step, R/yamaa/tools/configure.R, appends a rust-std row for the
rustc that compiles the installation to the copy that R installs, so an
installed R package and an R binary package carry the R inventory with that
row (installed_inventory). The command-line archives carry the command
line's inventory, rust/crates/yamaa-cli/COMPONENTS.tsv.

A host's inventory lists the components that its packages are built from:

- every package of the host's trimmed lock, which
  tools/staging/engine_bundle.py stages (#3030 D55): the engine crates, the
  host's binding crate, or yamaa-cli for the command line, the vendored YAML
  parser and every crates.io crate that they reach on any platform with any
  feature, including the crates that only build scripts and procedural macros
  use and those that no supported target compiles;
- the C code that those crates compile from sources that they carry: ReadStat,
  vendored in yamaa-adapters, and the zstd library that zstd-sys bundles;
- for a host in BINARY_HOSTS, whose committed inventory is that of binary
  packages that the pinned toolchain builds, the Rust standard library of
  that toolchain, which those packages link. A source archive links the
  standard library of whatever toolchain builds it, so its inventory has no
  such row; the R package's configure step adds the row of the toolchain that
  builds it when R installs it.

It is tab-separated UTF-8 text with LF line endings: a header row, then one
row per component, sorted by name and version (the row that the R package's
configure step appends follows them), with these columns:

- name and version: the locked crate's, the C code's upstream release, or
  rust-std and the Rust release that rust/rust-toolchain.toml pins, or for
  the row that configure appends, the release that rustc reports;
- license: the license that yamaa relies on, as the notices choose it from
  the expression that the crate's manifest declares
  (generate_third_party_notices.relied_on), or that it relies on for the C
  code or the standard library; never a license that is offered but not
  relied on, so no row names a GNU license;
- source: crates.io for a registry crate; for a path crate, its directory in
  the yamaa repository, followed for a vendored copy of third-party code by
  the upstream repository and the commit that its provenance names; for
  bundled C code, the crate that bundles it; for the standard library, the
  Rust repository at the release's tag;
- purpose: the crate's purpose in tools/packaging/crate_purposes.toml, or the
  C code's in VENDORED below, or the standard library's, BINARY_PURPOSE or,
  for the row that configure appends, INSTALLED_PURPOSE.

No column holds a digest: a component is named by its name, version, license,
source and purpose (AGENTS.md, no content hashing).

A purpose also says whether the host's binaries link the crate: the purposes
of the crates that they do not link start with one of UNLINKED. Each host's
LICENSE-THIRD-PARTY.txt lists the third-party crates that its binding links,
so it must list exactly the crates that the host's inventory marks as linked
(notice_problems).

By default this checks the committed inventories against the trimmed locks,
the purposes, the locked crates' manifests, the provenance of the vendored
sources and the pinned toolchain, each host's notices against the crates
that its inventory marks as linked, and that the Python project lists
COMPONENTS.tsv among its license files. `--write` regenerates the committed
inventories. `--host <host>` writes the inventory of any host that
tools/staging/engine_bundle.py stages to standard output instead, for a
package whose inventory is not committed.
"""

import argparse
import re
import subprocess
import sys
import tempfile
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

from tools.ci import toolchain
from tools.packaging import check_crate_purposes as purposes
from tools.packaging import generate_third_party_notices as notices
from tools.staging import engine_bundle

NAME = notices.INVENTORY
COLUMNS = ("name", "version", "license", "source", "purpose")
REGISTRY = "crates.io"
# The hosts whose committed inventory is that of binary packages that the
# pinned toolchain builds: the Python wheels and the command-line binaries.
# Their inventory names that toolchain's standard library, which they link;
# the R source package's names none, and the Python source archive's leaves it
# out (source_inventory). The R package's configure step appends the row to
# the copy that R installs (installed_inventory).
BINARY_HOSTS = frozenset({"python", "cli"})
# The standard library's row: rustup's name for the component that holds it,
# and the license that yamaa relies on of its MIT OR Apache-2.0.
STANDARD_LIBRARY = "rust-std"
STANDARD_LIBRARY_LICENSE = "MIT"
RUST_REPOSITORY = "https://github.com/rust-lang/rust"
# The row's purpose in the inventory of the wheels and the command-line
# binaries, and in an installed R package's. R/yamaa/tools/configure.R writes
# the second as STANDARD_LIBRARY_PURPOSE, and tools/tests/test_r_configure.py
# keeps the two texts equal.
BINARY_PURPOSE = (
    "the Rust standard library (core, alloc and std) of the pinned toolchain "
    "that builds this package, statically linked into its native code; "
    "LICENSE-THIRD-PARTY.txt carries its notices"
)
INSTALLED_PURPOSE = (
    "the Rust standard library (core, alloc and std) of the toolchain that "
    "compiled this package, statically linked into its native code; configure "
    "adds this row at installation with the version that rustc reports"
)
# A vendored crate's PROVENANCE.md names the upstream commit it was taken from.
UPSTREAM_COMMIT = re.compile(r"The upstream release commit is `([0-9a-f]{40})`\.")
# The README of the vendored ReadStat sources names their upstream release,
# commit and repository.
READSTAT_RELEASE = re.compile(
    r"ReadStat ([0-9]+(?:\.[0-9]+)+), upstream commit `([0-9a-f]{40})` from "
    r"<(https://[^>\s]+)>"
)
# A zstd-sys version names the zstd release it bundles as build metadata.
BUNDLED_ZSTD = re.compile(r"[^+]+\+zstd\.([0-9]+(?:\.[0-9]+)+)")
# The beginnings of the purposes of the crates that no host's binaries link,
# as tools/packaging/crate_purposes.toml states them: the crates that only
# build scripts and procedural macros use, which run while compiling, and the
# crates that no supported target compiles. A crate whose purpose begins
# otherwise is linked.
UNLINKED = ("build script:", "procedural macro:", "not compiled:")
NOTICE_HINT = (
    "Each LICENSE-THIRD-PARTY.txt lists the crates that its host's binding links: "
    "regenerate the notices with `python -m "
    "tools.packaging.generate_third_party_notices --write`, or begin the purpose "
    f"of a crate that no binary links in {purposes.LABEL} with "
    f"{', '.join(UNLINKED[:-1])} or {UNLINKED[-1]}"
)


@dataclass(frozen=True)
class Component:
    name: str
    version: str
    license: str
    source: str
    purpose: str


def directory_label(path):
    """A directory's path in the yamaa repository, as the notices name sources."""
    return path.resolve().relative_to(notices.WORKSPACE.parent.resolve()).as_posix()


def flat_text(path):
    """A provenance document's text, with its whitespace collapsed."""
    return " ".join(path.read_text(encoding="utf-8").split())


def readstat(package):
    """The upstream release and source of the ReadStat sources in yamaa-adapters."""
    root = Path(package["manifest_path"]).parent / "vendor/readstat"
    match = READSTAT_RELEASE.search(flat_text(root / "README.md"))
    if match is None:
        raise ValueError(
            f"{directory_label(root)}/README.md must name the vendored release as "
            "'ReadStat <version>, upstream commit `<commit>` from <<repository>>'"
        )
    version, commit, repository = match.groups()
    return version, f"{directory_label(root)}, from {repository} at {commit}"


def bundled_zstd(package):
    """The zstd release that a zstd-sys crate bundles, and that crate."""
    match = BUNDLED_ZSTD.fullmatch(package["version"])
    if match is None:
        raise ValueError(
            f"zstd-sys {package['version']}: its version names no bundled zstd "
            "release as +zstd.<version>"
        )
    return match[1], f"bundled in zstd-sys {package['version']}"


@dataclass(frozen=True)
class Vendored:
    """C code that a shipped crate compiles from sources that it carries.

    The inventory lists it wherever a host's trimmed lock keeps `crate`;
    `provenance` gives its version and source from that crate's package.
    """

    name: str
    crate: str
    license: str
    purpose: str
    provenance: Callable[[dict], tuple[str, str]]


VENDORED = (
    Vendored(
        name="ReadStat",
        crate="yamaa-adapters",
        license="MIT",
        purpose=(
            "SAS data set and transport file reading (the readstat feature): "
            "the SAS reader subset that yamaa-adapters compiles, with the later "
            "upstream fixes and the local changes that its README lists"
        ),
        provenance=readstat,
    ),
    Vendored(
        name="libzstd",
        crate="zstd-sys",
        # zstd is offered under BSD-3-Clause or GPL-2.0-only; yamaa relies on
        # BSD-3-Clause (EXTRA_FILES in generate_third_party_notices.py).
        license="BSD-3-Clause",
        purpose=(
            "the Zstandard C library that zstd-sys compiles and links: the zstd "
            "codec of Parquet pages (the parquet feature)"
        ),
        provenance=bundled_zstd,
    ),
)


def standard_library(release, purpose=BINARY_PURPOSE):
    """The row of the Rust standard library of the pinned toolchain, whose
    release rust/rust-toolchain.toml names, with the binary packages' purpose
    unless another is given."""
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", release):
        raise ValueError(
            f"rust/rust-toolchain.toml pins {release!r}, not a Rust release such "
            "as 1.99.1, which the binary packages' inventory names"
        )
    return Component(
        STANDARD_LIBRARY,
        release,
        STANDARD_LIBRARY_LICENSE,
        f"{RUST_REPOSITORY} at {release}",
        purpose,
    )


def crate_source(package):
    """Where a locked crate comes from: crates.io, or its directory, with the
    upstream repository and commit of a vendored copy in rust/vendor."""
    if package["source"] == notices.CRATES_IO:
        return REGISTRY
    directory = notices.source_label(package)
    root = Path(package["manifest_path"]).parent
    if root.resolve().parent != (notices.WORKSPACE / "vendor").resolve():
        return directory
    match = UPSTREAM_COMMIT.search(flat_text(root / "PROVENANCE.md"))
    repository = package.get("repository")
    if match is None or not repository:
        raise ValueError(
            f"{package['name']} {package['version']}: {directory}/PROVENANCE.md "
            "must name the upstream commit as 'The upstream release commit is "
            "`<commit>`.', and the crate's manifest its upstream repository"
        )
    return f"{directory}, from {repository} at {match[1]}"


def components(kept, metadata, crate_purposes, release=None):
    """The inventory rows of a host, sorted by name and version.

    `kept` holds the (name, version, source) keys of the host's trimmed lock,
    `metadata` is the canonical lock's `cargo metadata` and `crate_purposes`
    maps crate names to purposes. `release` is the pinned Rust release for a
    host in BINARY_HOSTS, whose packages link its standard library, and None
    for a host whose packages are source archives.
    """
    packages = {
        (package["name"], package["version"], package["source"]): package
        for package in metadata["packages"]
    }
    rows = []
    for key in sorted(kept, key=str):
        name, version, _ = key
        label = f"{name} {version}"
        package = packages.get(key)
        if package is None:
            raise ValueError(f"{label}: locked, but missing from the Cargo metadata")
        purpose = crate_purposes.get(name)
        if not isinstance(purpose, str) or not purpose.strip():
            raise ValueError(f"{label}: {purposes.LABEL} states no purpose for it")
        rows.append(
            Component(
                name,
                version,
                notices.relied_on(package, metadata),
                crate_source(package),
                purpose.strip(),
            )
        )
        for vendored in VENDORED:
            if vendored.crate == name:
                upstream, source = vendored.provenance(package)
                rows.append(
                    Component(
                        vendored.name,
                        upstream,
                        vendored.license,
                        source,
                        vendored.purpose,
                    )
                )
    if release is not None:
        rows.append(standard_library(release))
    seen = set()
    for row in rows:
        if (row.name, row.version) in seen:
            raise ValueError(f"{row.name} {row.version}: listed twice")
        seen.add((row.name, row.version))
    return sorted(
        rows, key=lambda row: (row.name.casefold(), notices.version_key(row.version))
    )


def row_line(row):
    """One component's line of the inventory, without its line ending."""
    fields = (row.name, row.version, row.license, row.source, row.purpose)
    for column, field in zip(COLUMNS, fields):
        if not field or re.search(r"[\t\r\n]", field):
            raise ValueError(
                f"{row.name} {row.version}: the {column} must be one line "
                "of text, without tabs"
            )
    return "\t".join(fields)


def render(rows):
    """The inventory's text: a header row, then one row per component."""
    text = "\n".join(["\t".join(COLUMNS), *map(row_line, rows)]) + "\n"
    return notices.refuse_gnu(text, f"the {NAME} rows")


def generate(hosts, metadata):
    """The inventory text of each host, by host; `metadata` is the canonical
    lock's `cargo metadata` (generate_third_party_notices.locked_metadata)."""
    crate_purposes = purposes.read()
    pinned = toolchain.channel()
    inventories = {}
    with tempfile.TemporaryDirectory(prefix="yamaa-component-inventory-") as scratch:
        for host in hosts:
            kept = engine_bundle.stage_workspace(host, Path(scratch) / host)
            release = pinned if host in BINARY_HOSTS else None
            inventories[host] = render(
                components(kept, metadata, crate_purposes, release)
            )
    return inventories


def source_inventory(text):
    """The inventory of the Python source archive: the wheels' inventory, the
    text of python/COMPONENTS.tsv, without its rust-std row. A build from the
    archive links the standard library of whatever toolchain builds it."""
    lines = text.splitlines(keepends=True)
    rows = [line for line in lines if line.partition("\t")[0] != STANDARD_LIBRARY]
    if len(lines) - len(rows) != 1:
        raise ValueError(
            f"python/{NAME}: the wheels' inventory must hold one {STANDARD_LIBRARY} "
            "row, which the Python source archive leaves out"
        )
    return "".join(rows)


def installed_inventory(text, release):
    """The inventory of an installed R package, and so of an R binary package:
    the R source package's, the text of R/yamaa/inst/COMPONENTS.tsv, followed
    by the one rust-std row that R/yamaa/tools/configure.R appends for the
    rustc that compiles the installation, here of the Rust release `release`.
    """
    if any(line.partition("\t")[0] == STANDARD_LIBRARY for line in text.splitlines()):
        raise ValueError(
            f"R/yamaa/inst/{NAME}: the R source package's inventory must hold no "
            f"{STANDARD_LIBRARY} row, which configure appends at installation"
        )
    return text + row_line(standard_library(release, INSTALLED_PURPOSE)) + "\n"


def committed():
    """The committed inventory of each host, by host."""
    return {host: directory / NAME for host, directory in notices.copies().items()}


def listed(text):
    """The (name, version) of each row of an inventory's text; a line without
    a tab names no row."""
    return {
        tuple(line.split("\t")[:2]) for line in text.splitlines()[1:] if "\t" in line
    }


def malformed(text):
    """The numbers of an inventory's lines that do not hold one field for each
    column, such as a conflict marker that a merge left behind."""
    return [
        number
        for number, line in enumerate(text.splitlines(), start=1)
        if line.count("\t") != len(COLUMNS) - 1
    ]


def check(generated, paths, project):
    """Messages for the committed inventories that differ from the generated
    ones, naming the lines that are not rows, and for a Python project whose
    license files leave the inventory out.

    `generated` and `paths` map each host to its generated inventory and to
    its committed copy, and `project` is the Python project table.
    """
    errors = []
    if NAME not in project.get("license-files", ()):
        errors.append(
            f"python/pyproject.toml: license-files must list {NAME}, so that the "
            "Python packages carry it"
        )
    for host, path in paths.items():
        relative = path.relative_to(notices.ROOT).as_posix()
        if not path.is_file():
            errors.append(f"{relative}: missing")
            continue
        text = path.read_bytes().decode("utf-8")
        expected = generated[host]
        if text == expected:
            continue
        for number in malformed(text):
            errors.append(
                f"{relative}:{number}: not a row of {len(COLUMNS)} tab-separated fields"
            )
        for name, version in sorted(listed(expected) - listed(text)):
            errors.append(
                f"{relative}: no row for {name} {version}, which the {host} "
                "bundle ships"
            )
        for name, version in sorted(listed(text) - listed(expected)):
            errors.append(
                f"{relative}: lists {name} {version}, which the {host} bundle no "
                "longer ships"
            )
        errors.append(
            f"{relative}: differs from the inventory that the {host} bundle's "
            f"trimmed lock, {purposes.LABEL} and the vendored sources give"
        )
    return errors


def linked_crates(text, metadata):
    """The (name, version) of each third-party crate that an inventory's text
    marks as linked: each row of a locked package outside the workspace whose
    purpose begins with none of UNLINKED. `metadata` is the canonical lock's
    `cargo metadata`."""
    members = set(metadata["workspace_members"])
    third_party = {
        (package["name"], package["version"])
        for package in metadata["packages"]
        if package["id"] not in members
    }
    linked = set()
    for line in text.splitlines()[1:]:
        name, version, _, _, purpose = line.split("\t")
        if (name, version) in third_party and not purpose.startswith(UNLINKED):
            linked.add((name, version))
    return linked


def notice_problems(generated, metadata):
    """Messages for each host whose LICENSE-THIRD-PARTY.txt lists other crates
    than its inventory marks as linked (#3470).

    `generated` maps each host to its inventory and `metadata` is the
    canonical lock's `cargo metadata`. An inventory states one purpose for
    each crate name, whatever the releases of that name that it lists, so the
    notices must list a release of each name that it marks as linked, and
    each release that they list must be a row that it marks as linked. A
    missing notice is generate_third_party_notices's to report.
    """
    errors = []
    for host, directory in notices.copies().items():
        path = directory / notices.NOTICES
        if not path.is_file():
            continue
        relative = path.relative_to(notices.ROOT).as_posix()
        inventory = (directory / NAME).relative_to(notices.ROOT).as_posix()
        listed = notices.committed_entries(path.read_bytes().decode("utf-8"))
        linked = linked_crates(generated[host], metadata)
        named = {name for name, _ in listed}
        for name, version in sorted(listed - linked):
            errors.append(
                f"{relative}: lists {name} {version}, which {inventory} does not "
                "mark as linked"
            )
        for name, version in sorted(linked):
            if name not in named:
                errors.append(
                    f"{relative}: no entry for {name} {version}, which {inventory} "
                    "marks as linked"
                )
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_mutually_exclusive_group()
    actions.add_argument(
        "--write", action="store_true", help="regenerate the committed inventories"
    )
    actions.add_argument(
        "--host",
        choices=sorted(engine_bundle.BINDINGS),
        help="write this host's inventory to standard output instead",
    )
    args = parser.parse_args()
    errors, unlisted = [], []
    try:
        metadata = notices.locked_metadata()
        if args.host:
            inventory = generate([args.host], metadata)[args.host]
            sys.stdout.buffer.write(inventory.encode("utf-8"))
            return
        paths = committed()
        generated = generate(paths, metadata)
        if args.write:
            for host, path in paths.items():
                path.write_bytes(generated[host].encode("utf-8"))
        else:
            errors = check(generated, paths, notices.python_project())
            unlisted = notice_problems(generated, metadata)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"{error}\n")
    if errors or unlisted:
        hints = []
        if errors:
            hints.append(
                "Run: python -m tools.packaging.generate_component_inventory --write"
            )
        if unlisted:
            hints.append(NOTICE_HINT)
        parser.exit(1, "\n".join(errors + unlisted + hints) + "\n")
    for host, path in paths.items():
        count = len(listed(generated[host]))
        print(
            f"{'Wrote ' if args.write else ''}"
            f"{path.relative_to(notices.ROOT).as_posix()}: the {count} components "
            f"of the {host} bundle"
        )


if __name__ == "__main__":
    main()
