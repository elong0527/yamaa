"""Run a domain specification against an explicitly selected project root.

This module is the runner R018-2 describes, and it lives outside the engine:
it names the project root, settles everything the project claims before the
engine reads any source, then hands the engine a dispatcher carrying the
activated bindings. The engine itself never imports this module.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from yamaa.domain import DomainRun, yamaa_domain
from yamaa.functions.calls import function_calls
from yamaa.functions.evaluator import function_dispatcher
from yamaa.functions.execution import activate_project_functions
from yamaa.specification import load_specification


def _discover_schema_root(entry_path: Path) -> Path:
    """Find the schema bundle the way the engine's front door does."""
    candidates = [entry_path.parent, *entry_path.parent.parents, Path.cwd()]
    package_path = Path(__file__).resolve()
    candidates.extend(package_path.parents)
    seen: set[Path] = set()
    for candidate in candidates:
        for root in (candidate, candidate / "yaml"):
            resolved = root.resolve()
            if resolved in seen:
                continue
            seen.add(resolved)
            if (resolved / "schema.yaml").is_file():
                return resolved
    raise ValueError("cannot find the YAMAA schema bundle; pass schema_root explicitly")


def run_with_project_functions(
    entry_path: str | Path,
    *,
    project_root: str | Path,
    schema_root: str | Path | None = None,
    **domain_kwargs: Any,
) -> DomainRun:
    """Load, activate, and execute one domain specification against a project root.

    The root is keyword-only because R018-2 makes it the runner's choice: a
    specification can neither name it nor override what it says. Everything
    the project claims is settled before any source is read, and a failure
    at any of those stages raises carrying the diagnostics R018 names. A
    specification that calls no project functions takes the ordinary engine
    path and ignores the root.
    """
    entry = Path(entry_path)
    if not entry.is_file():
        raise FileNotFoundError(f"domain specification is not a file: {entry_path}")
    entry = entry.resolve()
    schema = (
        Path(schema_root).resolve()
        if schema_root is not None
        else _discover_schema_root(entry)
    )
    specification = load_specification(entry, schema).specification
    if not function_calls(specification):
        return yamaa_domain(entry, schema_root=schema, **domain_kwargs)
    activated = activate_project_functions(specification, project_root, schema)
    return yamaa_domain(
        entry,
        schema_root=schema,
        dispatcher=function_dispatcher(activated),
        **domain_kwargs,
    )


__all__ = ["run_with_project_functions"]
