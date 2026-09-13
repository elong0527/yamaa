"""Resolve project files and retain immutable byte snapshots under R021."""

from __future__ import annotations

import hashlib
import os
import re
import stat
import weakref
from collections.abc import Iterable
from dataclasses import dataclass
from pathlib import Path, PurePath
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field

from yamaa.specification._yaml import read_yaml_document
from yamaa.specification.diagnostics import SpecificationError

_URI_SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")
_DRIVE_ROOT = re.compile(r"^[A-Za-z]:/")

_RESOURCE_REQUIREMENTS = {
    "resource_path_uri_scheme": "R021-9",
    "resource_path_not_relative": "R021-15",
    "resource_path_symlink": "R021-17",
    "resource_path_outside_project": "R021-18",
    "resource_path_missing": "R021-19",
    "resource_path_not_regular_file": "R021-19",
}

PROJECT_CONFIGURATION_NAME = "yamaa-project.yaml"


class _PathChanged(OSError):
    """A resource component changed during a descriptor-anchored walk."""


@dataclass(frozen=True)
class _ApprovedRoot:
    """One directory a runner approved before any specification was read."""

    descriptor: int
    canonical: tuple[str, ...]
    spellings: tuple[tuple[str, ...], ...]


@dataclass(frozen=True)
class _Anchor:
    """The approved root a written path resolves from, and what follows it."""

    root: _ApprovedRoot
    key: tuple[str, ...]
    rooted_segments: tuple[str, ...] | None


@dataclass(frozen=True)
class _OpenedResource:
    """A regular file opened beneath an anchored approved root."""

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
        self.requirement = _RESOURCE_REQUIREMENTS.get(condition)
        super().__init__(f"{condition}: {written_path!r}")


def rooted_project_segments(written_path: str) -> tuple[str, ...] | None:
    """Split a rooted written path into its marker and segments, or None.

    R021-7 spells a rooted path with a leading separator or with one ASCII
    letter and ``:/``. The marker leads the returned segments so that a path
    rooted one way never matches a root spelled the other way.
    """
    if written_path.startswith("/"):
        marker, head = "/", ""
    else:
        match = _DRIVE_ROOT.match(written_path)
        if match is None:
            return None
        marker, head = match.group(0), match.group(0)[:-1]
    remainder = written_path[len(marker) :]
    return (head, *(remainder.split("/") if remainder else ()))


def classify_project_path(written_path: str) -> str | None:
    """Return the first written-form condition from R021, if any.

    R021-25 fixes the order: a scheme (R021-9), then a backslash (R021-10),
    then an empty segment (R021-11), then a dot segment in a rooted path
    (R021-12). Nothing here consults the filesystem.
    """
    segments = rooted_project_segments(written_path)
    if segments is None and _URI_SCHEME.match(written_path):
        return "resource_path_uri_scheme"
    if "\\" in written_path:
        return "resource_path_not_normalized"
    written = written_path.split("/") if segments is None else list(segments[1:])
    if not written or any(segment == "" for segment in written):
        return "resource_path_not_normalized"
    if segments is not None and any(segment in (".", "..") for segment in written):
        return "resource_path_not_normalized"
    return None


def _directory_spelling(candidate: str | Path) -> tuple[str, ...] | None:
    """Return the rooted segments a runner used to name a directory."""
    return rooted_project_segments(PurePath(candidate).as_posix())


class ProjectConfigurationError(ValueError):
    """A project configuration a run cannot be started from (R021-29)."""


@dataclass(frozen=True)
class ApprovedRoots:
    """Every root one run may read from, fixed before any specification."""

    project_root: Path
    data_roots: tuple[Path, ...]
    configuration: Path | None


def _existing_directory(candidate: str | Path, label: str) -> Path:
    try:
        resolved = Path(candidate).resolve(strict=True)
    except OSError as error:
        raise ValueError(f"{label} must exist") from error
    if not resolved.is_dir():
        raise ValueError(f"{label} must be a directory")
    return resolved


