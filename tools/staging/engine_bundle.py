"""Stage the engine source bundle that a host's source archive builds from.

A host's bundle is the Rust workspace that its source archive compiles (#3030
D55): the engine crates, the boundary crate that both host bindings negotiate
through, and that host's binding crate, a workspace manifest
whose members are only those crates and whose profiles are those that the
host builds with (the R bundle's leaves out the wheels' dist profile), a lock
trimmed from the canonical rust/Cargo.lock and
verified crate by crate against it, the vendored YAML parser and ReadStat
sources, and the crates.io crates of that lock as `cargo vendor` writes them,
apart from the files that tools/staging/vendor_crates.py leaves out (a crate
that no supported target compiles, check_minimum_rust.TARGETS, keeps only its
manifest), with the Cargo configuration that reads them instead of the
network. The facade crate carries the selftest cases in its `selftest`
directory, the files of the benchmarks that benchmarks/selftest.yaml names,
which its build embeds (#3030 D108), and the adapters crate the schema files
that it embeds in its `schema` directory, the copies of yaml/ that
tools/packaging/generate_shipped_schema.py writes, so a build from the bundle
reads nothing outside it. It holds no tests or member dev-dependencies, no
examples, other benchmark fixtures or public-api.txt, and no Rust toolchain
file anywhere, the vendored crates included. The command line has no source
archive: its bundle, with the yamaa-cli crate in place of the boundary and a
binding, is the workspace that its
binaries build from, whose trimmed lock its component inventory lists
(tools/packaging/generate_component_inventory.py).

    python -m tools.staging.engine_bundle sdist <directory>

writes the Python source archive to <directory>: maturin's output over the
bundle, staged beside the Python package files, so that pip builds it with
Cargo's `--locked` and no network. It runs the `maturin` on the PATH, which
must be the release that python/pyproject.toml requires.

    python -m tools.staging.engine_bundle bundle {cli,python,r} <new directory>

writes a host's bundle alone: the workspace in rust/, the vendored crates in
vendor/ and the configuration in .cargo/config.toml.
tools/staging/stage_r_package.py stages the R bundle into the R source
package, with the vendored crates compressed.
"""

import argparse
import os
import re
import shutil
import subprocess
import tempfile
import tomllib
from pathlib import Path

from tools.packaging import check_minimum_rust
from tools.paths import ROOT, WORKSPACE
from tools.staging import vendor_crates

ENGINE = ("yamaa-core", "yamaa-adapters", "yamaa")
# The facade crate, which embeds the selftest cases.
FACADE = "yamaa"
# The adapters crate's directories of the data it embeds with include_str!,
# and the suffix each file there has: the copies of the schema files
# (src/shipped_schema/data.rs), which tools/packaging/generate_shipped_schema.py
# writes, and the closed Parquet profile (src/parquet_source/closed_profile.rs).
EMBEDDED = {"schema": (".yaml", ".json"), "profile": (".toml",)}
# The crate through which both host bindings negotiate their interface
# (rust/interface/README.md); the command line crosses no boundary.
BOUNDARY = "yamaa-boundary"
BINDINGS = {"python": "yamaa-python", "r": "yamaa-r", "cli": "yamaa-cli"}
MANIFEST, LOCK = "Cargo.toml", "Cargo.lock"
# The table of a member's test-only dependencies, which no bundle holds.
DEV_DEPENDENCIES = "[dev-dependencies]"
# Cargo reads its configuration from the directory it runs in and its parents.
# pip builds a Python source archive from its root, so the archive carries this
# file there, beside the vendored crates in vendor/; offline stops any Cargo
# command in the archive from reaching the network.
CARGO_CONFIG = vendor_crates.CONFIG + "\n[net]\noffline = true\n"
PYTHON_MANIFEST_PATH = "crates/yamaa-python/Cargo.toml"
# The profile that the Python wheels, as python/pyproject.toml selects, and the
# command-line binaries build with: release without the symbol table. The R
# bundle's manifest leaves it out, so that the R source package names no
# stripping. Its Makevars build with release: the R package strips nothing
# itself, as CRAN's Repository Policy asks and
# tools/packaging/check_cargo_lock.py checks. Its shared object keeps its
# symbols on Linux and macOS, and on Windows R's own toolchain links every
# package DLL with `-s` (#3030 D140).
WHEEL_PROFILE = "dist"


