"""Verify locked Cargo resolution and the engine bundles that source packages carry.

Without archives, it checks that the canonical rust/Cargo.lock is current and
that each host's bundle trims it to a lock that verifies crate by crate against
it, as `make check-repository` runs it, and that the R package's build strips
nothing. With archives, it checks the engine bundle (#3030 D55) of each Python
source archive and R source package against the checkout, that the archive
holds no Rust toolchain file anywhere, and that an R source package's build
strips nothing.
"""

import argparse
import dataclasses
import io
import json
import subprocess
import tarfile
import tempfile
import tomllib
from pathlib import Path, PurePosixPath

from tools.paths import ROOT, WORKSPACE
from tools.staging import engine_bundle, vendor_crates


@dataclasses.dataclass(frozen=True)
class Layout:
    """Where a host's source archive keeps its bundle, below the archive root."""

    workspace: str
    config: str
    # The xz tar of the vendored crates, or None when they sit in vendor/.
    packed: str | None = None

    def owned(self):
        """The directories that hold only the bundle's files."""
        return (self.workspace, self.config.rpartition("/")[0] + "/")


LAYOUTS = {
    "python": Layout("rust/", ".cargo/config.toml"),
    # The Makevars unpack the crates and copy the configuration into the
    # build's Cargo home (tools/staging/stage_r_package.py).
    "r": Layout("src/rust/", "src/rust/vendor-config.toml", "src/rust/vendor.tar.xz"),
}
# The R package's shared object keeps its symbols: CRAN's Repository Policy asks
# a package not to remove diagnostic information such as symbols in shared
# objects. Its Makevars build with the release profile; the Python wheels strip
# theirs with the dist profile (rust/Cargo.toml).
R_PACKAGE = ROOT / "R/yamaa"
R_MAKEVARS = ("src/Makevars.in", "src/Makevars.win.in")


def makevars_profile(label, makevars):
    """The Cargo profile that a Makevars template's one cargo build selects.

    Refuses a command that sets strip itself, as a variable, flag or
    configuration value.
    """
    commands = [
        line
        for line in makevars.replace("\\\n", " ").splitlines()
        if '"$(CARGO)" build ' in line
    ]
    if len(commands) != 1:
        raise ValueError(f"{label}: expected one cargo build command")
    if "strip" in commands[0].lower():
        raise ValueError(f"{label}: the cargo build command sets strip")
    words = commands[0].split()
    profiles = []
    for index, word in enumerate(words):
        if word in ("--release", "-r"):
            profiles.append("release")
        elif word == "--profile" and index + 1 < len(words):
            profiles.append(words[index + 1])
        elif word.startswith("--profile="):
            profiles.append(word.removeprefix("--profile="))
    if len(profiles) > 1:
        raise ValueError(f"{label}: the cargo build selects more than one profile")
    return profiles[0] if profiles else "dev"


def check_r_symbols(label, manifest, config, makevars):
    """Refuse an R build that would strip its shared object.

    `manifest` and `config` are the texts of the bundle's workspace manifest
    and Cargo configuration, and `makevars` maps each Makevars template's name
    to its text. Neither the profile that a template's cargo build selects nor
    a profile that it inherits from may set strip, other than to false or
    "none", in the manifest or in the configuration.
    """
    sources = [
        ("the Cargo configuration", tomllib.loads(config).get("profile", {})),
        ("the workspace manifest", tomllib.loads(manifest).get("profile", {})),
    ]
    for name, text in makevars.items():
        selected = makevars_profile(f"{label}: {name}", text)
        profile, seen = selected, set()
        while profile is not None and profile not in seen:
            seen.add(profile)
            for where, profiles in sources:
                if profiles.get(profile, {}).get("strip", False) not in (False, "none"):
                    use = (
                        f"which {name} builds with"
                        if profile == selected
                        else f"which the {selected} profile of {name} inherits"
                    )
                    raise ValueError(
                        f"{label}: {where} sets strip in the {profile} profile, "
                        f"{use}; the R package keeps its symbols, as CRAN's "
                        "Repository Policy asks"
                    )
            profile = next(
                (
                    profiles[profile]["inherits"]
                    for _, profiles in sources
                    if "inherits" in profiles.get(profile, {})
                ),
                None,
            )


