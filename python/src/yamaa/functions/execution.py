"""Execute one specification against one explicitly selected project root.

REQ-0663 gives this stage its shape: the root comes from the runner, not from
the specification, and everything the project claims is settled before a
source is read. Resolution, call validation, artifact verification, and the
activation vectors all run first; only then is the ordinary executor asked
to run, on a dispatcher that carries the activated bindings.
"""

from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path

from yamaa.functions.activation import (
    ACTIVATION_CACHE,
    ActivatedEnvironment,
    ActivationCache,
    activate,
)
from yamaa.functions.artifact import ArtifactResolver
from yamaa.functions.calls import function_calls, validate_calls
from yamaa.functions.environment import check_runner_language, load_environment
from yamaa.functions.errors import FunctionActivationError, FunctionFailure
from yamaa.functions.evaluator import function_dispatcher
from yamaa.planning import ExecutionDiagnostic, ProducerWorkflow
from yamaa.runtime import (
    ExecutionFailure,
    ExecutionHooks,
    ExecutionResult,
    SourceProvider,
    execute_with_source_provider,
)
from yamaa.specification.models import Specification


def _call_paths(specification: Specification) -> tuple[str, ...]:
    """Return the paths REQ-0704 anchors an environment-wide failure to."""
    return tuple(
        dict.fromkeys(
            f"{call.spec_path}.name" for call in function_calls(specification)
        )
    )


def activate_project_functions(
    specification: Specification,
    project_root: str | Path,
    schema_root: str | Path,
    *,
    resolver: ArtifactResolver | None = None,
    cache: ActivationCache | None = ACTIVATION_CACHE,
) -> ActivatedEnvironment:
    """Resolve, validate, and activate one project root for one specification.

    Raises `FunctionActivationError` carrying the committed diagnostics for
    every R018 failure between resolving the root and the last vector.
    """
    return _activate_specifications(
        (specification,), project_root, schema_root, resolver, cache
    )


def activate_workflow_functions(
    workflow: ProducerWorkflow,
    project_root: str | Path,
    schema_root: str | Path,
    *,
    resolver: ArtifactResolver | None = None,
    cache: ActivationCache | None = ACTIVATION_CACHE,
) -> ActivatedEnvironment | None:
    """Validate all node calls before importing code or reading study data."""
    specifications = tuple(node.resolved.specification for node in workflow.nodes)
    if not any(function_calls(specification) for specification in specifications):
        return None
    return _activate_specifications(
        specifications, project_root, schema_root, resolver, cache
    )


def _activate_specifications(
    specifications, project_root, schema_root, resolver, cache
):
    paths = tuple(
        dict.fromkeys(path for spec in specifications for path in _call_paths(spec))
    )
    try:
        environment = load_environment(Path(project_root), schema_root)
        check_runner_language(environment.environment)
        diagnostics: Sequence[ExecutionDiagnostic] = tuple(
            diagnostic
            for spec in specifications
            for diagnostic in validate_calls(spec, environment.environment)
        )
        if diagnostics:
            raise FunctionActivationError(diagnostics)
        return activate(environment, resolver, cache=cache)
    except FunctionFailure as failure:
        raise FunctionActivationError([failure.anchor(paths)]) from failure


def execute_with_project_functions(
    specification: Specification,
    source_provider: SourceProvider,
    project_root: str | Path,
    schema_root: str | Path,
    *,
    resolver: ArtifactResolver | None = None,
    cache: ActivationCache | None = ACTIVATION_CACHE,
    hooks: ExecutionHooks | None = None,
) -> ExecutionResult:
    """Activate a project root, then execute the specification it implements.

    A failure before activation completes is returned as an execution
    failure carrying the same diagnostics any other stage reports, and the
    source provider is never called: REQ-0691 keeps study data behind a
    passing activation.
    """
    try:
        activated = activate_project_functions(
            specification,
            project_root,
            schema_root,
            resolver=resolver,
            cache=cache,
        )
    except FunctionActivationError as error:
        return ExecutionFailure(diagnostics=error.diagnostics, handler_counts=())
    return execute_with_source_provider(
        specification,
        source_provider,
        dispatcher=function_dispatcher(activated),
        hooks=hooks,
    )


__all__ = [
    "activate_project_functions",
    "activate_workflow_functions",
    "execute_with_project_functions",
]
