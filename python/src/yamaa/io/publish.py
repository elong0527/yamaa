"""R020 publication: one atomic step from complete bytes to visible artifact."""

from __future__ import annotations

import os
import tempfile
from pathlib import Path


class ArtifactError(ValueError):
    """One artifact declaration or publication failure with its identity."""

    def __init__(
        self,
        condition: str,
        value: object = None,
        keys: dict[str, object] | None = None,
    ) -> None:
        super().__init__(condition)
        self.condition = condition
        self.value = value
        self.keys = keys or {}


def publish_artifact(target: str | Path, content: bytes) -> Path:
    """Atomically replace the explicit target with complete bytes."""
    path = Path(target)
    parent = path.parent
    temporary = ""
    try:
        with tempfile.NamedTemporaryFile(
            mode="wb", dir=parent, prefix=f"{path.name}.", delete=False
        ) as handle:
            temporary = handle.name
            handle.write(content)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        return path
    except OSError as error:
        if temporary:
            try:
                os.unlink(temporary)
            except OSError:
                pass
        raise ArtifactError("publication_failed", str(path)) from error