def locked_metadata(workspace: Path) -> str:
    if not (workspace / "Cargo.lock").is_file():
        raise ValueError("canonical Cargo.lock is missing")
    return subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        cwd=workspace,
        text=True,
    )


def bundle_files(host, workspace=WORKSPACE):
    """The bytes of each file of a host's bundle that comes from the checkout,
    by its path in the source archive."""
    layout = LAYOUTS[host]
    files = {
        layout.workspace + relative: source.read_bytes()
        for relative, source in engine_bundle.sources(host, workspace)
    }
    files[layout.workspace + "Cargo.toml"] = engine_bundle.workspace_manifest(
        workspace
    ).encode()
    files[layout.config] = engine_bundle.CARGO_CONFIG.encode()
    return files


def refuse_unsafe(label, entries):
    """Refuse duplicate, linked, absolute or ambiguous archive members."""
    if len({entry.name for entry in entries}) != len(entries):
        raise ValueError(f"{label}: duplicate archive member")
    if any(not (entry.isfile() or entry.isdir()) for entry in entries):
        raise ValueError(f"{label}: archive must not contain links")
    if any(
        "\\" in entry.name
        or any(part in {"", ".", ".."} for part in entry.name.rstrip("/").split("/"))
        for entry in entries
    ):
        raise ValueError(f"{label}: archive has an invalid member name")


def toolchain_files(names):
    """The archive member names that are Rust toolchain files."""
    return sorted(
        name
        for name in names
        if PurePosixPath(name).name in vendor_crates.TOOLCHAIN_FILES
    )


def packed_crates(label, data):
    """Read an xz tar of vendored crates in one pass.

    Refuses unsafe members, members outside vendor/ and Rust toolchain files.
    Returns the names of its files below vendor/ and the parsed
    `.cargo-checksum.json` of each crate, by crate directory.
    """
    entries, records = [], {}
    with tarfile.open(fileobj=io.BytesIO(data), mode="r|xz") as packed:
        for entry in packed:
            entries.append(entry)
            parts = entry.name.split("/")
            if entry.isfile() and parts[2:] == [vendor_crates.CHECKSUMS]:
                records[parts[1]] = json.loads(packed.extractfile(entry).read())
    refuse_unsafe(label, entries)
    top = vendor_crates.DIRECTORY
    if any(entry.name.split("/")[0] != top for entry in entries):
        raise ValueError(f"{label}: archive members must sit below {top}/")
    for name in toolchain_files(entry.name for entry in entries):
        raise ValueError(f"{label} carries {name}")
    files = [entry.name.removeprefix(f"{top}/") for entry in entries if entry.isfile()]
    return files, records


def check_vendored(label, files, records, kept):
    """Check vendored crates against the lock that selects them.

    `files` are the vendored file names below vendor/. Every crates.io package
    of the lock must have one crate, holding exactly the files that its
    `.cargo-checksum.json` lists.
    """
    crates = {}
    for name in files:
        crate, _, relative = name.partition("/")
        crates.setdefault(crate, set()).add(relative)
    registry = {f"{name}-{version}" for name, version, source in kept if source}
    for crate in sorted(set(crates) ^ registry):
        raise ValueError(
            f"{label}: vendored crate {crate} differs from the lock's crates"
        )
    for crate, names in sorted(crates.items()):
        if crate not in records:
            raise ValueError(
                f"{label}: vendored crate {crate} has no {vendor_crates.CHECKSUMS}"
            )
        if names - {vendor_crates.CHECKSUMS} != set(records[crate]["files"]):
            raise ValueError(
                f"{label}: vendored crate {crate} differs from the files "
                "its .cargo-checksum.json lists"
            )


