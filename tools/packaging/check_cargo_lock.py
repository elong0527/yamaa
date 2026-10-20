"""Verify locked Cargo resolution and the locks that source packages carry.

Without archives, it checks that the canonical rust/Cargo.lock is current and
that each host's bundle trims it to a lock that verifies crate by crate against
it, as `make check-repository` runs it. With archives, it checks the Python
source archive's engine bundle (#3030 D55) and the R source package's unchanged
copy of the canonical workspace, lock and toolchain file.
"""

import argparse
import json
import subprocess
import tarfile
import tempfile
from pathlib import Path, PurePosixPath

from tools.paths import WORKSPACE
from tools.staging import engine_bundle


def locked_metadata(workspace: Path) -> str:
    if not (workspace / "Cargo.lock").is_file():
        raise ValueError("canonical Cargo.lock is missing")
    return subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        cwd=workspace,
        text=True,
    )


def check_archive(
    path: Path,
    lock: bytes,
    manifest: bytes,
    toolchain: bytes | None = None,
    vendored=(),
) -> None:
    with tarfile.open(path, "r:gz") as archive:
        members = [
            m
            for m in archive.getmembers()
            if PurePosixPath(m.name).name == "Cargo.lock"
        ]
        if len(members) != 1:
            raise ValueError(f"{path.name}: expected exactly one canonical Cargo.lock")
        member = members[0]
        location = PurePosixPath(member.name)
        parts = member.name.split("/")
        if (
            location.is_absolute()
            or "\\" in member.name
            or ":" in parts[0]
            or any(part in {"", ".", ".."} for part in parts)
            or tuple(parts[1:])
            not in {
                ("rust", "Cargo.lock"),
                ("src", "rust", "Cargo.lock"),
            }
        ):
            raise ValueError(f"{path.name}: unexpected Cargo.lock location")
        if not member.isfile():
            raise ValueError(f"{path.name}: Cargo.lock must be a regular file")
        if member.size != len(lock) or archive.extractfile(member).read() != lock:
            raise ValueError(
                f"{path.name}: Cargo.lock differs from the canonical bytes"
            )
        manifest_name = str(PurePosixPath(member.name).with_name("Cargo.toml"))
        manifests = [m for m in archive.getmembers() if m.name == manifest_name]
        if len(manifests) != 1 or not manifests[0].isfile():
            raise ValueError(f"{path.name}: expected one regular workspace manifest")
        if archive.extractfile(manifests[0]).read() != manifest:
            raise ValueError(
                f"{path.name}: workspace manifest differs from the canonical bytes"
            )
        if toolchain is not None:
            toolchain_name = str(location.with_name("rust-toolchain.toml"))
            declarations = [m for m in archive.getmembers() if m.name == toolchain_name]
            if len(declarations) != 1 or not declarations[0].isfile():
                raise ValueError(
                    f"{path.name}: expected one regular Rust toolchain declaration"
                )
            if archive.extractfile(declarations[0]).read() != toolchain:
                raise ValueError(
                    f"{path.name}: Rust toolchain differs from the canonical bytes"
                )
        vendored_names = {str(location.parent / relative) for relative, _ in vendored}
        if vendored and any(
            member.name.startswith(str(location.parent / "vendor") + "/")
            and not (member.isfile() or member.isdir())
            for member in archive.getmembers()
        ):
            raise ValueError(f"{path.name}: vendored crates must not contain links")
        if vendored and any(
            member.isfile()
            and member.name.startswith(str(location.parent / "vendor") + "/")
            and member.name not in vendored_names
            for member in archive.getmembers()
        ):
            raise ValueError(f"{path.name}: unexpected vendored crate file")
        for relative, payload in vendored:
            name = str(location.parent / relative)
            copies = [m for m in archive.getmembers() if m.name == name]
            if len(copies) != 1 or not copies[0].isfile():
                raise ValueError(
                    f"{path.name}: expected one regular vendored crate source"
                )
            if archive.extractfile(copies[0]).read() != payload:
                raise ValueError(
                    f"{path.name}: vendored crate source differs from canonical bytes"
                )


def bundle_files(workspace=WORKSPACE):
    """The bytes of each file of the Python bundle that comes from the checkout."""
    files = {
        f"rust/{relative}": source.read_bytes()
        for relative, source in engine_bundle.sources("python", workspace)
    }
    files["rust/Cargo.toml"] = engine_bundle.workspace_manifest(workspace).encode()
    files[".cargo/config.toml"] = engine_bundle.CARGO_CONFIG.encode()
    return files


