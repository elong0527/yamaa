from __future__ import annotations

import os
from pathlib import Path

import pytest

from yamaa.artifacts import (
    ArtifactError,
    ArtifactTarget,
    build_artifact,
    publish_artifact,
    render_artifact,
)
from yamaa.io.polars import frame_from_values
from yamaa.models import TypedColumn
from yamaa.specification.models import Output

COLUMNS = (
    TypedColumn(name="USUBJID", type="str"),
    TypedColumn(name="AGE", type="int"),
)


def artifact(path: str = "adsl.csv", age: int = 34) -> object:
    table = frame_from_values(COLUMNS, [["S-1", age]])
    return build_artifact(
        table, Output(path=path, columns=["USUBJID", "AGE"]), ["USUBJID"]
    )


def test_publication_replaces_the_target_and_leaves_no_residue(
    tmp_path: Path,
) -> None:
    target = ArtifactTarget(tmp_path / "adsl.csv")
    target.path.write_bytes(b"previous\n")

    published = publish_artifact(target, artifact())

    assert published == target.path
    assert published.read_bytes() == b"USUBJID,AGE\nS-1,34\n"
    assert sorted(entry.name for entry in tmp_path.iterdir()) == ["adsl.csv"]


def test_a_failed_replacement_keeps_the_previous_artifact_and_removes_the_residue(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    target = ArtifactTarget(tmp_path / "adsl.csv")
    target.path.write_bytes(b"previous\n")
    staged: list[Path] = []

    def refuse(source: str, destination: str) -> None:
        staged.append(Path(source))
        raise OSError("replacement refused")

    monkeypatch.setattr(os, "replace", refuse)

    with pytest.raises(ArtifactError) as raised:
        publish_artifact(target, artifact(age=99))

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.phase == "output"
    assert diagnostic.condition == "publication_failed"
    assert diagnostic.requirement == "R020-47"
    assert diagnostic.context["target"] == str(target.path)
    # R020-39 keeps the temporary file beside the target so the replacement
    # stays on one filesystem, and R020-40 removes it when one fails.
    assert staged[0].parent == tmp_path
    assert target.path.read_bytes() == b"previous\n"
    assert sorted(entry.name for entry in tmp_path.iterdir()) == ["adsl.csv"]


def test_a_target_is_one_explicit_permitted_regular_file(tmp_path: Path) -> None:
    linked = tmp_path / "linked.csv"
    linked.symlink_to(tmp_path / "elsewhere.csv")

    with pytest.raises(ValueError, match="absolute path"):
        ArtifactTarget("adsl.csv")
    with pytest.raises(ValueError, match="existing directory"):
        ArtifactTarget(tmp_path / "absent" / "adsl.csv")
    with pytest.raises(ValueError, match="symbolic link"):
        ArtifactTarget(linked)
    with pytest.raises(ValueError, match="regular file"):
        ArtifactTarget(tmp_path)
    with pytest.raises(ValueError, match="no R020 profile"):
        ArtifactTarget(tmp_path / "adsl.xpt")


def test_a_target_must_agree_with_the_profile_the_artifact_was_built_for(
    tmp_path: Path,
) -> None:
    target = ArtifactTarget(tmp_path / "adsl.parquet")

    with pytest.raises(ValueError, match="cannot be published"):
        publish_artifact(target, artifact())

    assert not target.path.exists()


def test_the_published_bytes_are_the_rendered_bytes_for_either_profile(
    tmp_path: Path,
) -> None:
    for name in ("adsl.csv", "adsl.parquet"):
        target = ArtifactTarget(tmp_path / name)
        built = artifact(name)

        published = publish_artifact(target, built)

        assert published.read_bytes() == render_artifact(built)