def find_project_configuration(entry_file: str | Path) -> Path | None:
    """Walk up from an entry file to its project configuration (R021-2).

    The file marks the project root by sitting at it, so the first one found
    on the way up names the root. A run that finds none is a run whose study
    declared nothing, not a failure.
    """
    start = Path(entry_file).resolve()
    directory = start if start.is_dir() else start.parent
    for candidate in (directory, *directory.parents):
        configuration = candidate / PROJECT_CONFIGURATION_NAME
        if configuration.is_file():
            return configuration
    return None


def _declared_data_roots(configuration: Path, project_root: Path) -> tuple[Path, ...]:
    """Read the data roots one project configuration declares."""
    try:
        document = read_yaml_document(configuration)
    except (SpecificationError, OSError) as error:
        raise ProjectConfigurationError(
            "project configuration cannot be read"
        ) from error
    if not isinstance(document, dict):
        raise ProjectConfigurationError("project configuration must be a mapping")
    unknown = sorted(set(document) - {"version", "data_roots"})
    if unknown:
        raise ProjectConfigurationError(
            f"project configuration has unknown fields: {unknown}"
        )
    if document.get("version") != "1.0":
        raise ProjectConfigurationError("project configuration version must be '1.0'")

    declared = document.get("data_roots")
    if declared is None:
        return ()
    if not isinstance(declared, list) or not all(
        isinstance(item, str) and item for item in declared
    ):
        raise ProjectConfigurationError(
            "project configuration data_roots must be non-empty paths"
        )

    roots: list[Path] = []
    for item in declared:
        candidate = Path(item)
        if not candidate.is_absolute():
            # A relative entry names a directory beside the study, so it is
            # read from the project root the configuration itself marks.
            candidate = project_root / candidate
        try:
            _existing_directory(candidate, "approved data root")
        except ValueError as error:
            raise ProjectConfigurationError(
                "project configuration declares a data root that is not an "
                "existing directory"
            ) from error
        # The spelling the study wrote is kept, not its canonical form: a
        # rooted path repeats that spelling, and R021-15 matches it there.
        roots.append(candidate)
    return tuple(roots)


def approve_roots(
    entry_file: str | Path,
    *,
    project_root: str | Path | None = None,
    data_roots: Iterable[str | Path] | None = None,
    read_project_configuration: bool = True,
) -> ApprovedRoots:
    """Select every root one run may read from, before any specification.

    The project root is where the configuration sits unless the runner names
    one outright (R021-1). Data roots come from that configuration and from
    the runner (R021-3). A runner that names data roots makes them the
    ceiling every declared root must resolve inside, and a packaging run
    declines the configuration's roots entirely (R021-5).
    """
    if project_root is not None:
        # A runner that names the root takes the configuration sitting at it,
        # never one further up that names a wider project.
        root = _existing_directory(project_root, "approved project root")
        named = root / PROJECT_CONFIGURATION_NAME
        configuration = named if named.is_file() else None
    else:
        configuration = find_project_configuration(entry_file)
        if configuration is not None:
            selected: Path = configuration.parent
        else:
            entry = Path(entry_file).resolve()
            selected = entry if entry.is_dir() else entry.parent
        root = _existing_directory(selected, "approved project root")

    ceiling: tuple[Path, ...] | None = None
    if data_roots is not None:
        ceiling = tuple(
            _existing_directory(candidate, "approved data root")
            for candidate in data_roots
        )

    declared: tuple[Path, ...] = ()
    if configuration is not None and read_project_configuration:
        declared = _declared_data_roots(configuration, root)

    if declared and ceiling is not None:
        for written in declared:
            candidate = written.resolve()
            if not any(
                candidate == limit or limit in candidate.parents for limit in ceiling
            ):
                raise ProjectConfigurationError(
                    "project configuration declares a data root outside the "
                    "roots this run allows"
                )

    return ApprovedRoots(
        project_root=root,
        data_roots=declared if declared else (ceiling or ()),
        configuration=configuration,
    )


