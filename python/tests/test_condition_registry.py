"""Every condition the python implementation raises is registered.

`yaml/conditions.yaml` is the validation-condition registry: it maps every
condition an implementation may raise to the phases that may raise it.
The Rust core's diagnostic names all appear there; this test keeps the
python implementation to the same contract by scanning its sources for
the condition names it constructs.
"""

from __future__ import annotations

import ast
import re
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).parents[2]
SOURCE_ROOT = REPOSITORY_ROOT / "python" / "src" / "yamaa"
REGISTRY = REPOSITORY_ROOT / "yaml" / "conditions.yaml"


def _registered_conditions() -> set[str]:
    return set(re.findall(r"^  ([a-z_0-9]+):", REGISTRY.read_text(), re.MULTILINE))


def _raised_conditions() -> set[str]:
    names: set[str] = set()
    for path in SOURCE_ROOT.rglob("*.py"):
        tree = ast.parse(path.read_text(encoding="utf-8"))
        condition_positions = {
            index
            for definition in ast.walk(tree)
            if isinstance(definition, (ast.FunctionDef, ast.AsyncFunctionDef))
            and definition.name == "_condition"
            for index, parameter in enumerate(
                [*definition.args.posonlyargs, *definition.args.args]
            )
            if parameter.arg == "condition"
        }

        def add_literal(value: ast.expr | None) -> None:
            if isinstance(value, ast.Constant) and isinstance(value.value, str):
                names.add(value.value)

        for definition in ast.walk(tree):
            if isinstance(definition, ast.ClassDef):
                for statement in definition.body:
                    if isinstance(statement, ast.Assign):
                        targets = statement.targets
                        value = statement.value
                    elif isinstance(statement, ast.AnnAssign):
                        targets = [statement.target]
                        value = statement.value
                    else:
                        continue
                    if any(
                        isinstance(target, ast.Name) and target.id == "condition"
                        for target in targets
                    ):
                        add_literal(value)
            if isinstance(definition, (ast.FunctionDef, ast.AsyncFunctionDef)):
                parameters = [
                    *definition.args.posonlyargs,
                    *definition.args.args,
                ]
                defaults = [
                    *([None] * (len(parameters) - len(definition.args.defaults))),
                    *definition.args.defaults,
                ]
                for parameter, default in zip(parameters, defaults, strict=True):
                    if parameter.arg == "condition":
                        add_literal(default)
                for parameter, default in zip(
                    definition.args.kwonlyargs,
                    definition.args.kw_defaults,
                    strict=True,
                ):
                    if parameter.arg == "condition":
                        add_literal(default)
        for node in ast.walk(tree):
            if not isinstance(node, ast.Call):
                continue
            for keyword in node.keywords:
                if (
                    keyword.arg == "condition"
                    and isinstance(keyword.value, ast.Constant)
                    and isinstance(keyword.value.value, str)
                ):
                    names.add(keyword.value.value)
            func = node.func
            name = (
                func.attr
                if isinstance(func, ast.Attribute)
                else getattr(func, "id", "")
            )
            if name == "expression_condition" and len(node.args) >= 2:
                condition = node.args[1]
                if isinstance(condition, ast.Constant) and isinstance(
                    condition.value, str
                ):
                    names.add(condition.value)
            if name == "_condition":
                for index in condition_positions:
                    if len(node.args) <= index:
                        continue
                    condition = node.args[index]
                    if isinstance(condition, ast.Constant) and isinstance(
                        condition.value, str
                    ):
                        names.add(condition.value)
    return names


def test_every_python_condition_is_registered() -> None:
    unregistered = _raised_conditions() - _registered_conditions()

    assert unregistered == set()
