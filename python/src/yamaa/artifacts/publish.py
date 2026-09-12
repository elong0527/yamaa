"""R020 publication: one atomic step from complete bytes to visible artifact.

R020 owns the bytes and their replacement but not where a run may write.
R002 resolves the paths a specification names for reading, and R021 reaches
only the files a run reads, so nothing admits a target on a specification's
word alone. `ArtifactTarget` is therefore the caller's explicit permission:
one destination file it names, in a directory that already exists.
"""

from __future__ import annotations

import os
import tempfile
from pathlib import Path

from yamaa.artifacts.csv import render_csv
from yamaa.artifacts.diagnostics import ArtifactDiagnostic, ArtifactError
from yamaa.artifacts.output import Artifact, ArtifactProfile, profile_of
from yamaa.artifacts.parquet import render_parquet


class ArtifactTarget:
    """One destination file the caller explicitly permits a run to write."""

    def __init__(self, path: str | Path) -> None:
        candidate = Path(path)
        if not candidate.is_absolute():
            raise ValueError("an artifact target is an absolute path the caller names")
        if not candidate.parent.is_dir():
            raise ValueError("an artifact target sits in an existing directory")
        if candidate.is_symlink():
            # R020-38 replaces the target itself, so a link would publish
            # somewhere the caller did not name.
            raise ValueError("an artifact target is not a symbolic link")
        if candidate.exists() and not candidate.is_file():
            raise ValueError("an artifact target is a regular file")
        profile = profile_of(candidate.name)
        if profile is None:
            raise ValueError(f"no R020 profile writes {candidate.name!r}")
        self.path = candidate
        self.profile: ArtifactProfile = profile

    def __repr__(self) -> str:
        return f"ArtifactTarget({str(self.path)!r})"


def render_artifact(artifact: Artifact) -> bytes:
    """Render one artifact to the complete bytes its profile fixes."""
    if artifact.profile == "csv":
        return render_csv(artifact)
    return render_parquet(artifact)


def publish_artifact(target: ArtifactTarget, artifact: Artifact) -> Path:
    """Replace the permitted target with one complete artifact, atomically.

    R020-41 publishes once, after the whole artifact is complete, so the
    bytes are rendered before the target is touched at all. R020-38 then
    writes them into a temporary regular file in the target's own
    directory, flushes it to the filesystem, and replaces the target with
    it, and R020-40 leaves the previous artifact in place and removes the
    temporary file when any of that fails.
    """
    if target.profile != artifact.profile:
        raise ValueError(
            f"a {artifact.profile} artifact cannot be published to {target.path.name!r}"
        )
    content = render_artifact(artifact)
    temporary: str | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="wb",
            dir=target.path.parent,
            prefix=f".{target.path.name}.",
            suffix=".part",
            delete=False,
        ) as handle:
            temporary = handle.name
            handle.write(content)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, target.path)
    except OSError as error:
        if temporary is not None:
            try:
                os.unlink(temporary)
            except OSError:
                pass
        raise ArtifactError(
            [
                ArtifactDiagnostic(
                    phase="output",
                    condition="publication_failed",
                    spec_paths=("output.path",),
                    requirement="R020-47",
                    context={"target": str(target.path), "reason": str(error)},
                )
            ]
        ) from error
    return target.path
