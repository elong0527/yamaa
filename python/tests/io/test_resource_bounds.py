"""Capture ceilings and verification bounds preserve retained resource authority."""

from __future__ import annotations

import os
from pathlib import Path
from typing import BinaryIO, Self

import pytest

from yamaa.io.project import ProjectResources, ResourceByteLimit, ResourceFailure


class _ReadProbe:
    """Observe actual descriptor reads and optionally grow the file after stat."""

    def __init__(
        self, monkeypatch: pytest.MonkeyPatch, grow: Path | None = None
    ) -> None:
        self.requests: list[int] = []
        self.bytes_read = 0
        original = os.fdopen
        probe = self

        class Handle:
            def __init__(self, descriptor: int, mode: str) -> None:
                self.handle: BinaryIO = original(descriptor, mode)

            def __enter__(self) -> Self:
                return self

            def __exit__(self, *args: object) -> None:
                self.handle.close()

            def fileno(self) -> int:
                return self.handle.fileno()

            def read(self, size: int = -1) -> bytes:
                assert size >= 0, "bounded operation requested an unbounded read"
                if grow is not None and not probe.requests:
                    with grow.open("ab") as writer:
                        writer.write(b"x" * 1024 * 1024)
                probe.requests.append(size)
                content = self.handle.read(size)
                probe.bytes_read += len(content)
                return content

        monkeypatch.setattr(os, "fdopen", Handle)


@pytest.mark.parametrize("content", [b"", b"ID\n001\n", b"\x00\xff\r\n"])
def test_capture_accepts_exact_ceiling_and_reuses_snapshot(
    tmp_path: Path, content: bytes, monkeypatch: pytest.MonkeyPatch
) -> None:
    (tmp_path / "source.csv").write_bytes(content)
    resources = ProjectResources(tmp_path)
    probe = _ReadProbe(monkeypatch)

    snapshot = resources.capture("source.csv", maximum=len(content))
    reads = list(probe.requests)
    assert snapshot.content == content
    assert resources.capture("source.csv", maximum=len(content)) is snapshot
    assert probe.requests == reads
    assert probe.bytes_read == len(content)
    assert resources.capture_reads == 1


def test_large_ceiling_does_not_allocate_or_request_the_entire_ceiling(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    (tmp_path / "source.csv").write_bytes(b"small")
    resources = ProjectResources(tmp_path)
    probe = _ReadProbe(monkeypatch)

    assert resources.capture("source.csv", maximum=2**100).content == b"small"
    assert probe.bytes_read == 5
    assert all(0 < size <= 64 * 1024 for size in probe.requests)


def test_oversize_stat_refuses_before_read_and_does_not_cache(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    source = tmp_path / "source.csv"
    source.write_bytes(b"too large")
    resources = ProjectResources(tmp_path)
    probe = _ReadProbe(monkeypatch)

    with pytest.raises(ResourceByteLimit) as raised:
        resources.capture("source.csv", maximum=3)

    assert raised.value.maximum == 3
    assert not hasattr(raised.value, "condition")
    assert not hasattr(raised.value, "requirement")
    assert probe.requests == []
    assert resources.capture_reads == 0
    source.write_bytes(b"new")
    assert resources.capture("source.csv", maximum=3).content == b"new"
    assert resources.capture_reads == 1


def test_cached_alias_refusal_does_not_add_a_verification_dependency(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    source = tmp_path / "source.csv"
    source.write_bytes(b"retained")
    alias = tmp_path / "alias.csv"
    os.link(source, alias)
    resources = ProjectResources(tmp_path)
    snapshot = resources.capture("source.csv")
    probe = _ReadProbe(monkeypatch)

    for name in ("source.csv", "alias.csv"):
        with pytest.raises(ResourceByteLimit):
            resources.capture(name, maximum=7)
    assert probe.requests == []
    assert resources.capture_reads == 1
    alias.unlink()
    resources.verify(snapshot)
    assert snapshot.content == b"retained"
    assert probe.bytes_read == 8


def test_growth_after_stat_stops_at_one_byte_over_ceiling(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    source = tmp_path / "source.csv"
    source.write_bytes(b"held")
    resources = ProjectResources(tmp_path)
    probe = _ReadProbe(monkeypatch, grow=source)

    with pytest.raises(ResourceByteLimit):
        resources.capture("source.csv", maximum=4)

    assert probe.bytes_read == 5
    assert probe.requests == [5]
    assert resources.capture_reads == 0
    source.write_bytes(b"next")
    assert resources.capture("source.csv", maximum=4).content == b"next"
    assert resources.capture_reads == 1


def test_verification_detects_extension_with_bounded_prefix(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    source = tmp_path / "source.csv"
    source.write_bytes(b"held")
    resources = ProjectResources(tmp_path)
    snapshot = resources.capture("source.csv")
    source.write_bytes(b"held" + b"x" * 1024 * 1024)
    probe = _ReadProbe(monkeypatch)

    with pytest.raises(ResourceFailure) as raised:
        resources.verify(snapshot)

    assert raised.value.condition == "resource_path_content_changed"
    assert raised.value.phase == "ingest"
    assert raised.value.written_path == "source.csv"
    assert probe.bytes_read == 5
    assert probe.requests == [5]
    assert snapshot.content == b"held"
    assert resources.capture_reads == 1


@pytest.mark.parametrize("maximum", [-1, True, False, 1.5, "1"])
def test_invalid_ceiling_fails_before_resource_resolution(
    tmp_path: Path, maximum: object, monkeypatch: pytest.MonkeyPatch
) -> None:
    resources = ProjectResources(tmp_path)

    def unexpected_resolution(*args: object) -> None:
        pytest.fail("invalid ceiling reached resource resolution")

    monkeypatch.setattr(resources, "_open_resource", unexpected_resolution)
    with pytest.raises(TypeError, match="resource byte ceiling"):
        resources.capture("absent.csv", maximum=maximum)  # type: ignore[arg-type]
    assert resources.capture_reads == 0
