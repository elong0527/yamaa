from __future__ import annotations

import os
import socket
import tempfile
from collections.abc import Iterator
from pathlib import Path, PurePath

import pytest
from pydantic import ValidationError

from yamaa.io.project import (
    ProjectResources,
    ResourceFailure,
    classify_project_path,
)


@pytest.fixture
def unresolved_directory() -> Iterator[str]:
    """A temporary directory spelled the way the platform hands it out.

    pytest resolves tmp_path, which hides the symbolic link macOS puts in
    front of /var. R021-15 exempts the anchor from the no-link rejection, so
    the spelling a user would actually write has to reach the tests unresolved.
    """
    with tempfile.TemporaryDirectory(prefix="yamaa-") as directory:
        yield directory


def rooted(directory: str | Path, *segments: str) -> str:
    """Spell a rooted written path the way a specification writes one."""
    return "/".join((PurePath(directory).as_posix(), *segments))


@pytest.mark.parametrize(
    ("written", "condition"),
    [
        ("https://example.org/dm.csv", "resource_path_uri_scheme"),
        ("file:input/dm.csv", "resource_path_uri_scheme"),
        ("C:data/dm.csv", "resource_path_uri_scheme"),
        ("", "resource_path_not_normalized"),
        (r"input\dm.csv", "resource_path_not_normalized"),
        ("input//dm.csv", "resource_path_not_normalized"),
        ("input/", "resource_path_not_normalized"),
        ("/", "resource_path_not_normalized"),
        ("/data/./lbref.csv", "resource_path_not_normalized"),
        ("/data/../lbref.csv", "resource_path_not_normalized"),
        ("input/dm.csv", None),
        ("input/../input/dm.csv", None),
        ("/data/lbref.csv", None),
        ("C:/data/lbref.csv", None),
    ],
)
def test_classifies_written_paths_in_rule_order(
    written: str, condition: str | None
) -> None:
    assert classify_project_path(written) == condition


@pytest.mark.parametrize("written", ["/etc/passwd", "C:/data/dm.csv"])
def test_rooted_path_naming_no_approved_root_is_rejected(
    tmp_path: Path, written: str
) -> None:
    resources = ProjectResources(tmp_path)

    with pytest.raises(ResourceFailure) as raised:
        resources.capture(written)

    assert raised.value.condition == "resource_path_not_relative"
    assert str(tmp_path) not in str(raised.value)


def test_captures_a_rooted_path_under_an_approved_data_root(tmp_path: Path) -> None:
    project = tmp_path / "project"
    project.mkdir()
    store = tmp_path / "store"
    store.mkdir()
    (store / "lbref.csv").write_bytes(b"LBTESTCD\nALT\n")
    resources = ProjectResources(project, data_roots=[store])

    snapshot = resources.capture(rooted(store, "lbref.csv"))

    assert snapshot.content == b"LBTESTCD\nALT\n"
    assert resources.capture_reads == 1


def test_captures_a_rooted_path_through_an_unresolved_system_directory(
    tmp_path: Path, unresolved_directory: str
) -> None:
    store = Path(unresolved_directory)
    (store / "lbref.csv").write_bytes(b"LBTESTCD\nALT\n")
    resources = ProjectResources(tmp_path, data_roots=[unresolved_directory])

    snapshot = resources.capture(rooted(unresolved_directory, "lbref.csv"))

    assert snapshot.content == b"LBTESTCD\nALT\n"


def test_accepts_a_rooted_path_whose_anchor_is_a_symbolic_link(
    tmp_path: Path,
) -> None:
    store = tmp_path / "store"
    store.mkdir()
    (store / "lbref.csv").write_bytes(b"LBTESTCD\nALT\n")
    linked = tmp_path / "data"
    linked.symlink_to(store, target_is_directory=True)
    project = tmp_path / "project"
    project.mkdir()
    resources = ProjectResources(project, data_roots=[linked])

    snapshot = resources.capture(rooted(linked, "lbref.csv"))

    assert snapshot.content == b"LBTESTCD\nALT\n"


def test_rejects_a_symbolic_link_below_a_rooted_anchor(tmp_path: Path) -> None:
    project = tmp_path / "project"
    project.mkdir()
    store = tmp_path / "store"
    store.mkdir()
    (store / "lbref.csv").write_bytes(b"LBTESTCD\nALT\n")
    (store / "alias.csv").symlink_to("lbref.csv")
    resources = ProjectResources(project, data_roots=[store])

    with pytest.raises(ResourceFailure) as raised:
        resources.capture(rooted(store, "alias.csv"))

    assert raised.value.condition == "resource_path_symlink"


