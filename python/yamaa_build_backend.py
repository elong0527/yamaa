"""Delegate Python builds to maturin while preserving the locked Rust workspace."""

import argparse
import io
import os
import tarfile
import tempfile
import tomllib
from pathlib import Path, PurePosixPath


def preserve_workspace(archive: Path, workspace: Path) -> None:
    """Keep all lock members without adding an R dependency to the Python binary."""
    manifest = (workspace / "Cargo.toml").read_bytes()
    members = tomllib.loads(manifest.decode())["workspace"]["members"]
    lock = (workspace / "Cargo.lock").read_bytes()
    temporary = None
    try:
        with tarfile.open(archive, "r:gz") as source:
            entries = source.getmembers()
            names = {entry.name for entry in entries}
            if len(names) != len(entries):
                raise ValueError("source archive contains duplicate members")
            roots = {PurePosixPath(name).parts[0] for name in names}
            if len(roots) != 1:
                raise ValueError("source archive must have one package root")
            root = roots.pop()
            if root in {".", ".."}:
                raise ValueError("source archive has an invalid package root")
            manifest_name = f"{root}/rust/Cargo.toml"
            if (
                manifest_name not in names
                or not source.getmember(manifest_name).isfile()
            ):
                raise ValueError("source archive is missing the workspace manifest")
            lock_name = f"{root}/rust/Cargo.lock"
            if lock_name not in names or not source.getmember(lock_name).isfile():
                raise ValueError("source archive is missing the canonical Cargo.lock")
            if source.extractfile(lock_name).read() != lock:
                raise ValueError("source archive changed the canonical Cargo.lock")

            additions = []
            for member in members:
                parts = PurePosixPath(member).parts
                if len(parts) != 2 or parts[0] != "crates":
                    raise ValueError(f"unsupported workspace member: {member}")
                member_root = workspace / member
                if f"{root}/rust/{member}/Cargo.toml" in names:
                    continue
                for path in sorted(member_root.rglob("*")):
                    relative = path.relative_to(member_root)
                    if {"target", "__pycache__"}.intersection(relative.parts):
                        continue
                    if path.name == "Cargo.lock" or path.suffix in {".pyc", ".pyo"}:
                        continue
                    if path.is_file():
                        additions.append(
                            (f"{root}/rust/{member}/{relative.as_posix()}", path)
                        )

            with tempfile.NamedTemporaryFile(
                dir=archive.parent, suffix=".tar.gz", delete=False
            ) as file:
                temporary = Path(file.name)
            with tarfile.open(temporary, "w:gz") as target:
                for entry in entries:
                    if entry.name == manifest_name:
                        entry.size = len(manifest)
                        target.addfile(entry, io.BytesIO(manifest))
                    else:
                        payload = source.extractfile(entry) if entry.isfile() else None
                        target.addfile(entry, payload)
                for name, path in additions:
                    payload = path.read_bytes()
                    entry = tarfile.TarInfo(name)
                    entry.size = len(payload)
                    entry.mode = 0o644
                    target.addfile(entry, io.BytesIO(payload))
            os.replace(temporary, archive)
            temporary = None
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def build_sdist(sdist_directory, config_settings=None):
    import maturin

    project = Path(__file__).resolve().parent
    config = tomllib.loads((project / "pyproject.toml").read_text())
    workspace = (
        (project / config["tool"]["maturin"]["manifest-path"]).resolve().parents[2]
    )
    filename = maturin.build_sdist(sdist_directory, config_settings)
    preserve_workspace(Path(sdist_directory) / filename, workspace)
    return filename


def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    import maturin

    return maturin.build_wheel(wheel_directory, config_settings, metadata_directory)


def build_editable(wheel_directory, config_settings=None, metadata_directory=None):
    import maturin

    return maturin.build_editable(wheel_directory, config_settings, metadata_directory)


def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
    import maturin

    return maturin.prepare_metadata_for_build_wheel(metadata_directory, config_settings)


prepare_metadata_for_build_editable = prepare_metadata_for_build_wheel


def get_requires_for_build_wheel(config_settings=None):
    import maturin

    return maturin.get_requires_for_build_wheel(config_settings)


get_requires_for_build_editable = get_requires_for_build_wheel


def get_requires_for_build_sdist(config_settings=None):
    import maturin

    return maturin.get_requires_for_build_sdist(config_settings)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    args.destination.mkdir(parents=True, exist_ok=True)
    print(build_sdist(str(args.destination.resolve())))
