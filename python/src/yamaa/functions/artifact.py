"""Resolve and verify the one immutable runtime a project pins.

R018-5 makes the artifact digest the identity of everything callable: a
binding is resolved inside the verified artifact and nowhere else, so a
global library, the process search path, the working directory, or an
ambient installation cannot answer for project code. R018-6 verifies that
digest before activation, which is why nothing here imports anything until
the bytes on disk hash to what the environment declared.
"""

from __future__ import annotations

import builtins
import hashlib
import importlib
import importlib.util
import sys
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from importlib.abc import MetaPathFinder
from importlib.machinery import ModuleSpec, PathFinder, SourceFileLoader
from pathlib import Path
from typing import Protocol

from yamaa.functions.errors import FunctionFailure
from yamaa.functions.models import ProjectRuntime

# A host bytecode cache is written beside the sources it was compiled from
# and is not project code, so it stays out of the content identity. Without
# this, importing an artifact once would change its digest for the next run.
_EXCLUDED_DIRECTORIES = frozenset({"__pycache__"})
_EXCLUDED_SUFFIXES = frozenset({".pyc", ".pyo"})


def _artifact_import(
    name: str,
    globals: Mapping[str, object] | None = None,
    locals: Mapping[str, object] | None = None,
    fromlist: tuple[str, ...] = (),
    level: int = 0,
):
    if level == 0:
        raise ImportError(
            f"absolute import {name!r} cannot resolve outside the verified artifact"
        )
    return builtins.__import__(name, globals, locals, fromlist, level)


class _ArtifactSourceLoader(SourceFileLoader):
    def exec_module(self, module) -> None:
        artifact_builtins = dict(vars(builtins))
        artifact_builtins["__import__"] = _artifact_import
        module.__dict__["__builtins__"] = artifact_builtins
        super().exec_module(module)


class _ArtifactFinder(MetaPathFinder):
    def find_spec(self, fullname, path=None, target=None):
        if not fullname.startswith("_yamaa_artifact_"):
            return None
        specification = PathFinder.find_spec(fullname, path, target)
        if specification is None or not isinstance(
            specification.loader, SourceFileLoader
        ):
            return specification
        specification.loader = _ArtifactSourceLoader(
            specification.loader.name,
            specification.loader.path,
        )
        return specification


_ARTIFACT_FINDER = _ArtifactFinder()


def _install_artifact_finder() -> None:
    if _ARTIFACT_FINDER not in sys.meta_path:
        sys.meta_path.insert(0, _ARTIFACT_FINDER)


class ArtifactUnavailable(ValueError):
    """A resolver cannot serve the artifact an environment names."""


class ArtifactResolver(Protocol):
    """Map one organization-resolvable reference to local artifact bytes."""

    def resolve(self, reference: str) -> Path: ...


@dataclass(frozen=True, slots=True)
class ProjectArtifactDirectory:
    """Serve the runtime a project root vendors beside its environment.

    An organization resolver answers a reference from wherever it publishes
    runtimes. A project root that ships its own runtime -- a test root, or a
    project reviewed as one directory -- answers every reference with that
    one directory, and the declared digest still decides whether the bytes
    found there are the artifact the environment pinned.
    """

    root: Path
    directory_name: str = "runtime"

    def resolve(self, reference: str) -> Path:
        candidate = self.root / self.directory_name
        if candidate.is_symlink() or not candidate.is_dir():
            raise ArtifactUnavailable(
                f"{reference!r} is not vendored at {self.directory_name}/"
            )
        return candidate


@dataclass(frozen=True, slots=True)
class MappedArtifacts:
    """Serve artifacts a runner resolved for this run, by reference."""

    directories: Mapping[str, Path]

    def resolve(self, reference: str) -> Path:
        directory = self.directories.get(reference)
        if directory is None:
            raise ArtifactUnavailable(f"{reference!r} is not available to this runner")
        if directory.is_symlink() or not directory.is_dir():
            raise ArtifactUnavailable(f"{reference!r} does not resolve to a directory")
        return directory


def artifact_files(root: Path) -> list[Path]:
    """Return the regular files whose bytes make up one artifact, in order."""
    files: list[Path] = []
    for candidate in root.rglob("*"):
        relative = candidate.relative_to(root)
        if _EXCLUDED_DIRECTORIES & set(relative.parts):
            continue
        if candidate.is_dir() and not candidate.is_symlink():
            continue
        if candidate.suffix in _EXCLUDED_SUFFIXES:
            continue
        if candidate.is_symlink() or not candidate.is_file():
            raise ArtifactUnavailable(
                f"{relative.as_posix()} is not a regular file of the artifact"
            )
        files.append(candidate)
    return sorted(files, key=lambda path: path.relative_to(root).as_posix())


