"""Activate and execute the project functions R018 defines."""

from yamaa.functions.activation import (
    ACTIVATION_CACHE,
    ActivatedEnvironment,
    ActivationCache,
    activate,
)
from yamaa.functions.artifact import (
    ArtifactResolver,
    ArtifactUnavailable,
    LoadedArtifact,
    MappedArtifacts,
    ProjectArtifactDirectory,
    artifact_digest,
    artifact_files,
    verify_artifact,
)
from yamaa.functions.calls import FunctionCall, function_calls, validate_calls
from yamaa.functions.environment import (
    ENVIRONMENT_NAME,
    RUNNER_LANGUAGE,
    environment_schema,
    load_environment,
    select_project_root,
)
from yamaa.functions.errors import FunctionActivationError, FunctionFailure
from yamaa.functions.evaluator import (
    FUNCTION_OPERATION,
    function_dispatcher,
    function_handlers,
)
from yamaa.functions.execution import (
    activate_project_functions,
    execute_with_project_functions,
)
from yamaa.functions.fingerprint import contract_fingerprint
from yamaa.functions.invocation import BoundFunction, results_match
from yamaa.functions.models import (
    ConformanceCase,
    ConformanceDocument,
    FunctionBinding,
    FunctionContract,
    FunctionParameter,
    LoadedEnvironment,
    ProjectEnvironment,
    ProjectRuntime,
    RuntimeArtifact,
)
from yamaa.functions.runner import run_with_project_functions

__all__ = [
    "ACTIVATION_CACHE",
    "ENVIRONMENT_NAME",
    "FUNCTION_OPERATION",
    "RUNNER_LANGUAGE",
    "ActivatedEnvironment",
    "ActivationCache",
    "ArtifactResolver",
    "ArtifactUnavailable",
    "BoundFunction",
    "ConformanceCase",
    "ConformanceDocument",
    "FunctionActivationError",
    "FunctionBinding",
    "FunctionCall",
    "FunctionContract",
    "FunctionFailure",
    "FunctionParameter",
    "LoadedArtifact",
    "LoadedEnvironment",
    "MappedArtifacts",
    "ProjectArtifactDirectory",
    "ProjectEnvironment",
    "ProjectRuntime",
    "RuntimeArtifact",
    "activate",
    "activate_project_functions",
    "artifact_digest",
    "artifact_files",
    "contract_fingerprint",
    "environment_schema",
    "execute_with_project_functions",
    "function_calls",
    "function_dispatcher",
    "function_handlers",
    "load_environment",
    "results_match",
    "run_with_project_functions",
    "select_project_root",
    "validate_calls",
    "verify_artifact",
]
