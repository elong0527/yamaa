"""R020 publication: one atomic step from complete bytes to visible artifact."""

from __future__ import annotations

import os
import tempfile
from pathlib import Path

from yamaa.artifacts.csv import ArtifactError


def publish_artifact(target: str | Path, content: bytes) -> Path:
    """Write complete bytes to a temp file and atomically replace the target.

    The caller names the target outright; there is no default to derive.
    A failure leaves the previous artifact in place and removes the residue.
    """
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
        raise ArtifactError(
            "publication_failed", str(path), {"target": str(path)}
        ) from error
    finally:
        if temporary and os.path.exists(temporary):
            os.remove(temporary)