def test_rooted_path_naming_an_approved_root_itself_is_not_a_regular_file(
    tmp_path: Path,
) -> None:
    project = tmp_path / "project"
    project.mkdir()
    store = tmp_path / "store"
    store.mkdir()
    resources = ProjectResources(project, data_roots=[store])

    with pytest.raises(ResourceFailure) as raised:
        resources.capture(rooted(store))

    assert raised.value.condition == "resource_path_not_regular_file"


def test_an_approved_data_root_admits_no_relative_escape(tmp_path: Path) -> None:
    project = tmp_path / "project"
    project.mkdir()
    store = tmp_path / "store"
    store.mkdir()
    (store / "lbref.csv").write_bytes(b"LBTESTCD\nALT\n")
    resources = ProjectResources(project, data_roots=[store])

    with pytest.raises(ResourceFailure) as raised:
        resources.capture("../store/lbref.csv")

    assert raised.value.condition == "resource_path_outside_project"


def test_one_snapshot_for_a_relative_and_an_unresolved_rooted_spelling(
    unresolved_directory: str,
) -> None:
    root = Path(unresolved_directory)
    (root / "input").mkdir()
    (root / "input" / "dm.csv").write_bytes(b"ID\n001\n")
    resources = ProjectResources(unresolved_directory)

    first = resources.capture(rooted(unresolved_directory, "input", "dm.csv"))
    second = resources.capture("input/dm.csv")

    assert first is second
    assert resources.capture_reads == 1


def test_the_longest_approved_root_anchors_a_rooted_path(tmp_path: Path) -> None:
    project = tmp_path / "project"
    project.mkdir()
    outer = tmp_path / "store"
    outer.mkdir()
    held = tmp_path / "pilot7-data"
    held.mkdir()
    (held / "lbref.csv").write_bytes(b"LBTESTCD\nALT\n")
    inner = outer / "pilot7"
    inner.symlink_to(held, target_is_directory=True)
    written = rooted(inner, "lbref.csv")

    # Anchored at the outer root, "pilot7" is a component the walk rejects.
    with pytest.raises(ResourceFailure) as raised:
        ProjectResources(project, data_roots=[outer]).capture(written)
    assert raised.value.condition == "resource_path_symlink"

    # Approving the inner root anchors the same spelling one level deeper.
    resources = ProjectResources(project, data_roots=[outer, inner])
    assert resources.capture(written).content == b"LBTESTCD\nALT\n"


def test_dot_segments_resolve_within_root(tmp_path: Path) -> None:
    (tmp_path / "dm.csv").write_text("ID\n001\n")
    resources = ProjectResources(tmp_path)

    assert resources.capture("./dm.csv").sha256 == resources.capture("dm.csv").sha256


def test_parent_traversal_within_root_shares_snapshot(tmp_path: Path) -> None:
    (tmp_path / "input").mkdir()
    (tmp_path / "input" / "dm.csv").write_text("ID\n001\n")
    resources = ProjectResources(tmp_path)

    assert (
        resources.capture("input/../input/dm.csv").sha256
        == resources.capture("input/dm.csv").sha256
    )
    assert resources.capture_reads == 1


def test_parent_traversal_above_root_is_outside_project(tmp_path: Path) -> None:
    resources = ProjectResources(tmp_path)

    with pytest.raises(ResourceFailure) as raised:
        resources.capture("../dm.csv")

    assert raised.value.condition == "resource_path_outside_project"


def test_captures_one_immutable_snapshot_per_physical_file(tmp_path: Path) -> None:
    source = tmp_path / "dm.csv"
    source.write_bytes(b"ID\n001\n")
    alias = tmp_path / "alias.csv"
    os.link(source, alias)
    resources = ProjectResources(tmp_path)

    first = resources.capture("dm.csv")
    second = resources.capture("alias.csv")

    assert first is second
    assert first.content == b"ID\n001\n"
    assert resources.capture_reads == 1
    with pytest.raises(ValidationError):
        first.content = b"changed"  # type: ignore[misc]


def test_detects_changed_content_without_replacing_snapshot(tmp_path: Path) -> None:
    source = tmp_path / "dm.csv"
    source.write_bytes(b"ID\n001\n")
    resources = ProjectResources(tmp_path)
    snapshot = resources.capture("dm.csv")
    source.write_bytes(b"ID\n002\n")

    with pytest.raises(ResourceFailure) as raised:
        resources.verify(snapshot)

    assert raised.value.phase == "ingest"
    assert raised.value.condition == "resource_path_content_changed"
    assert raised.value.written_path == "dm.csv"
    assert snapshot.content == b"ID\n001\n"


def test_rejects_a_symlink_replacement_before_ingestion(tmp_path: Path) -> None:
    source = tmp_path / "dm.csv"
    source.write_bytes(b"ID\n001\n")
    replacement = tmp_path / "replacement.csv"
    replacement.write_bytes(b"ID\n001\n")
    resources = ProjectResources(tmp_path)
    snapshot = resources.capture("dm.csv")
    source.unlink()
    source.symlink_to(replacement)

    with pytest.raises(ResourceFailure) as raised:
        resources.verify(snapshot)

    assert raised.value.phase == "ingest"
    assert raised.value.condition == "resource_path_content_changed"
    assert raised.value.written_path == "dm.csv"


