"""Independent function-call truth shared with the no_std application service."""

from __future__ import annotations

import csv
import datetime as dt
import struct
from pathlib import Path

import pytest

from yamaa.functions.invocation import BoundFunction, runtime_value
from yamaa.functions.models import FunctionBinding, FunctionContract, FunctionParameter
from yamaa.models.values import MISSING, ConditionResult, DateTimeValue, DateValue

FIXTURE = (
    Path(__file__).resolve().parents[3]
    / "rust/crates/yamaa-engine/tests/fixtures/function_invocation.tsv"
)
with FIXTURE.open(encoding="utf-8", newline="") as stream:
    CASES = list(csv.DictReader(stream, delimiter="\t"))


def authored(token: str):
    """Decode authored scalar inputs without consulting the expected result."""
    if token == "missing":
        return None
    kind, text = token.split(":", 1)
    if kind == "int":
        return int(text)
    if kind == "float":
        return float(text)
    if kind == "bool":
        return text == "true"
    if kind == "str":
        return text
    if kind in ("date", "datetime"):
        return {kind: text}
    raise AssertionError(kind)


def encode(value: object) -> str:
    """Observe exact types and binary64 bits on both sides of the host call."""
    if value is MISSING or value is None:
        return "missing"
    if type(value) is bool:
        return f"bool:{str(value).lower()}"
    if type(value) is int:
        return f"int:{value}"
    if type(value) is float:
        return f"float:{struct.pack('>d', value).hex()}"
    if isinstance(value, str):
        return f"str:{value}"
    if isinstance(value, (DateValue, DateTimeValue)):
        kind = "date" if isinstance(value, DateValue) else "datetime"
        return f"{kind}:{value.to_text()}"
    if type(value) is dt.datetime:
        return f"datetime:{value.isoformat(timespec='seconds')}"
    if type(value) is dt.date:
        return f"date:{value.isoformat()}"
    raise AssertionError(type(value))


def parameters(text: str) -> tuple[list[FunctionParameter], dict[str, str]]:
    """Preserve declaration order and explicit missing versus absent defaults."""
    result = []
    mapping = {}
    if text != "-":
        for parameter in text.split(";"):
            name, kind, missing, default, host = parameter.split("/")
            values = {
                "name": name,
                "type": kind,
                "accepts_missing": missing == "true",
                "required": default == "required",
            }
            if default != "required":
                values["default"] = authored(default)
            result.append(FunctionParameter.model_validate(values))
            mapping[name] = host
    return result, mapping


def failure(result: ConditionResult) -> str:
    """Check complete identity and normative failure metadata before summarizing."""
    error = result.condition
    context = error.context
    assert error.phase == "derivation"
    assert error.applicable_handler is None
    assert context["function"] == "example"
    assert context["contract_version"] == "1"
    assert context["implementation_version"] == "2"
    if error.condition == "invalid_function_argument":
        assert error.requirement == "REQ-0700"
        if "unknown" in context:
            return "error:unknown:" + ",".join(context["unknown"])
        if "expected" in context:
            return (
                f"error:argument:{context['parameter']}:"
                f"{context['expected']}:{context['actual']}"
            )
        assert context["reason"] == "a required argument was not supplied"
        return f"error:missing:{context['parameter']}"
    if error.condition == "function_call_failed":
        assert error.requirement == "REQ-0701"
        assert context["call"] == "artifact.example"
        return f"error:raised:{context['host_error']}:{context['host_message']}"
    assert error.condition == "invalid_function_result"
    assert error.requirement == "REQ-0702"
    if "expected" in context:
        return f"error:result:{context['expected']}:{context['actual']}"
    reason = context["reason"]
    if reason == "a binding returned a Boolean":
        assert context["returned"] == "bool"
        return "error:boolean"
    if reason == "an invoked binding returned an undeclared missing":
        return "error:undeclared-missing"
    if reason == "a returned integer exceeds 64 bits":
        return "error:invalid:bigint"
    assert reason == "a binding returned a value of no scalar type"
    return f"error:invalid:{context['returned']}"


@pytest.mark.parametrize("case", CASES, ids=lambda case: case["id"])
def test_shared_function_invocation(case: dict[str, str]) -> None:
    """The actual reference runner must match values, failures and callback traces."""
    trace = []
    calls = 0
    action = case["callback"]

    def target(**arguments):
        """Record a host side effect and execute the fixture's declared action."""
        nonlocal calls
        calls += 1
        trace.extend(f"{name}={encode(value)}" for name, value in arguments.items())
        if not arguments:
            trace.append("called")
        if action == "raise":
            raise ValueError("boom")
        if action == "invalid:list":
            return [1]
        if action == "invalid:bigint":
            return 2**63
        if action.startswith("echo:"):
            return arguments[action.removeprefix("echo:")]
        value = authored(action)
        if isinstance(value, dict):
            return runtime_value(value)
        return value

    params, mapping = parameters(case["parameters"])
    contract = FunctionContract(
        contract_version="1",
        implementation_version="2",
        description="Shared independently specified invocation truth",
        params=params,
        returns=case["returns"],
        may_return_missing=case["may_missing"] == "true",
        binding=FunctionBinding(call="artifact.example", args=mapping),
        conformance="unused.yaml",
    )
    supplied = {}
    if case["arguments"] != "-":
        for item in case["arguments"].split(";"):
            name, token = item.split("=", 1)
            supplied[name] = runtime_value(authored(token))
    result = BoundFunction("example", contract, target).invoke(supplied)
    actual = (
        failure(result) if isinstance(result, ConditionResult) else encode(result.value)
    )
    assert actual == case["expected"]
    assert calls == int(case["trace"] != "-")
    assert (";".join(trace) if trace else "-") == case["trace"]
