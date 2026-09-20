"""Read every logical call a specification writes, and check it can be met.

REQ-0662 lets a portable specification name a logical contract long before any
project implements it, so nothing here runs until a runner selects a project
root. Once one is selected, this is the implementation stage: the calls are
read out of the specification with the paths a reviewer sees, and each is
held to the contract the environment actually provides -- its name, its
exact version, and its closed, exactly typed signature.
"""

from __future__ import annotations

from collections.abc import Iterator, Mapping, Sequence
from dataclasses import dataclass

from pydantic import JsonValue

from yamaa.functions.fingerprint import function_value_type
from yamaa.functions.models import FunctionContract, ProjectEnvironment
from yamaa.planning import ExecutionDiagnostic, expression_path
from yamaa.specification.models import ColumnType, Expression, Specification

# R011 reads an undeclared delimited field as text, so an argument naming
# one has a statically known type after all.
_UNDECLARED_FIELD_TYPE: ColumnType = "str"


@dataclass(frozen=True, slots=True)
class FunctionCall:
    """One `function` expression and the specification path it is written at."""

    name: str
    contract_version: str
    args: Mapping[str, JsonValue]
    spec_path: str


def _calls_in(value: object, path: str) -> Iterator[FunctionCall]:
    """Yield the calls one expression tree writes, at their authored paths."""
    if isinstance(value, Mapping):
        if len(value) == 1 and "function" in value:
            payload = value["function"]
            if isinstance(payload, Mapping):
                name = payload.get("name")
                version = payload.get("contract_version")
                arguments = payload.get("args") or {}
                if (
                    isinstance(name, str)
                    and isinstance(version, str)
                    and isinstance(arguments, Mapping)
                ):
                    yield FunctionCall(
                        name=name,
                        contract_version=version,
                        args=dict(arguments),
                        spec_path=f"{path}.function",
                    )
            return
        for key, child in value.items():
            yield from _calls_in(child, f"{path}.{key}")
    elif isinstance(value, Sequence) and not isinstance(value, (str, bytes)):
        for index, child in enumerate(value):
            yield from _calls_in(child, f"{path}[{index}]")


def _expression_calls(expression: Expression, path: str) -> Iterator[FunctionCall]:
    return _calls_in(expression.root, path)


def function_calls(specification: Specification) -> tuple[FunctionCall, ...]:
    """Return every logical call the specification writes, in authored order."""
    calls: list[FunctionCall] = []
    for index, row in enumerate(specification.rows or ()):
        for name, declaration in row.derivations.items():
            path = f"rows[{index}].derivations.{name}"
            calls.extend(
                _expression_calls(declaration.value, expression_path(path, declaration))
            )
    for column in specification.columns:
        declaration = column.derivation
        if declaration is None:
            continue
        path = f"columns.{column.name}.derivation"
        calls.extend(
            _expression_calls(declaration.value, expression_path(path, declaration))
        )
    return tuple(calls)


class _TypeCatalog:
    """The types a specification declares, as an argument sees them."""

    def __init__(self, specification: Specification) -> None:
        self._columns = {column.name: column.type for column in specification.columns}
        self._datasets = {
            dataset: dict(source.types or {})
            for dataset, source in specification.input.items()
        }
        self._lookups = {
            intermediate.id: intermediate.dataset
            for intermediate in specification.intermediates or ()
        }

    def resolve(self, variable: str) -> ColumnType | None:
        """Return the declared type of one name, or None when it declares none.

        A name this catalog cannot place is left to the planner, which
        reports an unknown identifier for exactly the same reference rather
        than having two components disagree about what a typo is.
        """
        if "." not in variable:
            return self._columns.get(variable)
        qualifier, field = variable.split(".", 1)
        dataset = self._lookups.get(qualifier, qualifier)
        fields = self._datasets.get(dataset)
        if fields is None:
            return None
        return fields.get(field, _UNDECLARED_FIELD_TYPE)


def _diagnostic(
    condition: str,
    requirement: str,
    spec_path: str,
    context: dict[str, JsonValue],
) -> ExecutionDiagnostic:
    return ExecutionDiagnostic(
        phase="validation",
        condition=condition,
        spec_paths=(spec_path,),
        requirement=requirement,
        context=context,
    )


def _argument_diagnostics(
    call: FunctionCall,
    contract: FunctionContract,
    catalog: _TypeCatalog,
) -> list[ExecutionDiagnostic]:
    """Hold one call to the closed, exactly typed signature of REQ-0676."""
    diagnostics: list[ExecutionDiagnostic] = []
    parameters = contract.parameters
    for name in sorted(set(call.args) - set(parameters)):
        diagnostics.append(
            _diagnostic(
                "invalid_function_argument",
                "REQ-0700",
                f"{call.spec_path}.args.{name}",
                {"function": call.name, "reason": "unknown argument"},
            )
        )
    for name, parameter in parameters.items():
        path = f"{call.spec_path}.args.{name}"
        if name not in call.args:
            if parameter.required:
                diagnostics.append(
                    _diagnostic(
                        "invalid_function_argument",
                        "REQ-0700",
                        path,
                        {
                            "function": call.name,
                            "reason": "a required argument was not supplied",
                        },
                    )
                )
            continue
        value = call.args[name]
        if isinstance(value, str):
            # REQ-0679: a plain string names a variable, and REQ-0678 admits
            # no conversion between its declared type and the parameter's.
            actual = catalog.resolve(value)
            if actual is not None and actual != parameter.type:
                diagnostics.append(
                    _diagnostic(
                        "invalid_function_argument",
                        "REQ-0700",
                        path,
                        {
                            "function": call.name,
                            "variable": value,
                            "expected": parameter.type,
                            "actual": actual,
                        },
                    )
                )
            continue
        literal = value
        if isinstance(value, Mapping) and set(value) == {"literal"}:
            literal = value["literal"]
        if literal is None:
            # REQ-0681 makes an explicit missing valid authoring: a
            # non-accepting parameter short-circuits rather than failing.
            continue
        actual_literal = function_value_type(literal)
        if actual_literal != parameter.type:
            diagnostics.append(
                _diagnostic(
                    "invalid_function_argument",
                    "REQ-0700",
                    path,
                    {
                        "function": call.name,
                        "expected": parameter.type,
                        "actual": actual_literal,
                    },
                )
            )
    return diagnostics


def validate_calls(
    specification: Specification,
    environment: ProjectEnvironment,
) -> tuple[ExecutionDiagnostic, ...]:
    """Check every call against the contracts this project actually provides."""
    catalog = _TypeCatalog(specification)
    diagnostics: list[ExecutionDiagnostic] = []
    for call in function_calls(specification):
        contract = environment.functions.get(call.name)
        if contract is None:
            diagnostics.append(
                _diagnostic(
                    "unknown_project_function",
                    "REQ-0698",
                    f"{call.spec_path}.name",
                    {
                        "function": call.name,
                        "available": sorted(environment.functions),
                    },
                )
            )
            continue
        if call.contract_version != contract.contract_version:
            diagnostics.append(
                _diagnostic(
                    "function_contract_mismatch",
                    "REQ-0699",
                    f"{call.spec_path}.contract_version",
                    {
                        "function": call.name,
                        "requested": call.contract_version,
                        "available": contract.contract_version,
                    },
                )
            )
            continue
        diagnostics.extend(_argument_diagnostics(call, contract, catalog))
    return tuple(diagnostics)


__all__ = ["FunctionCall", "function_calls", "validate_calls"]
