"""Verify locked Cargo resolution and unchanged locks in source packages."""

import argparse
import json
import subprocess
import tarfile
from pathlib import Path, PurePosixPath

WORKSPACE = Path(__file__).resolve().parents[1]


def locked_metadata(workspace: Path) -> str:
    if not (workspace / "Cargo.lock").is_file():
        raise ValueError("canonical Cargo.lock is missing")
    return subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        cwd=workspace,
        text=True,
    )


def check_archive(
    path: Path, lock: bytes, manifest: bytes, toolchain: bytes | None = None
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archives", nargs="*", type=Path)
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    lock = (WORKSPACE / "Cargo.lock").read_bytes()
    manifest = (WORKSPACE / "Cargo.toml").read_bytes()
    toolchain = (WORKSPACE / "rust-toolchain.toml").read_bytes()
    metadata = locked_metadata(WORKSPACE)
    for archive in args.archives:
        check_archive(archive, lock, manifest, toolchain)
        print(
            f"{archive.name}: canonical lock, workspace manifest and toolchain bytes retained"
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