def members(host):
    """The workspace members of a host's bundle."""
    boundary = () if host == "cli" else (BOUNDARY,)
    return (*ENGINE, *boundary, BINDINGS[host])


def member_sources(member_root: Path):
    """Yield a member's manifest, build script, license files and Rust sources,
    the data the adapters embed (EMBEDDED) and their ReadStat sources."""
    manifest = member_root / MANIFEST
    build_script = member_root / "build.rs"
    sources = member_root / "src"
    if member_root.parent.is_symlink() or member_root.is_symlink():
        raise ValueError("workspace members must not be symlinks")
    if not manifest.is_file():
        raise ValueError(f"workspace member {member_root.name} is missing Cargo.toml")
    for path in (manifest, build_script, sources):
        if path.is_symlink():
            raise ValueError("workspace member sources must not be symlinks")
    yield manifest
    if build_script.is_file():
        yield build_script
    for path in sorted(member_root.glob("LICENSE*")):
        if path.is_symlink() or not path.is_file():
            raise ValueError("workspace member licenses must be regular files")
        yield path
    for path in sorted(sources.rglob("*")):
        if path.is_symlink():
            raise ValueError("workspace member sources must not be symlinks")
        if path.is_file() and path.suffix == ".rs":
            yield path
    # The data that the adapters embed with include_str! (EMBEDDED).
    for name, suffixes in EMBEDDED.items():
        embedded = member_root / name
        if embedded.is_symlink():
            raise ValueError(f"embedded {name} files must not be symlinks")
        if not embedded.exists():
            continue
        if member_root.name != "yamaa-adapters":
            raise ValueError(f"unreviewed embedded {name} directory")
        for path in sorted(embedded.rglob("*")):
            if path.is_symlink():
                raise ValueError(f"embedded {name} files must not be symlinks")
            if path.is_file():
                if path.suffix not in suffixes:
                    raise ValueError(f"unreviewed embedded {name} file")
                yield path
    # ReadStat is compiled from this exact licensed vendored source tree.
    vendor = member_root / "vendor" / "readstat"
    if (member_root / "vendor").is_symlink():
        raise ValueError("native vendor sources must not be symlinks")
    if vendor.exists():
        if member_root.name != "yamaa-adapters" or vendor.is_symlink():
            raise ValueError("unreviewed native vendor source tree")
        for path in sorted(vendor.rglob("*")):
            if path.is_symlink():
                raise ValueError("native vendor sources must not be symlinks")
            if path.is_file():
                if path.suffix not in {".c", ".h", ".md"} and path.name != "LICENSE":
                    raise ValueError("unreviewed native vendor source input")
                yield path


def vendored_sources(workspace: Path):
    """Retain complete, regular vendored crate sources and licensing notices.

    Workspace members depend on each crate under `vendor/` by path, which
    reaches every graph that builds them, including a project that embeds the
    facade. A `[patch]` applies only when this workspace is the root, so such
    a project would silently build without it (#2929): the workspace declares
    none.
    """
    if "patch" in tomllib.loads((workspace / "Cargo.toml").read_text()):
        raise ValueError(
            "the workspace must not declare [patch]; depend on vendored crates by path"
        )
    vendor = workspace / "vendor"
    if vendor.is_symlink():
        raise ValueError("vendored crates must not be symlinks")
    if not vendor.exists():
        return
    if not vendor.is_dir():
        raise ValueError("vendored crates must sit in a vendor directory")
    for root in sorted(vendor.iterdir()):
        if root.is_symlink():
            raise ValueError("vendored crates must not be symlinks")
        if not root.is_dir() or not re.fullmatch(r"[a-z0-9_-]+", root.name):
            raise ValueError(f"invalid vendored crate directory: {root.name}")
        for name in (
            "Cargo.toml",
            "LICENSE",
            "README.md",
            "PROVENANCE.md",
            "src/lib.rs",
        ):
            if not (root / name).is_file():
                raise ValueError(f"vendored crate is missing {name}")
        for path in sorted(root.rglob("*")):
            if path.is_symlink():
                raise ValueError("vendored crate sources must not be symlinks")
            if any(
                part in {"target", ".git", "__pycache__"}
                for part in path.relative_to(root).parts
            ):
                raise ValueError("vendored crate contains build or cache files")
            if path.name == "Cargo.lock" or path.suffix == ".pyc":
                raise ValueError("vendored crate contains build or cache files")
            if path.is_file():
                yield path


