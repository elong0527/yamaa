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

        def add_positional(value: ast.expr, source: Path, line: int) -> None:
            if isinstance(value, ast.Constant) and isinstance(value.value, str):
                names.add(value.value)
                return
            # Pass-throughs are already resolved by the scans above: a
            # ``*.condition`` attribute forwards a condition held by an
            # exception or result, whose names come from its ``condition``
            # default, class attribute, or a literal ``condition=`` at its
            # construction; a bare ``condition`` name forwards the enclosing
            # function's ``condition`` parameter, whose default and literal
            # call sites are likewise scanned. Anything else is unsupported:
            # fail explicitly instead of silently omitting a name that
            # could be unregistered.
            if isinstance(value, ast.Attribute) and value.attr == "condition":
                return
            if isinstance(value, ast.Name) and value.id == "condition":
                return
            raise AssertionError(
                f"unsupported condition expression at {source}:{line}: "
                f"{ast.dump(value)}"
            )

        # Literal string returns of each function defined in this file, so
        # a condition assigned from a local helper call (e.g.
        # ``condition = classify_project_path(...)``) resolves to the names
        # the helper can produce instead of evading the scan. Only helpers
        # defined in the same file are resolved; propagation through
        # attributes, subscripts, and ``Field(...)`` declarations is not a
        # condition-producing path and stays unscanned, as before.
        helper_returns: dict[str, set[str]] = {}
        for definition in ast.walk(tree):
            if isinstance(definition, (ast.FunctionDef, ast.AsyncFunctionDef)):
                returns = {
                    node.value.value
                    for node in ast.walk(definition)
                    if isinstance(node, ast.Return)
                    and isinstance(node.value, ast.Constant)
                    and isinstance(node.value.value, str)
                }
                if returns:
                    helper_returns[definition.name] = returns

        def add_assignment(
            target: ast.expr,
            value: ast.expr | None,
            helpers: dict[str, set[str]] | None = None,
        ) -> None:
            # Tuple unpacking pairs each target with its own value, e.g.
            # ``condition, requirement = "unknown_field", "REQ-0103"``.
            if (
                isinstance(target, (ast.Tuple, ast.List))
                and isinstance(value, (ast.Tuple, ast.List))
                and len(target.elts) == len(value.elts)
            ):
                for sub_target, sub_value in zip(target.elts, value.elts, strict=True):
                    add_assignment(sub_target, sub_value, helpers)
                return
            if (isinstance(target, ast.Name) and target.id == "condition") or (
                isinstance(target, ast.Attribute) and target.attr == "condition"
            ):
                if isinstance(value, ast.Call):
                    func = value.func
                    helper = (
                        func.attr
                        if isinstance(func, ast.Attribute)
                        else getattr(func, "id", "")
                    )
                    names.update((helpers or {}).get(helper, set()))
                else:
                    add_literal(value)

        # Literal assignments to ``condition`` / ``*.condition`` anywhere in
        # the file: class attributes, instance assignments such as
        # ``self.condition = "..."``, and locals later passed through to a
        # condition constructor. Collecting them here means a pass-through
        # accepted by ``add_positional`` cannot hide a name that was only
        # ever assigned, never passed as a literal at a call site.
        for assignment in ast.walk(tree):
            if isinstance(assignment, ast.Assign):
                for target in assignment.targets:
                    add_assignment(target, assignment.value, helper_returns)
            elif isinstance(assignment, ast.AnnAssign):
                add_assignment(assignment.target, assignment.value, helper_returns)

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
                add_positional(node.args[1], path, node.lineno)
            if name == "_condition":
                for index in condition_positions:
                    if len(node.args) <= index:
                        continue
                    add_positional(node.args[index], path, node.lineno)
    return names


def test_every_python_condition_is_registered() -> None:
    unregistered = _raised_conditions() - _registered_conditions()

    assert unregistered == set()