class ProjectResources:
    """Capture and verify files under the roots one runner approved."""

    def __init__(
        self,
        project_root: str | Path,
        *,
        base_directory: str | Path | None = None,
        data_roots: Iterable[str | Path] = (),
    ) -> None:
        root = _existing_directory(project_root, "approved project root")

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

        approved: list[_ApprovedRoot] = []
        try:
            approved.append(self._approve(project_root, root))
            for candidate in data_roots:
                resolved = _existing_directory(candidate, "approved data root")
                approved.append(self._approve(candidate, resolved))
        except BaseException:
            for opened in approved:
                os.close(opened.descriptor)
            raise

        descriptors = [opened.descriptor for opened in approved]
        self._roots = tuple(approved)
        self._root_finalizer = weakref.finalize(self, self._close_all, descriptors)
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
    def _close_all(descriptors: list[int]) -> None:
        for descriptor in descriptors:
            os.close(descriptor)

    def _approve(self, written: str | Path, resolved: Path) -> _ApprovedRoot:
        """Open one approved root and record the spellings that name it.

        R021-3 canonicalizes and opens a root when it is selected, so
        R021-17 can exempt the anchor: nothing above this descriptor can be
        swapped between validation and ingestion.
        """
        descriptor = -1
        try:
            initial_status = resolved.stat(follow_symlinks=False)
            descriptor = os.open(resolved, self._directory_flags())
            opened_status = os.fstat(descriptor)
        except OSError as error:
            if descriptor >= 0:
                os.close(descriptor)
            raise ValueError("approved root must be a directory") from error
        if not stat.S_ISDIR(opened_status.st_mode):
            os.close(descriptor)
            raise ValueError("approved root must be a directory")
        if self._entry_identity(initial_status) != self._entry_identity(opened_status):
            os.close(descriptor)
            raise ValueError("approved root changed during selection")

        canonical = _directory_spelling(resolved)
        if canonical is None:  # A resolved path is always rooted.
            os.close(descriptor)
            raise ValueError("approved root must be a rooted local directory")
        spellings = {canonical}
        given = _directory_spelling(written)
        if given is not None:
            spellings.add(given)
        return _ApprovedRoot(
            descriptor=descriptor,
            canonical=canonical,
            spellings=tuple(sorted(spellings)),
        )

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

    def _anchor(self, written_path: str) -> _Anchor:
        """Choose the approved root a written path resolves from (R021-15)."""
        condition = classify_project_path(written_path)
        if condition is not None:
            raise ResourceFailure(condition, written_path)

        segments = rooted_project_segments(written_path)
        if segments is not None:
            matched: _ApprovedRoot | None = None
            depth = -1
            for root in self._roots:
                for spelling in root.spellings:
                    size = len(spelling)
                    if size > depth and segments[:size] == spelling:
                        matched, depth = root, size
            if matched is None:
                raise ResourceFailure("resource_path_not_relative", written_path)
            remainder = segments[depth:]
            if not remainder:
                raise ResourceFailure("resource_path_not_regular_file", written_path)
            return _Anchor(matched, (*matched.canonical, *remainder), remainder)

        if self._base_components is None:
            raise ResourceFailure("resource_path_outside_project", written_path)

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
        project = self._roots[0]
        return _Anchor(project, (*project.canonical, *components), None)

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
        anchor = self._anchor(written_path)
        if anchor.rooted_segments is None:
            if self._base_components is None:  # Guarded by _anchor.
                raise AssertionError("unreachable project base")
            walk = (*self._base_components, *written_path.split("/"))
        else:
            walk = anchor.rooted_segments

        directories = [os.dup(anchor.root.descriptor)]
        components: list[str] = []
        links: list[tuple[int, str, tuple[int, int, int]]] = []
        file_descriptor: int | None = None
        try:
            for segment in walk[:-1]:
                if segment == ".":
                    continue
                if segment == "..":
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

            final = walk[-1]
            if final == "..":
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
            if (*anchor.root.canonical, *components, final) != anchor.key:
                raise AssertionError(
                    "descriptor walk disagrees with path normalization"
                )

            opened = _OpenedResource(file_descriptor, anchor.key, file_status)
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