def sources(host, workspace=WORKSPACE):
    """Yield (path in the bundle workspace, checkout file) for each copied source.

    The facade crate's `selftest` directory holds each file that the facade
    embeds from the selftest cases of the checkout that holds `workspace`.
    The YAML parser keeps its provenance, patch and licenses but not its tests.
    """
    # Imported here because only staging reads the selftest list, a YAML file,
    # with PyYAML: the package checks that import this module, such as the
    # release selector's, run without PyYAML (#3347).
    from tools.packaging import generate_selftest_cases as selftest_cases

    for member in members(host):
        root = workspace / "crates" / member
        for path in member_sources(root):
            yield f"crates/{member}/{path.relative_to(root).as_posix()}", path
    staged = f"crates/{FACADE}/{selftest_cases.STAGED}"
    for relative, path in selftest_cases.case_files(workspace.parent):
        yield f"{staged}/{relative}", path
    for path in vendored_sources(workspace):
        relative = path.relative_to(workspace)
        if relative.parts[2] != "tests":
            yield relative.as_posix(), path


def without_table(lines, header, label="rust/Cargo.toml: the R bundle"):
    """A manifest's lines without one table: its header line, the comment lines
    directly above it, its lines up to the next blank line, and that blank line.

    The header must appear once, alone on its line.
    """
    found = [index for index, line in enumerate(lines) if line.strip() == header]
    if len(found) != 1:
        raise ValueError(
            f"{label} leaves out the {header} table, which "
            f"must appear once, as a line {header}"
        )
    start = end = found[0]
    while start > 0 and lines[start - 1].startswith("#"):
        start -= 1
    end += 1
    while end < len(lines) and lines[end].strip():
        end += 1
    return lines[:start] + lines[end + 1 :]


def member_manifest(path: Path):
    """A member's manifest without its dev-dependencies, which only its tests
    build, and a bundle holds no tests: the parquet crate, the adapters'
    differential oracle, is one, so no bundle vendors it (#3878).

    The table goes with the comment lines directly above it and every line up
    to the blank line that ends it (without_table), and the result must parse
    as the member's manifest without that table.
    """
    text = path.read_text(encoding="utf-8")
    expected = tomllib.loads(text)
    if expected.pop("dev-dependencies", None) is None:
        return text
    label = f"{path.parent.name}/{MANIFEST}: a bundle"
    lines = without_table(text.splitlines(keepends=True), DEV_DEPENDENCIES, label)
    manifest = "".join(lines)
    try:
        rewritten = tomllib.loads(manifest)
    except tomllib.TOMLDecodeError:
        rewritten = None
    if rewritten != expected:
        raise ValueError(
            f"{path.parent.name}/{MANIFEST}: write the {DEV_DEPENDENCIES} table "
            "as one block that a blank line ends"
        )
    return manifest


def source_bytes(path: Path, workspace=WORKSPACE):
    """A source's bundled bytes, leaving member dev-dependencies out."""
    if path.name == MANIFEST and path.parent.parent == workspace / "crates":
        return member_manifest(path).encode("utf-8")
    return path.read_bytes()


def workspace_manifest(host, workspace=WORKSPACE):
    """A host's workspace manifest: the canonical one, with the members that
    the bundle carries, and in the R bundle without the wheels' profile.

    A bundle holds only its host's crates, so `crates/*` names exactly them.
    maturin 1.9.6 would rewrite an explicit list to name only the crate that it
    builds, leaving the engine crates to join as path dependencies; it keeps
    this one unchanged. The R bundle leaves out the WHEEL_PROFILE table with the
    comment above it (without_table), and the result must parse as the
    canonical manifest without that profile, so that no line of the table can
    stay behind in another one.
    """
    text = (workspace / MANIFEST).read_text(encoding="utf-8")
    lines = [
        'members = ["crates/*"]\n' if line.startswith("members = ") else line
        for line in text.splitlines(keepends=True)
        if not line.startswith("default-members = ")
    ]
    expected = tomllib.loads(text)
    expected["workspace"]["members"] = ["crates/*"]
    expected["workspace"].pop("default-members", None)
    if host == "r":
        lines = without_table(lines, f"[profile.{WHEEL_PROFILE}]")
        profiles = expected.get("profile", {})
        profiles.pop(WHEEL_PROFILE, None)
        if not profiles:
            expected.pop("profile", None)
    manifest = "".join(lines)
    try:
        rewritten = tomllib.loads(manifest)
    except tomllib.TOMLDecodeError:
        rewritten = None
    if rewritten != expected:
        raise ValueError(
            "rust/Cargo.toml: write the workspace members and default-members "
            f"on one line each, and the [profile.{WHEEL_PROFILE}] table as one "
            "block that a blank line ends"
        )
    return manifest


