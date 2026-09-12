"""R020 publication: atomic replacement that never leaves a partial artifact."""

from __future__ import annotations

from pathlib import Path

import pytest

from yamaa.io import publish_artifact
from yamaa.io.publish import ArtifactError


def test_publish_writes_complete_bytes(tmp_path: Path) -> None:
    target = tmp_path / "dm.csv"

    assert publish_artifact(target, b"STUDYID\nS1\n") == target
    assert target.read_bytes() == b"STUDYID\nS1\n"


def test_publish_replaces_atomically_and_leaves_no_residue(
    tmp_path: Path,
) -> None:
    target = tmp_path / "dm.csv"
    target.write_bytes(b"old\n")

    publish_artifact(target, b"new\n")

    assert target.read_bytes() == b"new\n"
    assert [path.name for path in tmp_path.iterdir()] == ["dm.csv"]


def test_failed_publication_preserves_the_previous_artifact(
    tmp_path: Path,
) -> None:
    target = tmp_path / "missing-dir" / "dm.csv"

    with pytest.raises(ArtifactError) as raised:
        publish_artifact(target, b"new\n")

    assert raised.value.condition == "publication_failed"
    assert [path.name for path in tmp_path.iterdir()] == []
