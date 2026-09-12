"""Resolve project files and retain immutable byte snapshots under R021."""

from __future__ import annotations

import hashlib
import os
import re
import stat
import weakref
from dataclasses import dataclass
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field

_URI_SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")
_DRIVE_LETTER = re.compile(r"^[A-Za-z]:")


class _PathChanged(OSError):
    """A resource component changed during a descriptor-anchored walk."""


@dataclass(frozen=True)
class _OpenedResource:
    """A regular file opened beneath the anchored project root."""

    descriptor: int
    key: tuple[str, ...]
    status: os.stat_result


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
    if "\\" in written_path:
        return "resource_path_not_relative"
    if _URI_SCHEME.match(written_path) and not _DRIVE_LETTER.match(written_path):
        return "resource_path_uri_scheme"
    segments = written_path.split("/")
    if any(segment == "" for segment in segments[1:]):
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

        try:
            base_components: tuple[str, ...] | None = base.relative_to(root).parts
        except ValueError:
            base_components = None

        required_flags = ("O_DIRECTORY", "O_NOFOLLOW")
        if (
            os.open not in os.supports_dir_fd
            or os.stat not in os.supports_dir_fd
            or os.stat not in os.supports_follow_symlinks
            or any(not hasattr(os, name) for name in required_flags)
        ):
            raise RuntimeError(
                "component-safe project resource resolution is unavailable"
            )

        root_descriptor = -1
        try:
            root_status = root.stat(follow_symlinks=False)
            root_descriptor = os.open(root, self._directory_flags())
            opened_root_status = os.fstat(root_descriptor)
        except OSError as error:
            if root_descriptor >= 0:
                os.close(root_descriptor)
            raise ValueError("approved project root must be a directory") from error
        if not stat.S_ISDIR(opened_root_status.st_mode):
            os.close(root_descriptor)
            raise ValueError("approved project root must be a directory")
        if self._entry_identity(root_status) != self._entry_identity(
            opened_root_status
        ):
            os.close(root_descriptor)
            raise ValueError("approved project root changed during selection")

        self._root_descriptor = root_descriptor
        self._root_finalizer = weakref.finalize(self, os.close, root_descriptor)
        self._base_components = base_components
        self._by_identity: dict[tuple[int, int], ResourceSnapshot] = {}
        self._paths: dict[int, list[tuple[tuple[str, ...], str]]] = {}
        self._path_snapshots: dict[tuple[str, ...], ResourceSnapshot] = {}
        self._capture_reads = 0

    @property
    def capture_reads(self) -> int:
        """Number of physical files read to create snapshots."""
        return self._capture_reads

    @staticmethod
    def _directory_flags() -> int:
        return (
            os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0)
        )

    @staticmethod
    def _file_flags() -> int:
        return (
            os.O_RDONLY
            | os.O_NOFOLLOW
            | getattr(os, "O_CLOEXEC", 0)
            | getattr(os, "O_NONBLOCK", 0)
        )

    @staticmethod
    def _entry_identity(status: os.stat_result) -> tuple[int, int, int]:
        return (status.st_dev, status.st_ino, stat.S_IFMT(status.st_mode))

    def _path_key(self, written_path: str) -> tuple[str, ...]:
        condition = classify_project_path(written_path)
        if condition is not None:
            raise ResourceFailure(condition, written_path)

        if self._base_components is None:
            raise ResourceFailure("resource_path_outside_project", written_path)

        if written_path.startswith("/") or _DRIVE_LETTER.match(written_path):
            components: list[str] = []
            for segment in written_path.split("/"):
                if segment in ("", "."):
                    continue
                if segment == "..":
                    if components:
                        components.pop()
                    continue
                components.append(segment)
            return ("", *components)

        components = list(self._base_components)
        for segment in written_path.split("/"):
            if segment == ".":
                continue
            if segment == "..":
                if not components:
                    raise ResourceFailure("resource_path_outside_project", written_path)
                components.pop()
            else:
                components.append(segment)
        return tuple(components)

    def _open_component(
        self,
        parent_descriptor: int,
        component: str,
        written_path: str,
        *,
        directory: bool,
    ) -> tuple[int, os.stat_result]:
        try:
            initial_status = os.stat(
                component,
                dir_fd=parent_descriptor,
                follow_symlinks=False,
            )
        except OSError as error:
            raise ResourceFailure("resource_path_missing", written_path) from error

        if stat.S_ISLNK(initial_status.st_mode):
            raise ResourceFailure("resource_path_symlink", written_path)
        expected_kind = stat.S_ISDIR if directory else stat.S_ISREG
        if not expected_kind(initial_status.st_mode):
            raise ResourceFailure("resource_path_not_regular_file", written_path)

        try:
            flags = self._directory_flags() if directory else self._file_flags()
            descriptor = os.open(
                component,
                flags,
                dir_fd=parent_descriptor,
            )
        except OSError as error:
            try:
                current_status = os.stat(
                    component,
                    dir_fd=parent_descriptor,
                    follow_symlinks=False,
                )
            except OSError:
                raise _PathChanged from error
            if self._entry_identity(current_status) != self._entry_identity(
                initial_status
            ):
                raise _PathChanged from error
            raise ResourceFailure("resource_path_missing", written_path) from error

        try:
            opened_status = os.fstat(descriptor)
        except OSError as error:
            os.close(descriptor)
            raise ResourceFailure("resource_path_missing", written_path) from error
        if self._entry_identity(opened_status) != self._entry_identity(initial_status):
            os.close(descriptor)
            raise _PathChanged
        return descriptor, opened_status

    def _open_resource(self, written_path: str) -> _OpenedResource:
        key = self._path_key(written_path)
        if self._base_components is None:  # Guarded by _path_key.
            raise AssertionError("unreachable project base")

        links: list[tuple[int, str, tuple[int, int, int]]] = []
        if key[:1] == ("",):
            try:
                anchor = os.open("/", self._directory_flags())
            except OSError as error:
                raise ResourceFailure("resource_path_missing", written_path) from error
            directories = [anchor]
            components = [""]
            if written_path.startswith("/"):
                segments = written_path.split("/")[1:]
            else:
                segments = written_path.split("/")
        else:
            directories = [os.dup(self._root_descriptor)]
            components = []
            for component in self._base_components:
                descriptor, status = self._open_component(
                    directories[-1], component, written_path, directory=True
                )
                links.append((directories[-1], component, self._entry_identity(status)))
                directories.append(descriptor)
                components.append(component)

            segments = written_path.split("/")
        file_descriptor: int | None = None
        try:
            for segment in segments[:-1]:
                if segment == ".":
                    continue
                if segment == "..":
                    if len(directories) > 1:
                        os.close(directories.pop())
                        links.pop()
                        components.pop()
                    continue
                descriptor, status = self._open_component(
                    directories[-1], segment, written_path, directory=True
                )
                links.append((directories[-1], segment, self._entry_identity(status)))
                directories.append(descriptor)
                components.append(segment)

            final = segments[-1]
            if final == "..":
                if len(directories) > 1:
                    os.close(directories.pop())
                    links.pop()
                    components.pop()
                final = "."
            if final == ".":
                self._verify_links(links)
                raise ResourceFailure("resource_path_not_regular_file", written_path)

            file_descriptor, file_status = self._open_component(
                directories[-1], final, written_path, directory=False
            )
            final_link = (
                directories[-1],
                final,
                self._entry_identity(file_status),
            )
            self._verify_links([*links, final_link])
            if (*components, final) != key:
                raise AssertionError(
                    "descriptor walk disagrees with path normalization"
                )

            opened = _OpenedResource(file_descriptor, key, file_status)
            file_descriptor = None
            return opened
        finally:
            if file_descriptor is not None:
                os.close(file_descriptor)
            for descriptor in reversed(directories):
                os.close(descriptor)

    def _verify_links(
        self,
        links: list[tuple[int, str, tuple[int, int, int]]],
    ) -> None:
        for parent_descriptor, component, expected_identity in links:
            try:
                status = os.stat(
                    component,
                    dir_fd=parent_descriptor,
                    follow_symlinks=False,
                )
            except OSError as error:
                raise _PathChanged from error
            if self._entry_identity(status) != expected_identity:
                raise _PathChanged

    def capture(self, written_path: str) -> ResourceSnapshot:
        """Validate a path and return its run-wide immutable snapshot."""
        try:
            opened = self._open_resource(written_path)
        except _PathChanged as error:
            raise ResourceFailure(
                "resource_path_content_changed", written_path, phase="ingest"
            ) from error

        descriptor = opened.descriptor
        try:
            accepted = self._path_snapshots.get(opened.key)
            if accepted is not None:
                return accepted

            identity = (opened.status.st_dev, opened.status.st_ino)
            accepted = self._by_identity.get(identity)
            if accepted is not None:
                self._path_snapshots[opened.key] = accepted
                self._paths[id(accepted)].append((opened.key, written_path))
                return accepted

            with os.fdopen(descriptor, "rb") as handle:
                descriptor = -1
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
        finally:
            if descriptor >= 0:
                os.close(descriptor)

        if self._entry_identity(opened_status) != self._entry_identity(opened.status):
            raise ResourceFailure(
                "resource_path_content_changed", written_path, phase="ingest"
            )

        snapshot = ResourceSnapshot(
            sha256=hashlib.sha256(content).hexdigest(), content=content
        )
        self._capture_reads += 1
        self._by_identity[identity] = snapshot
        self._path_snapshots[opened.key] = snapshot
        self._paths[id(snapshot)] = [(opened.key, written_path)]
        return snapshot

    def validate(self, written_path: str) -> None:
        """Validate a written path and file kind without reading its bytes."""
        try:
            opened = self._open_resource(written_path)
        except _PathChanged as error:
            raise ResourceFailure("resource_path_missing", written_path) from error
        os.close(opened.descriptor)

    def verify(self, snapshot: ResourceSnapshot) -> None:
        """Verify accepted path bytes before parsing the retained snapshot."""
        paths = self._paths.get(id(snapshot))
        if not paths:
            raise ValueError("snapshot was not captured by this project")
        for expected_key, written_path in paths:
            descriptor = -1
            try:
                opened = self._open_resource(written_path)
                descriptor = opened.descriptor
                if opened.key != expected_key:
                    raise _PathChanged
                with os.fdopen(descriptor, "rb") as handle:
                    descriptor = -1
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
            finally:
                if descriptor >= 0:
                    os.close(descriptor)
            digest = hashlib.sha256(content).hexdigest()
            if digest != snapshot.sha256:
                raise ResourceFailure(
                    "resource_path_content_changed",
                    written_path,
                    phase="ingest",
                )
