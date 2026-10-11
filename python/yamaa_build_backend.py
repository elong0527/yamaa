"""Delegate Python builds to maturin while preserving the locked Rust workspace."""

import argparse
import io
import json
import os
import re
import tarfile
import tempfile
import tomllib
from pathlib import Path, PurePosixPath


def native_test_fixtures(workspace: Path):
    """Select only the explicitly listed original Cargo test inputs and truth."""
    repository = workspace.parent
    relative_manifest = "rust/qualification/native-source-fixtures.json"
    manifest = repository / relative_manifest
    declaration = json.loads(manifest.read_bytes())
    if (
        not isinstance(declaration, dict)
        or set(declaration) != {"format", "files"}
        or declaration["format"] != "yamaa-native-source-fixtures/1"
        or not isinstance(declaration["files"], list)
    ):
        raise ValueError("invalid native source fixture manifest")
    files = declaration["files"]
    if len(files) > 4096:
        raise ValueError("native source fixture manifest is too large")
    selected = [(relative_manifest, manifest)]
    seen = set()
    for name in files:
        if (
            not isinstance(name, str)
            or len(name) > 65_536
            or not name.startswith("benchmarks/")
            or "\\" in name
            or ":" in name
            or any(part in {"", ".", ".."} for part in name.split("/"))
            or name in seen
        ):
            raise ValueError("invalid or duplicate native source fixture path")
        seen.add(name)
        selected.append((name, repository / name))
    for name, path in selected:
        if not path.is_file():
            raise ValueError(f"native source fixture is missing: {name}")
        parent = repository
        for part in name.split("/"):
            parent = parent / part
            if parent.is_symlink():
                raise ValueError("native source fixtures must not be symlinks")
    # This conservative source-text scan admits direct include_str!/include_bytes!
    # calls only with ordinary quoted literals and matching (), [] or {}.
    # Raw/dynamic arguments and comment-separated tokens are refused. Inputs
    # outside Cargo's workspace need an explicit declaration before an archive.
    include = re.compile(r"\binclude_(?:str|bytes)\b")
    invocation = re.compile(r"\s*!\s*([\(\[\{])\s*")
    argument = re.compile(r'"((?:[^"\\]|\\.)*)"\s*,?\s*([\)\]\}])')
    closing = {"(": ")", "[": "]", "{": "}"}
    for source in sorted((workspace / "crates").rglob("*.rs")):
        if any(part in {"target", ".git", "__pycache__"} for part in source.parts):
            continue
        text = source.read_text(encoding="utf-8")
        for directive in include.finditer(text):
            call = invocation.match(text, directive.end())
            literal = argument.match(text, call.end()) if call is not None else None
            if literal is None or literal.group(2) != closing[call.group(1)]:
                raise ValueError(
                    "native compile fixture arguments must be ordinary string literals"
                )
            written = literal.group(1)
            if (
                "\\" in written
                or ":" in written
                or PurePosixPath(written).is_absolute()
            ):
                raise ValueError(
                    "native compile fixture paths must be unescaped relative paths"
                )
            path = source.parent.resolve()
            for part in PurePosixPath(written).parts:
                path = path / part
                if path.is_symlink():
                    raise ValueError("native compile fixtures must not be symlinks")
            path = path.resolve()
            if path.is_relative_to(workspace.resolve()):
                continue
            if not path.is_relative_to(repository.resolve()):
                raise ValueError("native compile fixture escapes the repository")
            relative = path.relative_to(repository.resolve()).as_posix()
            if relative not in seen:
                raise ValueError(f"native compile fixture is not declared: {relative}")
    return selected


def member_sources(member_root: Path):
    """Restore only the Cargo manifest and Rust library sources, without links."""
    manifest = member_root / "Cargo.toml"
    sources = member_root / "src"
    if member_root.parent.is_symlink() or member_root.is_symlink():
        raise ValueError("restored workspace members must not be symlinks")
    if not manifest.is_file():
        raise ValueError("restored workspace member is missing Cargo.toml")
    for path in (manifest, sources):
        if path.is_symlink():
            raise ValueError("restored workspace sources must not be symlinks")
    yield manifest
    for path in sorted(sources.rglob("*")):
        if path.is_symlink():
            raise ValueError("restored workspace sources must not be symlinks")
        if path.is_file() and path.suffix == ".rs":
            yield path


def preserve_workspace(archive: Path, workspace: Path, fixture_sources=()) -> None:
    """Keep all lock members without adding an R dependency to the Python binary."""
    manifest = (workspace / "Cargo.toml").read_bytes()
    members = tomllib.loads(manifest.decode())["workspace"]["members"]
    lock = (workspace / "Cargo.lock").read_bytes()
    toolchain = workspace / "rust-toolchain.toml"
    toolchain_bytes = toolchain.read_bytes()
    temporary = None
    try:
        with tarfile.open(archive, "r:gz") as source:
            entries = source.getmembers()
            names = {entry.name for entry in entries}
            if len(names) != len(entries):
                raise ValueError("source archive contains duplicate members")
            if any(
                PurePosixPath(name).is_absolute()
                or "\\" in name
                or ":" in name.split("/")[0]
                or any(part in {"", ".", ".."} for part in name.rstrip("/").split("/"))
                for name in names
            ):
                raise ValueError("source archive has an invalid package root")
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

            toolchain_name = f"{root}/rust/rust-toolchain.toml"
            additions = []
            if toolchain_name in names:
                entry = source.getmember(toolchain_name)
                if (
                    not entry.isfile()
                    or source.extractfile(entry).read() != toolchain_bytes
                ):
                    raise ValueError(
                        "source archive changed the Rust toolchain declaration"
                    )
            else:
                additions.append((toolchain_name, toolchain))
            for member in members:
                parts = member.split("/")
                if (
                    len(parts) != 2
                    or parts[0] != "crates"
                    or parts[1] in {"", ".", ".."}
                ):
                    raise ValueError(f"unsupported workspace member: {member}")
                member_root = workspace / member
                # A member manifest already in the archive does not imply its
                # sources are intact: verify every member source (manifest
                # included) against the local tree so a pruned or altered
                # member cannot slip through the restore.
                for path in member_sources(member_root):
                    relative = path.relative_to(member_root)
                    name = f"{root}/rust/{member}/{relative.as_posix()}"
                    if name in names:
                        entry = source.getmember(name)
                        if (
                            not entry.isfile()
                            or source.extractfile(entry).read() != path.read_bytes()
                        ):
                            raise ValueError(
                                "source archive changed a restored member source"
                            )
                    else:
                        additions.append((name, path))

            for relative, path in fixture_sources:
                if (
                    not isinstance(relative, str)
                    or "\\" in relative
                    or ":" in relative
                    or any(part in {"", ".", ".."} for part in relative.split("/"))
                    or not path.is_file()
                    or path.is_symlink()
                ):
                    raise ValueError("invalid native source fixture")
                name = f"{root}/{relative}"
                if name in names:
                    entry = source.getmember(name)
                    if (
                        not entry.isfile()
                        or source.extractfile(entry).read() != path.read_bytes()
                    ):
                        raise ValueError("source archive changed a native test fixture")
                else:
                    additions.append((name, path))

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
        # Windows refuses replacement while the source archive is still open.
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
    fixtures = native_test_fixtures(workspace)
    filename = maturin.build_sdist(sdist_directory, config_settings)
    preserve_workspace(Path(sdist_directory) / filename, workspace, fixtures)
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