def lock_packages(text, label):
    """Return a lock's format version and its packages by (name, version, source)."""
    lock = tomllib.loads(text)
    packages = {}
    for package in lock.get("package", []):
        key = (package["name"], package["version"], package.get("source"))
        if key in packages:
            raise ValueError(f"{label}: {key[0]} {key[1]} is locked twice")
        packages[key] = package
    return lock.get("version"), packages


def dependency_edges(packages, label):
    """Resolve each package's dependency entries to the packages they name.

    Cargo writes an entry as `name`, adding the version, and then the source
    in parentheses, only where a shorter entry would be ambiguous in that lock.
    """
    edges = {}
    for key, package in packages.items():
        resolved = set()
        for entry in package.get("dependencies", []):
            name, *rest = entry.split(" ")
            found = [
                other
                for other in packages
                if other[0] == name
                and (not rest or other[1] == rest[0])
                and (len(rest) < 2 or rest[1] == f"({other[2]})")
            ]
            if len(found) != 1:
                raise ValueError(
                    f"{label}: {key[0]} {key[1]} names the dependency {entry}, "
                    "which matches no single locked package"
                )
            resolved.add(found[0])
        edges[key] = resolved
    return edges


def verify_lock(trimmed, canonical, roots):
    """Check a trimmed lock against the canonical lock, crate by crate.

    Every retained package must be the canonical entry, with the same version,
    source and Cargo checksum, and depend only on packages that its canonical
    entry depends on. The retained packages must be exactly those that the
    `roots`, the bundle's members, reach through the trimmed lock's own
    dependencies. Returns the retained packages.
    """
    label = "trimmed Cargo.lock"
    version, kept = lock_packages(trimmed, label)
    canonical_version, locked = lock_packages(canonical, "canonical Cargo.lock")
    if version != canonical_version:
        raise ValueError(f"{label}: lock format {version} is not the canonical one")
    kept_edges = dependency_edges(kept, label)
    locked_edges = dependency_edges(locked, "canonical Cargo.lock")

    def entry(package):
        return {
            field: value for field, value in package.items() if field != "dependencies"
        }

    for key, package in sorted(kept.items(), key=lambda item: str(item[0])):
        if key not in locked or entry(package) != entry(locked[key]):
            raise ValueError(
                f"{label}: {key[0]} {key[1]} differs from the canonical lock"
            )
        for name, version_, _ in sorted(kept_edges[key] - locked_edges[key], key=str):
            raise ValueError(
                f"{label}: {key[0]} {key[1]} depends on {name} {version_}, "
                "which the canonical lock does not record"
            )
    start = [key for key in kept if key[2] is None and key[0] in roots]
    missing = set(roots) - {key[0] for key in start}
    if missing:
        raise ValueError(
            f"{label}: the bundle member {sorted(missing)[0]} is not locked"
        )
    reached = set()
    while start:
        key = start.pop()
        if key not in reached:
            reached.add(key)
            start.extend(kept_edges[key])
    for name, version_, _ in sorted(set(kept) - reached, key=str):
        raise ValueError(
            f"{label}: {name} {version_} is outside the dependency closure of "
            + ", ".join(roots)
        )
    return kept


def trim_lock(workspace: Path, canonical: str) -> str:
    """Let Cargo drop the canonical packages that the workspace does not reach.

    Cargo resolves the staged workspace starting from a copy of the canonical
    lock, which holds every version it needs, and writes the lock that the
    bundle's members alone resolve to.
    """
    (workspace / LOCK).write_bytes(canonical.encode("utf-8"))
    subprocess.run(
        ["cargo", "metadata", "--format-version", "1"],
        cwd=workspace,
        stdout=subprocess.DEVNULL,
        check=True,
    )
    return (workspace / LOCK).read_bytes().decode("utf-8")


def stage_workspace(host, destination: Path, workspace=WORKSPACE):
    """Write a host's bundle workspace, with its verified trimmed lock, to a new
    directory. Returns the retained lock packages."""
    if not (workspace / LOCK).is_file():
        raise FileNotFoundError("canonical Cargo.lock is missing")
    canonical = (workspace / LOCK).read_bytes().decode("utf-8")
    destination.mkdir(parents=True)
    (destination / MANIFEST).write_bytes(
        workspace_manifest(host, workspace).encode("utf-8")
    )
    for relative, path in sources(host, workspace):
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(source_bytes(path, workspace))
    trimmed = trim_lock(destination, canonical)
    return verify_lock(trimmed, canonical, members(host))