def check_bundle_archive(path: Path, workspace=WORKSPACE):
    """Check a Python source archive's engine bundle against the checkout.

    The archive must carry exactly the bundle's Rust sources, manifest and
    Cargo configuration, a lock that verifies against the canonical lock, and
    each crates.io crate of that lock in vendor/, with every file that its
    `.cargo-checksum.json` lists and no other. No toolchain file may sit
    outside the vendored crates. Returns the lock's packages.
    """
    canonical = (workspace / "Cargo.lock").read_bytes().decode()
    expected = bundle_files(workspace)
    with tarfile.open(path, "r:gz") as archive:
        entries = archive.getmembers()
        if len({entry.name for entry in entries}) != len(entries):
            raise ValueError(f"{path.name}: duplicate source archive member")
        if any(not (entry.isfile() or entry.isdir()) for entry in entries):
            raise ValueError(f"{path.name}: source archive must not contain links")
        if any(
            "\\" in entry.name
            or any(
                part in {"", ".", ".."} for part in entry.name.rstrip("/").split("/")
            )
            for entry in entries
        ):
            raise ValueError(f"{path.name}: source archive has an invalid member name")
        roots = {entry.name.split("/")[0] for entry in entries}
        if len(roots) != 1:
            raise ValueError(f"{path.name}: source archive must have one root")
        root = roots.pop()
        files = {
            entry.name.removeprefix(f"{root}/"): entry
            for entry in entries
            if entry.isfile()
        }

        def read(name):
            if name not in files:
                raise ValueError(f"{path.name}: the bundle is missing {name}")
            return archive.extractfile(files[name]).read()

        carried = {name for name in files if name.startswith(("rust/", ".cargo/"))}
        for name in sorted(carried ^ (set(expected) | {"rust/Cargo.lock"})):
            raise ValueError(
                f"{path.name}: the bundle differs from the checkout at {name}"
            )
        for name, payload in sorted(expected.items()):
            if read(name) != payload:
                raise ValueError(f"{path.name}: {name} differs from the checkout")
        kept = engine_bundle.verify_lock(
            read("rust/Cargo.lock").decode(),
            canonical,
            engine_bundle.members("python"),
        )
        vendored = {}
        for name in files:
            if name.startswith("vendor/"):
                crate, _, relative = name.removeprefix("vendor/").partition("/")
                vendored.setdefault(crate, set()).add(relative)
        registry = {f"{name}-{version}" for name, version, source in kept if source}
        for crate in sorted(set(vendored) ^ registry):
            raise ValueError(
                f"{path.name}: vendored crate {crate} differs from the lock's crates"
            )
        for crate, names in sorted(vendored.items()):
            record = json.loads(read(f"vendor/{crate}/.cargo-checksum.json"))
            if names - {".cargo-checksum.json"} != set(record["files"]):
                raise ValueError(
                    f"{path.name}: vendored crate {crate} differs from the files "
                    "its .cargo-checksum.json lists"
                )
        for name in sorted(files):
            if not name.startswith("vendor/") and PurePosixPath(name).name in {
                "rust-toolchain",
                "rust-toolchain.toml",
            }:
                raise ValueError(f"{path.name}: source archive carries {name}")
    return kept


def is_r_source(path: Path):
    """Whether an archive is an R source package, which holds src/rust/."""
    with tarfile.open(path, "r:gz") as archive:
        return any(
            name.split("/")[1:] == ["src", "rust", "Cargo.lock"]
            for name in archive.getnames()
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archives", nargs="*", type=Path)
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    lock = (WORKSPACE / "Cargo.lock").read_bytes()
    manifest = (WORKSPACE / "Cargo.toml").read_bytes()
    toolchain = (WORKSPACE / "rust-toolchain.toml").read_bytes()
    metadata = locked_metadata(WORKSPACE)
    vendored = [
        (source.relative_to(WORKSPACE).as_posix(), source.read_bytes())
        for source in engine_bundle.vendored_sources(WORKSPACE)
    ]
    for archive in args.archives:
        if is_r_source(archive):
            check_archive(archive, lock, manifest, toolchain, vendored)
            print(
                f"{archive.name}: canonical lock, workspace manifest and toolchain "
                "bytes retained"
            )
        else:
            kept = check_bundle_archive(archive)
            print(
                f"{archive.name}: engine bundle with {len(kept)} canonical lock "
                "packages and their vendored crates"
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
