"""Prepare one workflow without importing host functions or reading study data."""

from __future__ import annotations

from collections.abc import Iterable
from dataclasses import dataclass
from pathlib import Path

from yamaa.io import ProjectResources, approve_roots
from yamaa.planning import ProducerWorkflow, plan_workflow
from yamaa.specification.schema import load_schema_bundle


def discover_schema_root(entry_path: Path) -> Path:
    """Locate the schema bundle using the established Python search order."""
    candidates = [entry_path.parent, *entry_path.parent.parents, Path.cwd()]
    candidates.extend(Path(__file__).resolve().parents)
    seen: set[Path] = set()
    for candidate in candidates:
        for root in (candidate, candidate / "yaml"):
            resolved = root.resolve()
            if resolved in seen:
                continue
            seen.add(resolved)
            if (resolved / "schema.yaml").is_file():
                return resolved
    raise ValueError("cannot find the yamaa schema bundle; pass schema_root explicitly")


@dataclass(frozen=True, slots=True)
class PreparedWorkflow:
    """Resolved specifications and their approved resources for one run."""

    entry: Path
    schema_root: Path
    resources: ProjectResources
    workflow: ProducerWorkflow


def prepare_workflow(
    entry_path: str | Path,
    *,
    schema_root: str | Path | None = None,
    project_root: str | Path | None = None,
    data_roots: Iterable[str | Path] | None = None,
    read_project_configuration: bool = True,
) -> PreparedWorkflow:
    """Approve resources and resolve the producer graph exactly once."""
    entry = Path(entry_path)
    if not entry.is_file():
        raise FileNotFoundError(f"domain specification is not a file: {entry}")
    entry = entry.resolve()
    schema = (
        Path(schema_root).resolve()
        if schema_root is not None
        else discover_schema_root(entry)
    )
    approved = approve_roots(
        entry,
        project_root=project_root,
        data_roots=data_roots,
        read_project_configuration=read_project_configuration,
    )
    resources = ProjectResources(
        approved.project_root,
        base_directory=entry.parent,
        data_roots=approved.data_roots,
    )
    workflow = plan_workflow(entry, load_schema_bundle(schema), resources)
    return PreparedWorkflow(entry, schema, resources, workflow)