def stage(host, destination: Path, workspace=WORKSPACE):
    """Write a host's bundle to a new directory: the workspace in rust/, the
    crates.io crates of its lock in vendor/ and the configuration that makes
    Cargo read them in .cargo/config.toml. Returns the vendored crates."""
    if destination.exists():
        raise FileExistsError(f"{destination} already exists")
    stage_workspace(host, destination / "rust", workspace)
    # Cargo would read the source replacement while vendoring, so it is
    # written last.
    crates = vendor_crates.vendor(
        destination / "rust",
        destination / vendor_crates.DIRECTORY,
        check_minimum_rust.TARGETS,
    )
    (destination / ".cargo").mkdir()
    (destination / ".cargo/config.toml").write_bytes(CARGO_CONFIG.encode("utf-8"))
    return crates


def python_project(destination: Path, project=ROOT / "python"):
    """Copy the Python package files beside a bundle, pointing maturin at it.

    The archive's component inventory is the wheels' without the Rust standard
    library, which a build from the archive links from whatever toolchain
    builds it.
    """
    # Imported here because the inventory's generator imports this module.
    from tools.packaging import generate_component_inventory as inventory

    pyproject = (project / "pyproject.toml").read_bytes().decode("utf-8")
    old = f'manifest-path = "../rust/{PYTHON_MANIFEST_PATH}"'
    if pyproject.count(old) != 1:
        raise ValueError(f"python/pyproject.toml must declare {old} once")
    new = old.replace("../rust/", "rust/")
    (destination / "pyproject.toml").write_bytes(
        pyproject.replace(old, new).encode("utf-8")
    )
    declared = tomllib.loads(pyproject)["project"]
    for name in (declared["readme"], *declared["license-files"]):
        if name == inventory.NAME:
            wheels = (project / name).read_bytes().decode("utf-8")
            text = inventory.source_inventory(wheels)
            (destination / name).write_bytes(text.encode("utf-8"))
        else:
            shutil.copyfile(project / name, destination / name)
    shutil.copytree(project / "src", destination / "src")


def python_archive(output: Path) -> Path:
    """Build the Python source archive from the bundle into `output`."""
    pyproject = tomllib.loads(
        (ROOT / "python/pyproject.toml").read_text(encoding="utf-8")
    )
    [requirement] = pyproject["build-system"]["requires"]
    found = subprocess.run(
        ["maturin", "--version"], capture_output=True, text=True, check=True
    ).stdout.split()
    if requirement != f"maturin=={found[-1]}":
        raise ValueError(f"maturin {found[-1]} is not the build system's {requirement}")
    project = pyproject["project"]
    archive = output / f"{project['name']}-{project['version']}.tar.gz"
    if archive.exists():
        raise FileExistsError(f"{archive} already exists")
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="yamaa-python-bundle-") as scratch:
        root = Path(scratch) / "yamaa"
        stage("python", root)
        python_project(root)
        # maturin lists each crate's files with `cargo package --list`, which
        # can create a target directory inside the crate it lists.
        environment = dict(os.environ, CARGO_TARGET_DIR=str(Path(scratch) / "target"))
        subprocess.run(
            ["maturin", "sdist", "--out", str(output.resolve())],
            cwd=root,
            env=environment,
            check=True,
        )
    if not archive.is_file():
        raise ValueError(f"maturin did not write {archive.name}")
    return archive


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    sdist = commands.add_parser("sdist", help="build the Python source archive")
    sdist.add_argument("directory", type=Path)
    bundle = commands.add_parser("bundle", help="write a host's bundle")
    bundle.add_argument("host", choices=sorted(BINDINGS))
    bundle.add_argument("destination", type=Path)
    arguments = parser.parse_args()
    if arguments.command == "sdist":
        print(python_archive(arguments.directory))
    else:
        crates = stage(arguments.host, arguments.destination.resolve())
        stubs = sum(not whole for *_, whole in crates)
        print(
            f"{arguments.destination}: {len(crates)} vendored crates, {stubs} "
            "of them manifests alone"
        )


if __name__ == "__main__":
    main()
