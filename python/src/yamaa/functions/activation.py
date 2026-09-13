"""Run every activation vector before any specification may execute.

R018-30 is an ordering rule, not a reporting one: the vectors are what
establish that this project's code still means what its contract says, so
they run against the verified artifact first and a failure among them stops
the run before a single study row reaches a binding. Success is cached only
for the exact combination of identities R018-30 lists, and each of those is
read off the loaded environment rather than recomputed here.
"""

from __future__ import annotations

import hashlib
from collections.abc import Mapping
from dataclasses import dataclass, field

from pydantic import JsonValue

from yamaa.functions.artifact import (
    ArtifactResolver,
    LoadedArtifact,
    ProjectArtifactDirectory,
    verify_artifact,
)
from yamaa.functions.environment import check_runner_language
from yamaa.functions.errors import FunctionFailure
from yamaa.functions.invocation import (
    AuthoredValueError,
    BoundFunction,
    results_match,
    runtime_value,
)
from yamaa.functions.models import (
    ConformanceCase,
    ConformanceDocument,
    LoadedEnvironment,
)
from yamaa.models.values import ConditionResult, RuntimeValue, ValueResult


class ActivationCache:
    """The R018-30 record of which exact pinned identity already passed.

    The key covers the environment version, the artifact digest, every
    contract fingerprint, every implementation version, and the complete
    vector content. Changing any of them is a different key, so a re-pinned
    artifact or an edited vector is activated again rather than trusted.
    """

    def __init__(self) -> None:
        self._passed: set[str] = set()

    @staticmethod
    def key(loaded: LoadedEnvironment) -> str:
        environment = loaded.environment
        digest = hashlib.sha256()
        digest.update(f"{environment.version}\n".encode())
        digest.update(f"{environment.runtime.artifact.digest}\n".encode())
        for name, contract in sorted(environment.functions.items()):
            digest.update(f"{name}\n".encode())
            digest.update(f"{loaded.fingerprints[name]}\n".encode())
            digest.update(f"{contract.implementation_version}\n".encode())
        digest.update(f"{loaded.vector_identity}\n".encode())
        return f"sha256:{digest.hexdigest()}"

    def passed(self, key: str) -> bool:
        return key in self._passed

    def record(self, key: str) -> None:
        self._passed.add(key)

    def clear(self) -> None:
        self._passed.clear()


# One process-wide record, so a runner executing several specifications
# against one unchanged project activates it once.
ACTIVATION_CACHE = ActivationCache()


@dataclass(frozen=True, slots=True)
class ActivatedEnvironment:
    """One project whose vectors have passed against its verified artifact."""

    environment: LoadedEnvironment
    artifact: LoadedArtifact
    functions: Mapping[str, BoundFunction]
    # Whether this activation ran the vectors or was answered by the cache
    # R018-30 permits. A run never depends on it; it is how a caller sees
    # that a changed identity really did activate again.
    vectors_executed: bool = field(default=True)

    def bound(self, name: str) -> BoundFunction | None:
        return self.functions.get(name)


def _conformance_failure(
    name: str,
    case: ConformanceCase,
    reason: str,
    **context: JsonValue,
) -> FunctionFailure:
    return FunctionFailure(
        "function_conformance_failed",
        "R018-42",
        {"function": name, "case": case.id, "reason": reason, **context},
    )


def _run_case(bound: BoundFunction, case: ConformanceCase) -> None:
    """Run one vector and fail the activation unless it reproduces its result."""
    try:
        supplied: dict[str, RuntimeValue] = {
            argument: runtime_value(value) for argument, value in case.args.items()
        }
        expected = runtime_value(case.result)
    except AuthoredValueError as error:
        raise FunctionFailure(
            "project_environment_invalid",
            "R018-34",
            {
                "reason": "a vector case carries a value of no scalar type",
                "function": bound.name,
                "case": case.id,
                "detail": str(error),
            },
        ) from error

    produced = bound.invoke(supplied)
    if isinstance(produced, ConditionResult):
        condition = produced.condition
        raise _conformance_failure(
            bound.name,
            case,
            "the case failed before it produced a result",
            condition=condition.condition,
            requirement=condition.requirement,
            context=dict(condition.context),
        )
    assert isinstance(produced, ValueResult)
    if not results_match(produced.value, expected, bound.contract.comparison_decimals):
        raise _conformance_failure(
            bound.name,
            case,
            "the case produced a different result",
            contract_version=bound.contract.contract_version,
            implementation_version=bound.contract.implementation_version,
            comparison_decimals=bound.contract.comparison_decimals,
        )


def _run_vectors(
    functions: Mapping[str, BoundFunction],
    documents: Mapping[str, ConformanceDocument],
) -> None:
    for name in sorted(functions):
        for case in documents[name].cases:
            _run_case(functions[name], case)


def activate(
    loaded: LoadedEnvironment,
    resolver: ArtifactResolver | None = None,
    *,
    cache: ActivationCache | None = ACTIVATION_CACHE,
) -> ActivatedEnvironment:
    """Verify this runner may run the project, then activate its bindings.

    The language and the artifact are checked before any code is loaded
    (R018-6), every binding is resolved inside that artifact (R018-5), and
    the vectors run last (R018-30) -- after which the environment is ready
    for a specification and not before.
    """
    environment = loaded.environment
    check_runner_language(environment)
    selected = ProjectArtifactDirectory(loaded.root) if resolver is None else resolver
    artifact = verify_artifact(environment.runtime, selected)
    functions = {
        name: BoundFunction(
            name=name,
            contract=contract,
            target=artifact.load(contract.binding.call),
        )
        for name, contract in environment.functions.items()
    }

    key = ActivationCache.key(loaded)
    cached = cache is not None and cache.passed(key)
    if not cached:
        _run_vectors(functions, loaded.conformance)
        if cache is not None:
            cache.record(key)
    return ActivatedEnvironment(
        environment=loaded,
        artifact=artifact,
        functions=functions,
        vectors_executed=not cached,
    )


__all__ = [
    "ACTIVATION_CACHE",
    "ActivatedEnvironment",
    "ActivationCache",
    "activate",
]