def check_bundle_archive(path: Path, workspace=WORKSPACE):
    """Check a source archive's engine bundle against the checkout.

    The archive must hold no Rust toolchain file anywhere, the vendored crates
    included. It must carry exactly the bundle's Rust sources, manifest and
    Cargo configuration, a lock that verifies against the canonical lock, and
    each crates.io crate of that lock, with every file that its
    `.cargo-checksum.json` lists and no other: in vendor/ in a Python source
    archive, and in src/rust/vendor.tar.xz in an R source package. An R source
    package's Makevars must build with a profile that strips nothing
    (check_r_symbols). Returns the host and the lock's packages.
    """
    canonical = (workspace / "Cargo.lock").read_bytes().decode()
    with tarfile.open(path, "r:gz") as archive:
        entries = archive.getmembers()
        refuse_unsafe(path.name, entries)
        roots = {entry.name.split("/")[0] for entry in entries}
        if len(roots) != 1:
            raise ValueError(f"{path.name}: source archive must have one root")
        root = roots.pop()
        for name in toolchain_files(
            entry.name.removeprefix(f"{root}/") for entry in entries
        ):
            raise ValueError(f"{path.name}: source archive carries {name}")
        files = {
            entry.name.removeprefix(f"{root}/"): entry
            for entry in entries
            if entry.isfile()
        }

        def read(name):
            if name not in files:
                raise ValueError(f"{path.name}: the bundle is missing {name}")
            return archive.extractfile(files[name]).read()

        host = "r" if LAYOUTS["r"].workspace + "Cargo.lock" in files else "python"
        layout = LAYOUTS[host]
        expected = bundle_files(host, workspace)
        lock = layout.workspace + "Cargo.lock"
        carried = {
            name
            for name in files
            if name.startswith(layout.owned()) and name != layout.packed
        }
        for name in sorted(carried ^ (set(expected) | {lock})):
            raise ValueError(
                f"{path.name}: the bundle differs from the checkout at {name}"
            )
        for name, payload in sorted(expected.items()):
            if read(name) != payload:
                raise ValueError(f"{path.name}: {name} differs from the checkout")
        if host == "r":
            templates = {}
            for name in R_MAKEVARS:
                if name not in files:
                    raise ValueError(f"{path.name}: the R package is missing {name}")
                templates[name] = archive.extractfile(files[name]).read().decode()
            check_r_symbols(
                path.name,
                read(layout.workspace + "Cargo.toml").decode(),
                read(layout.config).decode(),
                templates,
            )
        kept = engine_bundle.verify_lock(
            read(lock).decode(), canonical, engine_bundle.members(host)
        )
        if layout.packed is None:
            label, top = path.name, vendor_crates.DIRECTORY + "/"
            vendored = [
                name.removeprefix(top) for name in files if name.startswith(top)
            ]
            records = {
                name.partition("/")[0]: json.loads(read(top + name))
                for name in vendored
                if name.partition("/")[2] == vendor_crates.CHECKSUMS
            }
        else:
            label = f"{path.name}: {layout.packed}"
            vendored, records = packed_crates(label, read(layout.packed))
    check_vendored(label, vendored, records, kept)
    return host, kept


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archives", nargs="*", type=Path)
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    metadata = locked_metadata(WORKSPACE)
    for archive in args.archives:
        host, kept = check_bundle_archive(archive)
        print(
            f"{archive.name}: {host} engine bundle with {len(kept)} canonical lock "
            "packages and their vendored crates, and no Rust toolchain file"
        )
    if not args.archives:
        with tempfile.TemporaryDirectory(prefix="yamaa-bundle-lock-") as scratch:
            for host in sorted(engine_bundle.BINDINGS):
                kept = engine_bundle.stage_workspace(host, Path(scratch) / host)
                print(
                    f"{host} bundle: the trimmed lock keeps {len(kept)} canonical "
                    "packages, the dependency closure of "
                    + ", ".join(engine_bundle.members(host))
                )
        check_r_symbols(
            "R/yamaa",
            engine_bundle.workspace_manifest(),
            engine_bundle.CARGO_CONFIG,
            {
                name: (R_PACKAGE / name).read_text(encoding="utf-8")
                for name in R_MAKEVARS
            },
        )
        print("R package: the profile that its Makevars build with strips nothing")
    if args.output_dir:
        args.output_dir.mkdir(parents=True, exist_ok=True)
        (args.output_dir / "cargo-metadata.json").write_text(metadata)
        for name, command in (
            ("cargo-version.txt", ["cargo", "--version"]),
            ("rustc-version.txt", ["rustc", "-vV"]),
            (
                "cargo-features.txt",
                [
                    "cargo",
                    "tree",
                    "--locked",
                    "--workspace",
                    "--target",
                    "all",
                    "-e",
                    "features",
                ],
            ),
        ):
            output = subprocess.check_output(command, cwd=WORKSPACE, text=True)
            (args.output_dir / name).write_text(output)
    print(f"Locked Cargo graph: {len(json.loads(metadata)['packages'])} packages")


if __name__ == "__main__":
    main()