def artifact_digest(root: Path) -> str:
    """Return the `sha256:` content identity of one artifact directory.

    The identity is a hash over a manifest of every file the artifact
    carries: its path relative to the artifact root, then the hash of its
    bytes. Renaming a file, reordering a directory, or changing one byte of
    one file therefore produces a different artifact, which is what R018-30
    needs in order to invalidate an activation.
    """
    manifest = hashlib.sha256()
    for path in artifact_files(root):
        relative = path.relative_to(root).as_posix()
        manifest.update(f"{relative}\n".encode())
        manifest.update(f"{hashlib.sha256(path.read_bytes()).hexdigest()}\n".encode())
    return f"sha256:{manifest.hexdigest()}"


@dataclass(frozen=True, slots=True)
class LoadedArtifact:
    """One verified artifact, and the only place a binding is resolved."""

    reference: str
    digest: str
    root: Path

    @property
    def namespace(self) -> str:
        """Return the private package name this artifact's modules live in.

        The name carries the digest, so code from two artifacts never shares
        a module identity and a re-pinned artifact is imported afresh rather
        than answered from the modules the previous one left behind.
        """
        return f"_yamaa_artifact_{self.digest.removeprefix('sha256:')}"

    def load(self, call: str) -> Callable[..., object]:
        """Return the callable `call` names inside this artifact alone."""
        module_path, _, attribute = call.rpartition(".")
        if not module_path or not attribute:
            raise FunctionFailure(
                "project_environment_invalid",
                "R018-34",
                {"reason": "a binding call must be module-qualified", "call": call},
            )
        module = self._import(module_path, call)
        target = getattr(module, attribute, None)
        if target is None or not callable(target):
            raise FunctionFailure(
                "project_environment_invalid",
                "R018-34",
                {
                    "reason": "the artifact declares no such callable",
                    "call": call,
                    "artifact": self.reference,
                },
            )
        return target

    def _import(self, module_path: str, call: str):
        package = self._package()
        qualified = f"{package}.{module_path}"
        try:
            return importlib.import_module(qualified)
        except ModuleNotFoundError as error:
            missing = error.name or ""
            if qualified == missing or qualified.startswith(f"{missing}."):
                raise FunctionFailure(
                    "project_environment_invalid",
                    "R018-34",
                    {
                        "reason": "the artifact contains no such module",
                        "call": call,
                        "artifact": self.reference,
                    },
                ) from error
            raise self._host_failure(call, error) from error
        except Exception as error:
            # Host code raised while being imported, which R018-40 owns.
            raise self._host_failure(call, error) from error

    def _host_failure(self, call: str, error: BaseException) -> FunctionFailure:
        return FunctionFailure(
            "function_call_failed",
            "R018-40",
            {
                "call": call,
                "artifact": self.reference,
                "host_error": type(error).__name__,
                "host_message": str(error),
            },
        )

    def _package(self):
        _install_artifact_finder()
        package = self.namespace
        existing = sys.modules.get(package)
        if existing is not None:
            return package
        specification = ModuleSpec(package, None, is_package=True)
        module = importlib.util.module_from_spec(specification)
        # The search path is exactly this artifact, so an import inside it
        # reaches the artifact's own modules and the process search path
        # answers for nothing R018-5 pins.
        module.__path__ = [str(self.root)]
        sys.modules[package] = module
        return package


def verify_artifact(
    runtime: ProjectRuntime,
    resolver: ArtifactResolver,
) -> LoadedArtifact:
    """Resolve one artifact and verify its digest before activation."""
    declared = runtime.artifact
    try:
        root = resolver.resolve(declared.reference)
        computed = artifact_digest(root)
    except (ArtifactUnavailable, OSError) as error:
        raise FunctionFailure(
            "runtime_artifact_mismatch",
            "R018-36",
            {
                "reason": "the pinned artifact could not be read",
                "artifact": declared.reference,
                "detail": str(error),
            },
        ) from error
    if computed != declared.digest:
        raise FunctionFailure(
            "runtime_artifact_mismatch",
            "R018-36",
            {
                "artifact": declared.reference,
                "declared": declared.digest,
                "computed": computed,
            },
        )
    return LoadedArtifact(
        reference=declared.reference,
        digest=declared.digest,
        root=root.resolve(),
    )


__all__ = [
    "ArtifactResolver",
    "ArtifactUnavailable",
    "LoadedArtifact",
    "MappedArtifacts",
    "ProjectArtifactDirectory",
    "artifact_digest",
    "artifact_files",
    "verify_artifact",
]
