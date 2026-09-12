"""Resolve project files and retain immutable byte snapshots under R021."""

from __future__ import annotations

import hashlib
import os
import re
import stat
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field

_URI_SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")
_DRIVE_LETTER = re.compile(r"^[A-Za-z]:")


class ResourceSnapshot(BaseModel):
    """One immutable copy of an accepted physical file's bytes."""

    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)

    sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    content: bytes


class ResourceFailure(ValueError):
    """A stable R021 failure that never exposes a host path."""

    def __init__(
        self,
        condition: str,
        written_path: str,
        *,
        phase: Literal["validation", "ingest"] = "validation",
    ) -> None:
        self.phase = phase
        self.condition = condition
        self.written_path = written_path
        super().__init__(f"{condition}: {written_path!r}")


def classify_project_path(written_path: str) -> str | None:
    """Return the first written-form condition from R021, if any."""
    if not written_path:
        return "resource_path_not_relative"
    if written_path.startswith("/") or "\\" in written_path:
        return "resource_path_not_relative"
    if _DRIVE_LETTER.match(written_path):
        return "resource_path_not_relative"
    if _URI_SCHEME.match(written_path):
        return "resource_path_uri_scheme"
    segments = written_path.split("/")
    if any(segment == "" for segment in segments):
        return "resource_path_not_normalized"
    return None


class ProjectResources:
    """Capture and verify files relative to one approved project root."""

    def __init__(
        self,
        project_root: str | Path,
        *,
        base_directory: str | Path | None = None,
    ) -> None:
        try:
            root = Path(project_root).resolve(strict=True)
        except OSError as error:
            raise ValueError("approved project root must exist") from error
        if not root.is_dir():
            raise ValueError("approved project root must be a directory")

        base = Path(base_directory) if base_directory is not None else root
        try:
            base = base.resolve(strict=True)
        except OSError as error:
            raise ValueError("resource base directory must exist") from error
        if not base.is_dir():
            raise ValueError("resource base must be a directory")

        self._root = root
        self._base = base
        self._by_identity: dict[tuple[int, int], ResourceSnapshot] = {}
        self._paths: dict[int, list[tuple[Path, str]]] = {}
        self._path_snapshots: dict[Path, ResourceSnapshot] = {}
        self._capture_reads = 0

    @property
    def capture_reads(self) -> int:
        """Number of physical files read to create snapshots."""
        return self._capture_reads

    def _resolve(self, written_path: str) -> Path:
        condition = classify_project_path(written_path)
        if condition is not None:
            raise ResourceFailure(condition, written_path)

        try:
            depth = len(self._base.relative_to(self._root).parts)
        except ValueError as error:
            raise ResourceFailure(
                "resource_path_outside_project", written_path
            ) from error
        for segment in written_path.split("/"):
            if segment == ".":
                continue
            if segment == "..":
                depth -= 1
                if depth < 0:
                    raise ResourceFailure("resource_path_outside_project", written_path)
            else:
                depth += 1

        current = self._base
        segments = written_path.split("/")
        for index, segment in enumerate(segments):
            current = current / segment
            if current.is_symlink():
                raise ResourceFailure("resource_path_symlink", written_path)
            if not current.exists():
                raise ResourceFailure("resource_path_missing", written_path)
            final = index == len(segments) - 1
            if final:
                if not current.is_file():
                    raise ResourceFailure(
                        "resource_path_not_regular_file", written_path
                    )
            elif not current.is_dir():
                raise ResourceFailure("resource_path_not_regular_file", written_path)

        try:
            canonical = current.resolve(strict=True)
            canonical.relative_to(self._root)
        except (OSError, ValueError) as error:
            raise ResourceFailure(
                "resource_path_outside_project", written_path
            ) from error
        return canonical

    def capture(self, written_path: str) -> ResourceSnapshot:
        """Validate a path and return its run-wide immutable snapshot."""
        path = self._resolve(written_path)
        accepted = self._path_snapshots.get(path)
        if accepted is not None:
            return accepted

        try:
            path_status = path.stat(follow_symlinks=False)
        except OSError as error:
            raise ResourceFailure("resource_path_missing", written_path) from error
        if not stat.S_ISREG(path_status.st_mode):
            raise ResourceFailure("resource_path_not_regular_file", written_path)

        identity = (path_status.st_dev, path_status.st_ino)
        accepted = self._by_identity.get(identity)
        if accepted is not None:
            self._path_snapshots[path] = accepted
            self._paths[id(accepted)].append((path, written_path))
            return accepted

        try:
            flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
            descriptor = os.open(path, flags)
            with os.fdopen(descriptor, "rb") as handle:
                opened_status = os.fstat(handle.fileno())
                if not stat.S_ISREG(opened_status.st_mode):
                    raise ResourceFailure(
                        "resource_path_not_regular_file", written_path
                    )
                content = handle.read()
        except ResourceFailure:
            raise
        except OSError as error:
            raise ResourceFailure("resource_path_missing", written_path) from error

        opened_identity = (opened_status.st_dev, opened_status.st_ino)
        if opened_identity != identity:
            raise ResourceFailure(
                "resource_path_content_changed", written_path, phase="ingest"
            )

        snapshot = ResourceSnapshot(
            sha256=hashlib.sha256(content).hexdigest(), content=content
        )
        self._capture_reads += 1
        self._by_identity[identity] = snapshot
        self._path_snapshots[path] = snapshot
        self._paths[id(snapshot)] = [(path, written_path)]
        return snapshot

    def validate(self, written_path: str) -> None:
        """Validate a written path and file kind without reading its bytes."""
        self._resolve(written_path)

    def verify(self, snapshot: ResourceSnapshot) -> None:
        """Verify accepted path bytes before parsing the retained snapshot."""
        paths = self._paths.get(id(snapshot))
        if not paths:
            raise ValueError("snapshot was not captured by this project")
        for path, written_path in paths:
            try:
                self._resolve(written_path)
                status = path.stat(follow_symlinks=False)
                if not stat.S_ISREG(status.st_mode):
                    raise OSError("resource is no longer a regular file")
                flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
                descriptor = os.open(path, flags)
                with os.fdopen(descriptor, "rb") as handle:
                    opened_status = os.fstat(handle.fileno())
                    if not stat.S_ISREG(opened_status.st_mode):
                        raise OSError("resource is no longer a regular file")
                    content = handle.read()
            except (OSError, ResourceFailure) as error:
                raise ResourceFailure(
                    "resource_path_content_changed",
                    written_path,
                    phase="ingest",
                ) from error
            digest = hashlib.sha256(content).hexdigest()
            if digest != snapshot.sha256:
                raise ResourceFailure(
                    "resource_path_content_changed",
                    written_path,
                    phase="ingest",
                )
