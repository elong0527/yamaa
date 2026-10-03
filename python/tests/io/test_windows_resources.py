"""Windows reparse points must preserve the R021 approved-root boundary."""

from __future__ import annotations

import errno
import os
import subprocess
from pathlib import Path

import pytest

from yamaa.io.project import ProjectResources, ResourceFailure

pytestmark = pytest.mark.skipif(os.name != "nt", reason="requires Windows file APIs")


def _junction(link: Path, target: Path) -> None:
    """Create a Windows directory junction without requiring symlink privileges."""
    subprocess.run(
        ["cmd", "/c", "mklink", "/J", str(link), str(target)],
        check=True,
        capture_output=True,
    )


def test_junction_below_an_approved_root_is_rejected(tmp_path: Path) -> None:
    """A junction below an approved root cannot expose its target's bytes."""
    project = tmp_path / "project"
    project.mkdir()
    outside = tmp_path / "outside"
    outside.mkdir()
    (outside / "dm.csv").write_bytes(b"ID\n001\n")
    _junction(project / "input", outside)
    resources = ProjectResources(project)

    with pytest.raises(ResourceFailure) as raised:
        resources.capture("input/dm.csv")

    assert raised.value.condition == "resource_path_symlink"
    assert resources.capture_reads == 0


def test_junction_can_itself_be_an_approved_root(tmp_path: Path) -> None:
    """Approving the junction itself fixes the root at its resolved directory."""
    store = tmp_path / "store"
    store.mkdir()
    (store / "dm.csv").write_bytes(b"ID\n001\n")
    link = tmp_path / "data"
    _junction(link, store)
    resources = ProjectResources(link)

    assert resources.capture(f"{link.as_posix()}/dm.csv").content == b"ID\n001\n"


def test_junction_replacement_is_detected_before_ingestion(tmp_path: Path) -> None:
    """A new junction cannot stand in for a previously captured source directory."""
    project = tmp_path / "project"
    source = project / "input"
    source.mkdir(parents=True)
    (source / "dm.csv").write_bytes(b"ID\n001\n")
    outside = tmp_path / "outside"
    outside.mkdir()
    (outside / "dm.csv").write_bytes(b"ID\n001\n")
    resources = ProjectResources(project)
    snapshot = resources.capture("input/dm.csv")
    source.rename(project / "original-input")
    _junction(source, outside)

    with pytest.raises(ResourceFailure) as raised:
        resources.verify(snapshot)

    assert raised.value.phase == "ingest"
    assert raised.value.condition == "resource_path_content_changed"


def test_alternate_data_stream_is_not_a_child_file(tmp_path: Path) -> None:
    """Native child opens reject alternate streams even on an existing file."""
    (tmp_path / "input").mkdir()
    (tmp_path / "input/dm.csv").write_bytes(b"ID\n001\n")
    (tmp_path / "input/dm.csv:hidden").write_bytes(b"not a dataset")
    resources = ProjectResources(tmp_path)

    with pytest.raises(ResourceFailure) as raised:
        resources.capture("input/dm.csv:hidden")

    assert raised.value.condition == "resource_path_missing"
    assert resources.capture_reads == 0


@pytest.mark.parametrize("operation", ["open_directory", "open_file", "stat_child"])
@pytest.mark.parametrize(
    "component",
    ["", ".", "..", "input/dm.csv", "input\\dm.csv", "dm.csv:hidden", "dm\0.csv"],
)
def test_invalid_child_names_are_rejected_before_native_open(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    operation: str,
    component: str,
) -> None:
    """Every child helper refuses names that could change its parent boundary."""
    from yamaa.io import _windows

    def unexpected_open(*args: object) -> None:
        """Fail if validation lets an invalid name reach NtCreateFile."""
        pytest.fail("an invalid child name reached NtCreateFile")

    descriptor = _windows.open_directory(tmp_path.resolve())
    monkeypatch.setattr(_windows, "_nt_create_file", unexpected_open)
    try:
        with pytest.raises(OSError) as raised:
            getattr(_windows, operation)(component, dir_fd=descriptor)

        assert raised.value.errno == errno.EINVAL
    finally:
        os.close(descriptor)
