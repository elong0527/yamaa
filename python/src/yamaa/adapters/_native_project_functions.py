"""Native invocation qualification over shared verified artifact bindings.

The host still owns environment loading, artifact resolution and conformance
comparison. Native vectors use the Rust signature/argument/result lifecycle;
reference activation success never supplies a native cache hit.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path

from yamaa.adapters._native_dataset_plan import literal
from yamaa.adapters._native_dataset_report import scalar
from yamaa.adapters.native_functions import adapt_callback, invoke_function
from yamaa.functions.activation import (
    ActivationCache,
    _run_vectors,
    resolve_bindings,
)
from yamaa.functions.calls import function_calls, validate_calls
from yamaa.functions.environment import check_runner_language, load_environment
from yamaa.functions.errors import FunctionActivationError, FunctionFailure
from yamaa.functions.invocation import BoundFunction, runtime_value
from yamaa.functions.models import binding_arguments
from yamaa.models.values import (
    MISSING,
    ConditionResult,
    DateTimeValue,
    DateValue,
    RuntimeCondition,
    ValueResult,
)


class NativeActivationCache:
    """Keep native vector success separate from reference activation, without digests."""

    def __init__(self):
        """Create a process-local cache of canonical native activation identities."""
        self._passed: set[str] = set()

    def clear(self):
        """Force vectors to execute on the next native project run."""
        self._passed.clear()


NATIVE_ACTIVATION_CACHE = NativeActivationCache()


def encode_value(value):
    """Encode an admitted runtime scalar, retaining exact temporal precision and bits."""
    if value is MISSING:
        return {"missing": None}
    if isinstance(value, (DateValue, DateTimeValue)):
        kind = "date" if isinstance(value, DateValue) else "datetime"
        return {kind: {"text": value.to_text(), "precision": value.collected_precision}}
    return literal(value)


def authored(value):
    """Decode only the authored R018 scalar vocabulary before transport encoding."""
    return encode_value(runtime_value(value))


def signature(bound):
    """Copy a resolved contract into the shared native closed signature vocabulary."""
    contract = bound.contract
    mapping = binding_arguments(contract)
    return {
        "identity": {
            "name": bound.name,
            "contract_version": contract.contract_version,
            "implementation_version": contract.implementation_version,
            "call": contract.binding.call,
        },
        "parameters": [
            {
                "name": parameter.name,
                "host_name": mapping[parameter.name],
                "type": parameter.type,
                "accepts_missing": parameter.accepts_missing,
                "presence": {"required": None}
                if parameter.required
                else {"optional": authored(parameter.default)},
            }
            for parameter in contract.params
        ],
        "returns": contract.returns,
        "may_return_missing": contract.may_return_missing,
    }


def decode_value(value):
    """Materialize native activation observations without converting result types."""
    if "missing" in value:
        return MISSING
    for kind, model in (("date", DateValue), ("datetime", DateTimeValue)):
        if kind in value:
            data = value[kind]
            return model.parse(data["text"]).model_copy(
                update={"collected_precision": data["precision"]}
            )
    return scalar(value)


class NativeBoundFunction(BoundFunction):
    """Use native invocation for every activation vector and no reference evaluator."""

    def invoke(self, supplied):
        """Invoke the verified target through Rust, preserving the public vector result."""
        request = {
            "protocol": "function/1",
            **signature(self),
            "arguments": [
                {"name": name, "value": encode_value(value)}
                for name, value in supplied.items()
            ],
        }
        encoded = invoke_function(json.dumps(request, ensure_ascii=True), self.target)
        result = json.loads(encoded)
        if result["protocol"] != "function/1":
            raise ValueError("unsupported native activation response protocol")
        outcome = result["outcome"]
        if outcome["status"] == "value":
            return ValueResult(value=decode_value(outcome["value"]))
        if outcome["status"] != "condition":
            raise ValueError("unknown native activation outcome")
        return ConditionResult(condition=RuntimeCondition(**outcome["diagnostic"]))


@dataclass(frozen=True)
class Bindings:
    """One run's captured callback authority and immutable serialized signatures."""

    names: tuple[str, ...]
    declarations: str
    callbacks: tuple
    vectors_executed: bool


def activate_project(specification, project_root, schema_root, resolver, cache):
    """Validate calls, resolve the pinned artifact and pass native vectors before IO."""
    if cache is not None and not isinstance(cache, NativeActivationCache):
        raise TypeError("native activation requires NativeActivationCache or None")
    paths = tuple(
        dict.fromkeys(
            f"{call.spec_path}.name" for call in function_calls(specification)
        )
    )
    try:
        loaded = load_environment(Path(project_root), schema_root).model_copy(deep=True)
        check_runner_language(loaded.environment)
        diagnostics = validate_calls(specification, loaded.environment)
        if diagnostics:
            raise FunctionActivationError(diagnostics)
        _, resolved = resolve_bindings(loaded, resolver)
        functions = {
            name: NativeBoundFunction(bound.name, bound.contract, bound.target)
            for name, bound in resolved.items()
        }
        # The type-separated cache cannot reuse reference backend activation success.
        key = ActivationCache.key(loaded)
        cached = cache is not None and key in cache._passed
        if not cached:
            _run_vectors(functions, loaded.conformance)
        used = tuple(dict.fromkeys(call.name for call in function_calls(specification)))
        bindings = Bindings(
            names=used,
            declarations=json.dumps([signature(functions[name]) for name in used]),
            callbacks=tuple(adapt_callback(functions[name].target) for name in used),
            vectors_executed=not cached,
        )
        if not cached and cache is not None:
            cache._passed.add(key)
        return bindings
    except FunctionFailure as failure:
        raise FunctionActivationError([failure.anchor(paths)]) from failure