@pytest.mark.parametrize("operation", ["capture", "verify"])
def test_intermediate_symlink_race_cannot_open_outside_project(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    operation: str,
) -> None:
    project = tmp_path / "project"
    source_directory = project / "input"
    source_directory.mkdir(parents=True)
    (source_directory / "dm.csv").write_bytes(b"ID\n001\n")
    outside = tmp_path / "outside"
    outside.mkdir()
    outside_source = outside / "dm.csv"
    outside_source.write_bytes(b"ID\n001\n")
    resources = ProjectResources(project)
    snapshot = resources.capture("input/dm.csv") if operation == "verify" else None

    original_open = os.open
    replacement = tmp_path / "original-input"
    outside_identity = (outside_source.stat().st_dev, outside_source.stat().st_ino)
    replaced = False
    opened_outside = False

    def replacing_open(
        path: str | bytes | os.PathLike[str] | os.PathLike[bytes],
        flags: int,
        mode: int = 0o777,
        *,
        dir_fd: int | None = None,
    ) -> int:
        nonlocal opened_outside, replaced
        if not replaced and Path(path).name == "dm.csv":
            source_directory.rename(replacement)
            source_directory.symlink_to(outside, target_is_directory=True)
            replaced = True
        descriptor = original_open(path, flags, mode, dir_fd=dir_fd)
        status = os.fstat(descriptor)
        opened_outside |= (status.st_dev, status.st_ino) == outside_identity
        return descriptor

    monkeypatch.setattr(os, "open", replacing_open)

    with pytest.raises(ResourceFailure) as raised:
        if snapshot is None:
            resources.capture("input/dm.csv")
        else:
            resources.verify(snapshot)

    assert replaced
    assert not opened_outside
    assert raised.value.phase == "ingest"
    assert raised.value.condition == "resource_path_content_changed"
    assert raised.value.written_path == "input/dm.csv"


def test_rejects_symlinks_at_final_and_intermediate_components(
    tmp_path: Path,
) -> None:
    (tmp_path / "inside").mkdir()
    (tmp_path / "inside" / "dm.csv").write_text("ID\n1\n")
    (tmp_path / "linked.csv").symlink_to("inside/dm.csv")
    (tmp_path / "linked").symlink_to("inside", target_is_directory=True)
    resources = ProjectResources(tmp_path)

    for written in ("linked.csv", "linked/dm.csv"):
        with pytest.raises(ResourceFailure) as raised:
            resources.capture(written)
        assert raised.value.condition == "resource_path_symlink"


def test_rejects_non_regular_runtime_file_kinds() -> None:
    # macOS limits AF_UNIX paths to 104 bytes, shorter than pytest's tmp_path.
    with tempfile.TemporaryDirectory(prefix="yamaa-", dir="/tmp") as temporary:
        root = Path(temporary)
        (root / "directory").mkdir()
        fifo = root / "stream.csv"
        os.mkfifo(fifo)
        socket_path = root / "socket.csv"
        server = socket.socket(socket.AF_UNIX)
        server.bind(str(socket_path))
        resources = ProjectResources(root)
        try:
            for written in ("directory", "stream.csv", "socket.csv"):
                with pytest.raises(ResourceFailure) as raised:
                    resources.validate(written)
                assert raised.value.condition == "resource_path_not_regular_file"
        finally:
            server.close()


def test_rejects_file_as_intermediate_component(tmp_path: Path) -> None:
    (tmp_path / "input").write_text("not a directory")
    resources = ProjectResources(tmp_path)

    with pytest.raises(ResourceFailure) as raised:
        resources.validate("input/dm.csv")

    assert raised.value.condition == "resource_path_not_regular_file"


def test_rejects_resolution_outside_narrower_project(tmp_path: Path) -> None:
    project = tmp_path / "project"
    layer = tmp_path / "layer"
    project.mkdir()
    layer.mkdir()
    (layer / "dm.csv").write_text("ID\n1\n")
    resources = ProjectResources(project, base_directory=layer)

    with pytest.raises(ResourceFailure) as raised:
        resources.validate("dm.csv")

    assert raised.value.condition == "resource_path_outside_project"


def test_failure_text_never_exposes_host_paths(tmp_path: Path) -> None:
    resources = ProjectResources(tmp_path)

    with pytest.raises(ResourceFailure) as raised:
        resources.capture("input/missing.csv")

    assert str(tmp_path) not in str(raised.value)
    assert str(raised.value) == "resource_path_missing: 'input/missing.csv'"
