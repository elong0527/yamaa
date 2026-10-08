"""Run a domain specification against an explicitly selected project root.

This module is the runner REQ-0663 describes, and it lives outside the engine:
it names the project root, settles everything the project claims before the
engine reads any source, then hands the engine a dispatcher carrying the
activated bindings. The engine itself never imports this module.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from yamaa._reference_domain import DomainRun, _run_domain
from yamaa.application import PreparedWorkflow
from yamaa.functions.evaluator import function_dispatcher
from yamaa.functions.execution import activate_workflow_functions


def run_with_project_functions(
    entry_path: str | Path,
    *,
    project_root: str | Path,
    schema_root: str | Path | None = None,
    **domain_kwargs: Any,
) -> DomainRun:
    """Load, activate, and execute one domain specification against a project root.

    The root is keyword-only because REQ-0663 makes it the runner's choice: a
    specification can neither name it nor override what it says. Everything
    the project claims is settled before any source is read, and a failure
    at any of those stages raises carrying the diagnostics R018 names. A
    workflow that calls no project functions takes the ordinary engine
    path and ignores the root.
    """

    def activate(prepared: PreparedWorkflow):
        activated = activate_workflow_functions(
            prepared.workflow, project_root, prepared.schema_root
        )
        return None if activated is None else function_dispatcher(activated)

    return _run_domain(
        entry_path,
        schema_root=schema_root,
        dispatcher_factory=activate,
        raise_specification_errors=True,
        **domain_kwargs,
    )


__all__ = ["run_with_project_functions"]
