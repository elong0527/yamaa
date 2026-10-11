"""Regression tests for the sdist build backend's workspace restore."""

import importlib.util
import io
import tarfile
from pathlib import Path

import pytest

_BACKEND = Path(__file__).resolve().parents[1] / "yamaa_build_backend.py"
_SPEC = importlib.util.spec_from_file_location("yamaa_build_backend", _BACKEND)
_BACKEND_MODULE = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(_BACKEND_MODULE)
preserve_workspace = _BACKEND_MODULE.preserve_workspace


def _make_workspace(root: Path) -> Path:
    workspace = root / "rust"
    (workspace / "crates" / "foo" / "src").mkdir(parents=True)
    (workspace / "Cargo.toml").write_bytes(b'[workspace]\nmembers = ["crates/foo"]\n')
    (workspace / "Cargo.lock").write_bytes(b"lock-bytes")
    (workspace / "rust-toolchain.toml").write_bytes(b"toolchain-bytes")
    (workspace / "crates" / "foo" / "Cargo.toml").write_bytes(
        b'[package]\nname = "foo"\n'
    )
    (workspace / "crates" / "foo" / "src" / "lib.rs").write_bytes(b"pub fn f() {}\n")
    return workspace


def _make_archive(path: Path, entries: list[tuple[str, bytes]]) -> Path:
    with tarfile.open(path, "w:gz") as target:
        for name, payload in entries:
            info = tarfile.TarInfo(name)
            info.size = len(payload)
            target.addfile(info, io.BytesIO(payload))
    return path


def _archive_payloads(path: Path) -> dict[str, bytes]:
    with tarfile.open(path, "r:gz") as source:
        return {
            member.name: source.extractfile(member).read()
            for member in source.getmembers()
            if member.isfile()
        }


_BASE_ENTRIES = [
    ("pkg-0.1/rust/Cargo.toml", b"stale manifest\n"),
    ("pkg-0.1/rust/Cargo.lock", b"lock-bytes"),
]


def test_member_sources_restored_when_manifest_already_present(
    tmp_path: Path,
) -> None:
    # A member whose manifest survived in the sdist but whose sources did
    # not must still have every source restored.
    workspace = _make_workspace(tmp_path)
    archive = _make_archive(
        tmp_path / "sdist.tar.gz",
        _BASE_ENTRIES
        + [("pkg-0.1/rust/crates/foo/Cargo.toml", b'[package]\nname = "foo"\n')],
    )
    preserve_workspace(archive, workspace)
    payloads = _archive_payloads(archive)
    assert payloads["pkg-0.1/rust/crates/foo/src/lib.rs"] == b"pub fn f() {}\n"


def test_tampered_member_source_detected_when_manifest_already_present(
    tmp_path: Path,
) -> None:
    # An altered member source must raise even when the member manifest is
    # already in the archive.
    workspace = _make_workspace(tmp_path)
    archive = _make_archive(
        tmp_path / "sdist.tar.gz",
        _BASE_ENTRIES
        + [
            ("pkg-0.1/rust/crates/foo/Cargo.toml", b'[package]\nname = "foo"\n'),
            (
                "pkg-0.1/rust/crates/foo/src/lib.rs",
                b"pub fn f() { /* tampered */ }\n",
            ),
        ],
    )
    with pytest.raises(
        ValueError, match="source archive changed a restored member source"
    ):
        preserve_workspace(archive, workspace)


def test_intact_member_archive_preserved(tmp_path: Path) -> None:
    # An intact archive keeps every entry; the workspace manifest is still
    # replaced with the local canonical copy.
    workspace = _make_workspace(tmp_path)
    archive = _make_archive(
        tmp_path / "sdist.tar.gz",
        _BASE_ENTRIES
        + [
            ("pkg-0.1/rust/rust-toolchain.toml", b"toolchain-bytes"),
            ("pkg-0.1/rust/crates/foo/Cargo.toml", b'[package]\nname = "foo"\n'),
            ("pkg-0.1/rust/crates/foo/src/lib.rs", b"pub fn f() {}\n"),
        ],
    )
    preserve_workspace(archive, workspace)
    payloads = _archive_payloads(archive)
    assert (
        payloads["pkg-0.1/rust/Cargo.toml"] == (workspace / "Cargo.toml").read_bytes()
    )
    assert payloads["pkg-0.1/rust/crates/foo/src/lib.rs"] == b"pub fn f() {}\n"
