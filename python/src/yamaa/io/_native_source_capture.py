"""Descriptor-backed source IO for the shared original-document compiler."""

from __future__ import annotations

from typing import Literal

from yamaa.io.project import ProjectResources, ResourceFailure


class SourceCapture:
    """Retain bounded snapshots under the caller's existing approved roots."""

    def __init__(self, resources: ProjectResources) -> None:
        self.resources = resources

    def __call__(
        self, name: str, path: str, maximum: int
    ) -> (
        tuple[bytes, bool]
        | tuple[Literal["missing", "not_regular_file"], ResourceFailure]
    ):
        before = self.resources.capture_reads
        try:
            snapshot = self.resources.capture(path, maximum=maximum)
            self.resources.verify(snapshot)
        except ResourceFailure as error:
            if error.phase == "validation":
                if error.condition == "resource_path_missing":
                    return "missing", error
                if error.condition == "resource_path_not_regular_file":
                    return "not_regular_file", error
            raise
        created = self.resources.capture_reads - before
        if created not in (0, 1):
            raise ValueError("invalid resource snapshot accounting")
        return snapshot.content, bool(created)
