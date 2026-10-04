"""Independent reference observations for the typed dataset callback composition."""

from types import SimpleNamespace

import pytest

from yamaa.expressions.core import FailedResolution, ResolvedValue
from yamaa.functions.evaluator import function_handlers
from yamaa.functions.invocation import BoundFunction
from yamaa.functions.models import FunctionBinding, FunctionContract, FunctionParameter
from yamaa.models.values import MISSING, ConditionResult, RuntimeCondition, ValueResult


@pytest.mark.parametrize("scenario", ["values", "missing", "missing_then_error"])
def test_reference_resolves_authored_arguments_before_signature_checks(scenario):
    """A missing argument cannot skip a later read, while host names use signature order."""
    reads = []
    calls = []

    def target(**arguments):
        """Observe exact mapped order and the applied default without deriving expected truth."""
        calls.append(list(arguments.items()))
        return 11

    contract = FunctionContract(
        contract_version="1",
        implementation_version="2",
        description="Independent dataset callback ordering",
        params=[
            FunctionParameter(name="first", type="int"),
            FunctionParameter(name="second", type="int"),
            FunctionParameter(name="factor", type="int", required=False, default=5),
        ],
        returns="int",
        binding=FunctionBinding(
            call="project.sum", args={"first": "lhs", "second": "rhs", "factor": "scale"}
        ),
        conformance="unused.yaml",
    )
    bound = BoundFunction("sum", contract, target)
    activated = SimpleNamespace(bound=lambda _: bound)
    condition = RuntimeCondition(
        phase="join", condition="multiple_matches", context={}, requirement="REQ-0127"
    )

    class Resolver:
        """Keep source failures and missing readings distinct before invocation."""

        def resolve(self, name):
            """Record every authored occurrence, including reads after a missing value."""
            reads.append(name)
            if name == "SRC.A":
                return ResolvedValue(value=10 if scenario == "values" else MISSING)
            if scenario == "missing_then_error":
                return FailedResolution(condition=condition)
            return ResolvedValue(value=20)

    arguments = {"second": "SRC.B", "first": "SRC.A"}
    if scenario == "missing_then_error":
        arguments = {"first": "SRC.A", "second": "SRC.B"}
    result = function_handlers(activated)["function"](
        {"name": "sum", "contract_version": "1", "args": arguments}, Resolver()
    )
    assert reads == (
        ["SRC.A", "SRC.B"] if scenario == "missing_then_error" else ["SRC.B", "SRC.A"]
    )
    if scenario == "values":
        assert result == ValueResult(value=11)
        assert calls == [[("lhs", 10), ("rhs", 20), ("scale", 5)]]
    elif scenario == "missing":
        assert result == ValueResult(value=MISSING)
        assert calls == []
    else:
        assert result == ConditionResult(condition=condition)
        assert calls == []
