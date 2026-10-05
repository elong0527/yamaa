"""Temporary normalized-specification lowering for the closed dataset/1 bridge."""

from __future__ import annotations

import json
import math
import struct

from yamaa.adapters import _native_predicate_plan
from yamaa.expressions import (
    AggregateError,
    NumericError,
    PredicateError,
    numeric_identifiers,
    parse_aggregate_cached,
    parse_numeric_cached,
    parse_predicate,
    predicate_identifiers,
)
from yamaa.models import INT64_MAX, INT64_MIN
from yamaa.planning import (
    ExecutionDiagnostic,
    ExecutionPlanningError,
    UnsupportedFeature,
    UnsupportedPlanningError,
    expression_path,
    preflight_execution,
)

NUMBERING = frozenset({"row_number", "rank"})
WINDOW_VALUES = frozenset({"row_value", "previous_non_missing", "locf"})
WINDOWS = NUMBERING | WINDOW_VALUES | {"baseline_flag"}
OPERATIONS = frozenset({"source", "literal", "aggregate", "compute"}) | WINDOWS
NUMERIC_FUNCTIONS = frozenset(
    {
        "ABS",
        "MOD",
        "GREATEST",
        "LEAST",
        "NULLIF",
        "COALESCE",
        "CEIL",
        "FLOOR",
        "TRUNC",
        "SQRT",
        "ROUND_HALF_AWAY_FROM_ZERO",
    }
)


def primary_source(specification):
    """Select the one admitted driver independently of input mapping order."""
    if specification.rows:
        return specification.rows[0].dataset or next(iter(specification.input))
    return specification.base or next(iter(specification.input))


def admit(specification, *, allow_functions=False):
    """Reject the entire unsupported vocabulary before requesting source tables.

    This accepts schema-normalized models, not arbitrary document dictionaries.
    Binding against actual source schemas remains a later planning operation.
    """
    diagnostics = []
    unsupported = []
    filters = []
    try:
        preflight_execution(
            specification,
            supported_operations=OPERATIONS | {"function"}
            if allow_functions
            else OPERATIONS,
        )
    except ExecutionPlanningError as error:
        diagnostics.extend(error.diagnostics)
    except UnsupportedPlanningError as error:
        unsupported.extend(error.features)

    def reject(operation, path):
        """Retain each unsupported declaration at its original owning path."""
        feature = UnsupportedFeature(operation=operation, spec_path=path)
        if feature not in unsupported:
            unsupported.append(feature)

    primary = primary_source(specification)
    if len(specification.input) > 8 or (
        len(specification.input) > 1
        and not specification.rows
        and specification.base is None
    ):
        reject("multiple_sources", "input")
    if (
        len(specification.input) > 1
        and specification.rows
        and any(row.dataset != primary for row in specification.rows)
    ):
        reject("multiple_row_drivers", "rows")
    if specification.submission is not None:
        reject("submission", "submission")
    named = {item.id for item in specification.intermediates or ()}
    if len(named) > 64:
        reject("intermediate_limit", "intermediates")
    for index, item in enumerate(specification.intermediates or ()):
        path = f"intermediates[{index}]"
        if (
            specification.rows
            or item.dataset not in specification.input
            or item.dataset == primary
        ):
            reject("intermediate_scope", path)
        if any(
            getattr(item, field) is not None
            for field in ("between", "derivations", "verifications")
        ):
            reject("intermediate_selection", path)
        if item.key is not None and (
            not isinstance(item.key, dict)
            or any(
                not isinstance(value, str) or "." in value
                for value in item.key.values()
            )
        ):
            reject("intermediate_key", f"{path}.key")
        if bool(item.order_by) != (item.keep is not None):
            reject("intermediate_order", path)
        if any(
            "." not in term.variable or term.variable.split(".", 1)[0] != item.dataset
            for term in item.order_by or ()
        ):
            reject("intermediate_order_binding", f"{path}.order_by")
        if not (
            item.no_match is None or type(item.no_match) in (str, bool, int, float)
        ) or (
            type(item.no_match) is int and not INT64_MIN <= item.no_match <= INT64_MAX
        ):
            reject("intermediate_absence_literal", f"{path}.no_match")
        if item.filter is not None:
            try:
                ast = _native_predicate_plan.admit(item.filter, f"{path}.filter")
                if any(
                    "." not in name or name.split(".", 1)[0] != item.dataset
                    for name in predicate_identifiers(ast)
                ):
                    reject("intermediate_correlated_filter", f"{path}.filter")
            except ExecutionPlanningError as error:
                diagnostics.extend(error.diagnostics)
            except UnsupportedPlanningError as error:
                unsupported.extend(error.features)
    if specification.filter is not None:
        try:
            ast = _native_predicate_plan.admit(specification.filter, "filter")
            for name in predicate_identifiers(ast):
                if (
                    len(specification.input) > 1
                    and "." in name
                    and name.split(".", 1)[0] != specification.base
                ):
                    reject("secondary_root_filter", "filter")
                if "." not in name:
                    diagnostics.append(
                        ExecutionDiagnostic(
                            phase="validation",
                            condition="phase_boundary",
                            spec_paths=("filter",),
                            context={
                                "identifier": name,
                                "available_phase": "column_derivation",
                                "required_phase": "row_filter",
                            },
                        )
                    )
        except ExecutionPlanningError as error:
            diagnostics.extend(error.diagnostics)
        except UnsupportedPlanningError as error:
            unsupported.extend(error.features)
    for name, source in specification.input.items():
        if source.schema_path is not None:
            reject("source_schema", f"input.{name}.schema")

    def expression(
        declaration,
        path,
        grouped,
        allow_source_filter=False,
        allow_row_lookup=False,
        keyed_nonkey=False,
    ):
        """Admit syntax without evaluating literals or converting output values."""
        if "unconvertible" in declaration.model_fields_set:
            replacement = declaration.unconvertible
            if type(replacement) is int and not INT64_MIN <= replacement <= INT64_MAX:
                reject("wide_integer_literal", f"{path}.unconvertible")
            elif not (
                replacement is None or type(replacement) in (str, bool, int, float)
            ):
                reject("literal_representation", f"{path}.unconvertible")
        operation = declaration.value.operation
        payload = declaration.value.root[operation]
        path = f"{expression_path(path, declaration)}.{operation}"
        if operation == "literal":
            if type(payload) is int and not INT64_MIN <= payload <= INT64_MAX:
                reject("wide_integer_literal", path)
            elif not (payload is None or type(payload) in (str, bool, int, float)):
                reject("literal_representation", path)
        elif operation == "function" and allow_functions:
            from yamaa.functions.invocation import AuthoredValueError, runtime_value

            if grouped or any(
                row.group_by is not None for row in specification.rows or ()
            ):
                reject("grouped_project_function", path)
            if not specification.rows and not keyed_nonkey:
                reject("key_project_function", path)
            for name, value in payload.get("args", {}).items():
                argument_path = f"{path}.args.{name}"
                if isinstance(value, str):
                    if "." in value and value.split(".", 1)[0] != primary:
                        reject("secondary_function_argument", argument_path)
                else:
                    value = (
                        value["literal"]
                        if isinstance(value, dict) and set(value) == {"literal"}
                        else value
                    )
                    try:
                        runtime_value(value)
                    except AuthoredValueError:
                        reject("function_argument_representation", argument_path)
        elif operation == "compute":
            if not isinstance(payload, dict) or set(payload) != {"expr"}:
                reject("compute_policy", path)
                return
            try:
                ast = parse_numeric_cached(payload["expr"])
            except NumericError as error:
                diagnostic = ExecutionDiagnostic(
                    phase="validation",
                    condition=error.condition,
                    spec_paths=(f"{path}.expr",),
                    requirement=error.requirement,
                    context={"expr": payload["expr"], **error.context},
                )
                if diagnostic not in diagnostics:
                    diagnostics.append(diagnostic)
                return

            def inspect(node):
                """Refuse unqualified numeric functions without evaluating any operand."""
                if isinstance(node, dict):
                    if (
                        node.get("kind") == "call"
                        and node["name"] not in NUMERIC_FUNCTIONS
                    ):
                        reject(f"numeric_function_{node['name']}", path)
                    for child in node.values():
                        if isinstance(child, (dict, list)):
                            inspect(child)
                elif isinstance(node, list):
                    for child in node:
                        inspect(child)

            inspect(ast)
            for name in numeric_identifiers(ast):
                if "." in name and (
                    grouped or keyed_nonkey or name.split(".", 1)[0] != primary
                ):
                    reject("compute_binding_scope", path)
        elif operation == "source":
            variable = (
                payload
                if isinstance(payload, str)
                else payload.get("variable")
                if isinstance(payload, dict)
                else None
            )
            qualifier = (
                variable.split(".", 1)[0]
                if isinstance(variable, str) and "." in variable
                else None
            )
            if qualifier in named:
                if not allow_source_filter:
                    reject("intermediate_read_scope", path)
                if isinstance(payload, dict) and set(payload) != {"variable"}:
                    reject("intermediate_read_selection", path)
            if qualifier in specification.input and qualifier != primary:
                if not (allow_source_filter or allow_row_lookup):
                    reject("secondary_source_scope", path)
                if isinstance(payload, dict) and set(payload) != {"variable"}:
                    reject("secondary_source_selection", path)
            if isinstance(payload, str):
                return
            if (
                not isinstance(payload, dict)
                or not isinstance(payload.get("variable"), str)
                or set(payload) - {"variable", "filter", "order_by", "keep"}
            ):
                reject("source_selection", path)
                return
            selected = "order_by" in payload or "keep" in payload
            if selected:
                terms = payload.get("order_by")
                variable = payload["variable"]
                dataset = variable.split(".", 1)[0]
                if not allow_source_filter or "." not in variable:
                    reject("source_selection_scope", path)
                if not terms or payload.get("keep") not in {"first", "last"}:
                    # Pair errors are evaluated lazily by the reference; this slice
                    # refuses them without inventing an earlier language condition.
                    reject("source_selection", path)
                elif any(
                    not isinstance(term, dict)
                    or not isinstance(term.get("variable"), str)
                    or "." not in term["variable"]
                    or term["variable"].split(".", 1)[0] != dataset
                    for term in terms
                ):
                    reject("source_order_binding", path)
            if payload.get("filter") is not None:
                if not allow_source_filter:
                    reject("source_filter_scope", path)
                try:
                    variable = payload["variable"]
                    if "." not in variable:
                        diagnostics.append(
                            ExecutionDiagnostic(
                                phase="validation",
                                condition="prohibited_construct",
                                spec_paths=(f"{path}.filter",),
                                requirement="REQ-0148",
                                context={"identifier": variable},
                            )
                        )
                    else:
                        ast = _native_predicate_plan.admit(
                            payload["filter"], f"{path}.filter"
                        )
                        dataset = variable.split(".", 1)[0]
                        for name in predicate_identifiers(ast):
                            if "." not in name or name.split(".", 1)[0] != dataset:
                                diagnostics.append(
                                    ExecutionDiagnostic(
                                        phase="validation",
                                        condition="unknown_field",
                                        spec_paths=(f"{path}.filter",),
                                        requirement="REQ-0132",
                                        context={
                                            "identifier": name,
                                            "dataset": dataset,
                                        },
                                    )
                                )
                except ExecutionPlanningError as error:
                    diagnostics.extend(error.diagnostics)
                except UnsupportedPlanningError as error:
                    unsupported.extend(error.features)
        elif operation in WINDOWS:
            window = payload.get("window", {}) if isinstance(payload, dict) else {}
            if operation in WINDOW_VALUES and isinstance(payload, dict):
                if "." in payload.get("source", ""):
                    reject("window_source", f"{path}.source")
                if operation == "row_value" and payload.get("offset") == 0:
                    diagnostics.append(
                        ExecutionDiagnostic(
                            phase="validation",
                            condition="zero_offset",
                            spec_paths=(f"{path}.offset",),
                            requirement="REQ-0328",
                            context={"offset": 0},
                        )
                    )

            if operation == "baseline_flag" and isinstance(payload, dict):
                types = {column.name: column.type for column in specification.columns}
                fields = [payload.get("date"), payload.get("reference_date")]
                for field, name in zip(["date", "reference_date"], fields, strict=True):
                    if isinstance(name, str) and "." in name:
                        reject("window_source", f"{path}.{field}")
                if all(name in types for name in fields) and not (
                    types[fields[0]] in {"date", "datetime"}
                    and types[fields[1]] == types[fields[0]]
                ):
                    reject("baseline_temporal_types", path)
                if isinstance(window, dict) and window.get("order_by"):
                    diagnostics.append(
                        ExecutionDiagnostic(
                            phase="validation",
                            condition="window_order_by_forbidden",
                            spec_paths=(f"{path}.window.order_by",),
                            requirement="REQ-0341",
                            context={"operation": "baseline_flag"},
                        )
                    )
            if specification.rows or not isinstance(window, dict):
                reject("window_scope", path)
            else:
                names = list(window.get("group_by", []))
                names.extend(
                    term if isinstance(term, str) else term["variable"]
                    for term in window.get("order_by", [])
                )
                if window.get("filter") is not None:
                    try:
                        ast = _native_predicate_plan.admit(
                            window["filter"], f"{path}.window.filter"
                        )
                        names.extend(predicate_identifiers(ast))
                    except ExecutionPlanningError as error:
                        diagnostics.extend(error.diagnostics)
                    except UnsupportedPlanningError as error:
                        unsupported.extend(error.features)
                if any("." in name for name in names):
                    reject("window_source", f"{path}.window")
                groups = window.get("group_by", [])
                if len(set(groups)) != len(groups):
                    reject("window_repeated_group", f"{path}.window.group_by")
        elif operation == "aggregate":
            if not isinstance(payload, dict) or not isinstance(
                payload.get("expr"), str
            ):
                diagnostics.append(
                    ExecutionDiagnostic(
                        phase="validation",
                        condition="invalid_field_type",
                        spec_paths=(path,),
                        requirement="REQ-0321",
                        context={
                            "operation": "aggregate",
                            "expected": "a reducer expression",
                        },
                    )
                )
                return
            try:
                ast = parse_aggregate_cached(payload["expr"])
            except AggregateError as error:
                diagnostics.append(
                    ExecutionDiagnostic(
                        phase="validation",
                        condition=error.condition,
                        spec_paths=(
                            path if set(payload) == {"expr"} else f"{path}.expr",
                        ),
                        requirement=error.requirement,
                        context={"expr": payload["expr"], **error.context},
                    )
                )
                return
            if not (
                grouped
                and set(payload) == {"expr"}
                and ast["kind"] == "reduction"
                and ast["name"] in {"SUM", "MEAN"}
                and ast["argument"]["kind"] == "identifier"
                and "." in ast["argument"]["name"]
            ):
                reject("aggregate_scope_or_expression", path)
        else:
            reject(operation, path)

    for index, row in enumerate(specification.rows or ()):
        if row.submission is not None:
            reject("submission", f"rows[{index}].submission")
        if row.filter is not None:
            try:
                path = f"rows[{index}].filter"
                ast = _native_predicate_plan.admit(row.filter, path)
                filters.append((row, path, ast))
                if row.group_by is None and any(
                    "." in name and name.split(".", 1)[0] != primary
                    for name in predicate_identifiers(ast)
                ):
                    reject("secondary_row_filter", path)
            except ExecutionPlanningError as error:
                diagnostics.extend(error.diagnostics)
            except UnsupportedPlanningError as error:
                unsupported.extend(error.features)
        for name, declaration in row.derivations.items():
            expression(
                declaration,
                f"rows[{index}].derivations.{name}",
                row.group_by is not None,
                allow_row_lookup=True,
            )
    for column in specification.columns:
        if column.verifications:
            reject("column_verifications", f"columns.{column.name}.verifications")
        if column.submission is not None:
            reject("submission", f"columns.{column.name}.submission")
        if column.derivation is not None:
            if (
                column.name in specification.keys
                and column.derivation.value.operation in WINDOWS
            ):
                reject("window_key", f"columns.{column.name}.derivation")
            expression(
                column.derivation,
                f"columns.{column.name}.derivation",
                False,
                allow_source_filter=not specification.rows
                and column.name not in specification.keys,
                keyed_nonkey=not specification.rows
                and column.name not in specification.keys,
            )
    for index, declaration in enumerate(specification.verifications or ()):
        operation = declaration.operation
        payload = declaration.root[operation]
        path = f"verifications[{index}].{operation}"
        if operation == "unique" and isinstance(payload, list):
            payload = {"columns": payload}
        allowed = {
            "unique": {"columns", "id", "severity"},
            "row_count": {"min", "max", "id", "severity"},
            "assert": {"expr", "id", "severity"},
            "implies": {"when", "then", "id", "severity"},
        }.get(operation, set())
        if (
            operation not in {"unique", "row_count", "assert", "implies"}
            or not isinstance(payload, dict)
            or set(payload) - allowed
            or payload.get("severity", "error") != "error"
        ):
            reject("dataset_verification", path)
        elif operation == "row_count" and any(
            type(value) is int and not INT64_MIN <= value <= INT64_MAX
            for value in (payload.get("min"), payload.get("max"))
        ):
            reject("row_count_bounds", path)
        elif operation in {"assert", "implies"}:
            for field in ("expr",) if operation == "assert" else ("when", "then"):
                text = payload.get(field)
                if not isinstance(text, str):
                    continue
                try:
                    _native_predicate_plan.admit(text, f"{path}.{field}")
                except ExecutionPlanningError:
                    # A declaration error belongs after keys and earlier checks.
                    # Lowering retains it as a pending error, never evaluates it.
                    pass
                except UnsupportedPlanningError as error:
                    unsupported.extend(error.features)
    if diagnostics:
        raise ExecutionPlanningError(diagnostics)
    if unsupported:
        raise UnsupportedPlanningError(unsupported)
    # In this admitted subset, every column-level source/literal derivation is
    # row-local and is promoted when a filter reads it (REQ-1260). Windows,
    # dataset aggregates and intermediates in explicit rows were refused above; do not infer
    # their phase availability here or duplicate the general planner.
    defaults = {column.name for column in specification.columns if column.derivation}
    for row, path, ast in filters:
        diagnostics.extend(
            _native_predicate_plan.row_scope(
                ast, row, path, defaults | row.derivations.keys()
            )
        )
    if diagnostics:
        raise ExecutionPlanningError(diagnostics)


def literal(value):
    """Encode admitted scalar literals without completed-result conversion."""
    if value is None or (type(value) is float and not math.isfinite(value)):
        return {"missing": None}
    if type(value) is bool:
        return {"bool": value}
    if type(value) is int:
        return {"int": str(value)}
    if type(value) is float:
        return {"float": struct.pack(">d", value).hex()}
    return {"str": value}


def lower(plan, source, secondary=None, *, functions=None):
    """Lower validated dependency order and bindings, never evaluate expressions."""
    spec = plan.specification
    dataset = primary_source(spec)
    secondary = secondary or {}
    secondary_indices = {name: index for index, name in enumerate(secondary)}
    secondary_fields = {
        name: {column.name: index for index, column in enumerate(table.columns)}
        for name, table in secondary.items()
    }
    output_types = {column.name: column.type for column in spec.columns}
    inputs = {column.name: index for index, column in enumerate(source.columns)}
    outputs = {column.name: index for index, column in enumerate(spec.columns)}
    keyed = not spec.rows
    intermediates = []
    intermediate_indices = {}
    for index, item in enumerate(plan.intermediates):
        fields = secondary_fields.get(item.dataset, {})
        table = secondary.get(item.dataset)
        if (
            table is None
            or item.match_expressions
            or not item.match_fields
            or len(item.match_fields) != len(item.match_variables)
            or any(
                field not in fields
                or variable not in outputs
                or table.columns[fields[field]].type != output_types[variable]
                for field, variable in zip(item.match_fields, item.match_variables)
            )
            or any(field not in fields for _, field in item.order_terms)
        ):
            raise UnsupportedPlanningError(
                (
                    UnsupportedFeature(
                        operation="intermediate_binding", spec_path=item.path
                    ),
                )
            )
        bound = {
            "identifier": item.identifier,
            "path": item.path,
            "source": secondary_indices[item.dataset],
            "keys": [
                {"source_column": fields[field], "output_column": outputs[variable]}
                for field, variable in zip(item.match_fields, item.match_variables)
            ],
        }
        if item.filter_predicate is not None:
            bound["filter"] = _native_predicate_plan.lower(
                item.filter_predicate,
                spec.intermediates[index].filter,
                f"{item.path}.filter",
                lambda name, fields=fields: {"source": fields[name.split(".", 1)[1]]},
                literal,
            )
        if item.order_terms:
            bound["selection"] = {
                "order_by": [
                    {
                        "column": fields[field],
                        "descending": term.direction == "desc",
                        "nulls_first": term.nulls == "first",
                    }
                    for term, field in item.order_terms
                ],
                "keep": item.keep,
            }
        if item.no_match_declared:
            bound["no_match"] = literal(item.no_match)
        intermediate_indices[item.identifier] = (index, fields)
        intermediates.append(bound)

    def reference(name):
        """Only the admitted driver or completed output columns are readable."""
        if "." not in name:
            return {"column": outputs[name]}
        qualifier, field = name.split(".", 1)
        if qualifier != dataset:
            raise ValueError("native plan contains an unadmitted source binding")
        return {"source": inputs[field]}

    def assignment(derived, collect=False):
        """Keep the original operation-qualified path and reduction text."""
        op = derived.declaration.value.operation
        value = derived.declaration.value.root[op]
        if op == "literal":
            expression = {"literal": literal(value)}
        elif op == "function":
            from yamaa.adapters._native_project_functions import authored

            if functions is None:
                raise ValueError("native project functions require activated bindings")
            arguments = []
            for name, argument in value.get("args", {}).items():
                if isinstance(argument, str):
                    read = reference(argument)
                    if collect and "source" in read:
                        read = {
                            "collect": {
                                "column": read["source"],
                                "identifier": argument,
                            }
                        }
                else:
                    item = (
                        argument["literal"]
                        if isinstance(argument, dict) and set(argument) == {"literal"}
                        else argument
                    )
                    read = {"literal": authored(item)}
                arguments.append({"name": name, "input": read})
            expression = {
                "function": {
                    "slot": functions.names.index(value["name"]),
                    "arguments": arguments,
                }
            }
        elif op == "compute":
            expression = {
                "compute": {
                    "text": value["expr"],
                    "bindings": [
                        {"name": name, "read": reference(name)}
                        for name in numeric_identifiers(
                            parse_numeric_cached(value["expr"])
                        )
                    ],
                }
            }
        elif op == "source":
            name = value if isinstance(value, str) else value["variable"]
            qualifier, dot, field = name.partition(".")
            if dot and qualifier in intermediate_indices:
                index, fields = intermediate_indices[qualifier]
                expression = {"intermediate": {"index": index, "column": fields[field]}}
            elif dot and qualifier in secondary:
                join = next(
                    (
                        join
                        for join in derived.implicit_joins
                        if join.dataset == qualifier
                    ),
                    None,
                )
                fields = secondary_fields[qualifier]
                table = secondary[qualifier]
                row_keys = join.match_variables if join is not None else None
                if row_keys is not None:
                    if len(row_keys) != len(join.keys) or any(
                        not variable.startswith(f"{dataset}.")
                        or variable.count(".") != 1
                        or variable.split(".", 1)[1] not in inputs
                        or key not in fields
                        or table.columns[fields[key]].type
                        != source.columns[inputs[variable.split(".", 1)[1]]].type
                        for key, variable in zip(join.keys, row_keys, strict=True)
                    ):
                        raise UnsupportedPlanningError(
                            (
                                UnsupportedFeature(
                                    operation="row_source_binding",
                                    spec_path=derived.operation_path,
                                ),
                            )
                        )
                    return {
                        "column": outputs[derived.column],
                        "path": derived.operation_path,
                        "expression": {
                            "row_lookup": {
                                "source": secondary_indices[qualifier],
                                "column": fields[field],
                                "keys": [
                                    {
                                        "source_column": fields[key],
                                        "driver_column": inputs[
                                            variable.split(".", 1)[1]
                                        ],
                                    }
                                    for key, variable in zip(
                                        join.keys, row_keys, strict=True
                                    )
                                ],
                            }
                        },
                    }
                if (
                    not collect
                    or join is None
                    or not join.keys
                    or any(
                        key not in outputs
                        or key not in fields
                        or table.columns[fields[key]].type != output_types[key]
                        for key in join.keys
                    )
                ):
                    raise UnsupportedPlanningError(
                        (
                            UnsupportedFeature(
                                operation="secondary_source_binding",
                                spec_path=derived.operation_path,
                            ),
                        )
                    )
                expression = {
                    "lookup": {
                        "source": secondary_indices[qualifier],
                        "column": fields[field],
                        "keys": [
                            {
                                "source_column": fields[key],
                                "output_column": outputs[key],
                            }
                            for key in join.keys
                        ],
                    }
                }
            else:
                expression = (
                    {
                        "collect": {
                            "column": reference(name)["source"],
                            "identifier": name,
                        }
                    }
                    if collect and "." in name
                    else reference(name)
                )
            if isinstance(value, dict) and value.get("filter") is not None:
                text = value["filter"]
                expression["collect"]["filter"] = _native_predicate_plan.lower(
                    parse_predicate(text),
                    text,
                    derived.operation_path,
                    reference,
                    literal,
                )
            if isinstance(value, dict) and value.get("order_by") is not None:
                terms = value["order_by"]
                if any(
                    term["variable"].split(".", 1)[1] not in inputs for term in terms
                ):
                    raise UnsupportedPlanningError(
                        (
                            UnsupportedFeature(
                                operation="source_order_binding",
                                spec_path=derived.operation_path,
                            ),
                        )
                    )
                expression["collect"]["selection"] = {
                    "order_by": [
                        {
                            "column": reference(term["variable"])["source"],
                            "descending": term.get("direction", "asc") == "desc",
                            "nulls_first": term.get("nulls", "last") == "first",
                        }
                        for term in terms
                    ],
                    "keep": value["keep"],
                }
        elif op in WINDOWS:
            window = value.get("window", {})
            tag = "number" if op in NUMBERING else "window"
            if op in NUMBERING:
                kind = (
                    "row_number"
                    if op == "row_number"
                    else value.get("method", "competition")
                )
            elif op == "baseline_flag":
                kind = {
                    op: {
                        "date": outputs[value["date"]],
                        "reference_date": outputs[value["reference_date"]],
                    }
                }
            else:
                payload = {"column": outputs[value["source"]]}
                if op == "row_value":
                    payload["offset"] = str(value["offset"])
                kind = {op: payload}
            expression = {
                tag: {
                    "kind": kind,
                    "group_by": [outputs[name] for name in window.get("group_by", [])],
                    "order_by": [
                        {
                            "column": outputs[
                                term if isinstance(term, str) else term["variable"]
                            ],
                            "descending": isinstance(term, dict)
                            and term.get("direction", "asc") == "desc",
                            "nulls_first": isinstance(term, dict)
                            and term.get("nulls", "last") == "first",
                        }
                        for term in window.get("order_by", [])
                    ],
                }
            }
            if window.get("filter") is not None:
                text = window["filter"]
                expression[tag]["filter"] = _native_predicate_plan.lower(
                    parse_predicate(text),
                    text,
                    derived.operation_path,
                    reference,
                    literal,
                )
        else:
            ast = parse_aggregate_cached(value["expr"])
            expression = {
                "reduce": {
                    "column": reference(ast["argument"]["name"])["source"],
                    "reducer": ast["name"],
                    "text": value["expr"],
                }
            }
        return {
            "column": outputs[derived.column],
            "path": derived.operation_path,
            "expression": expression,
        }

    # Plan declaration order precedes execution order: key assignments move into
    # a template, while every authored handler (including unused ones) starts at zero.
    declarations = [item for row in plan.rows for item in row.derivations]
    declarations.extend(plan.columns)
    handlers = {}
    for item in declarations:
        if "unconvertible" in item.declaration.model_fields_set:
            path = f"{item.path}.unconvertible"
            handlers.setdefault(
                path,
                {
                    "assignment_path": item.operation_path,
                    "path": path,
                    "value": literal(item.declaration.unconvertible),
                },
            )
    verifications, declaration_error = checks(spec, outputs)
    return {
        "protocol": "dataset/1",
        **(
            {"functions": json.loads(functions.declarations)}
            if functions is not None
            else {}
        ),
        **({"unconvertible": list(handlers.values())} if handlers else {}),
        **({"intermediates": intermediates} if intermediates else {}),
        "source": [
            {"name": column.name, "kind": column.type} for column in source.columns
        ],
        **(
            {
                "secondary": [
                    {
                        "name": name,
                        "schema": [
                            {"name": column.name, "kind": column.type}
                            for column in table.columns
                        ],
                    }
                    for name, table in secondary.items()
                ]
            }
            if secondary
            else {}
        ),
        "output": [
            {"name": column.name, "kind": column.type} for column in spec.columns
        ],
        "templates": [
            {
                "mode": {"keys": None},
                "assignments": [
                    assignment(item)
                    for item in plan.columns
                    if item.column in spec.keys
                ],
                **(
                    {
                        "filter": _native_predicate_plan.lower(
                            plan.rows[0].filter_predicate,
                            spec.filter,
                            "filter",
                            reference,
                            literal,
                        )
                    }
                    if spec.filter is not None
                    else {}
                ),
            }
        ]
        if keyed
        else [
            {
                "mode": {"groups": [inputs[name] for name in row.group_fields]}
                if row.grouped
                else {"records": None},
                "assignments": [assignment(item) for item in row.derivations],
                **(
                    {
                        "filter": _native_predicate_plan.lower(
                            row.filter_predicate,
                            row.declaration.filter,
                            row.filter_path,
                            reference,
                            literal,
                        )
                    }
                    if row.filter_predicate is not None
                    else {}
                ),
            }
            for row in plan.rows
        ],
        "columns": [
            assignment(item, collect=keyed)
            for item in plan.columns
            if not keyed or item.column not in spec.keys
        ],
        "keys": [outputs[name] for name in spec.keys],
        "verifications": verifications,
    }, declaration_error


def checks(specification, outputs):
    """Lower the valid check prefix and retain the first runtime declaration error.

    The reference evaluates declarations in order. An invalid later declaration
    retains earlier records but overrides their data failures; derivation and
    output-key failures still take precedence. No table is checked in Python.
    """
    lowered = []
    identifiers = {}

    def invalid(
        path, requirement, reason, condition="invalid_declaration", context=None
    ):
        """Use the existing public declaration-diagnostic vocabulary exactly."""
        return lowered, ExecutionDiagnostic(
            phase="validation",
            condition=condition,
            spec_paths=(path,),
            requirement=requirement,
            context=context or {"reason": reason},
        )

    for index, declaration in enumerate(specification.verifications or ()):
        op = declaration.operation
        payload = declaration.root[op]
        if isinstance(payload, list):
            payload = {"columns": payload}
        path = f"verifications[{index}].{op}"
        identifier = payload.get("id")
        if "id" in payload:
            if not isinstance(identifier, str) or not identifier:
                return invalid(path, "REQ-0374", "a verification id is text")
            if identifier in identifiers:
                return invalid(
                    path,
                    "REQ-0398",
                    f"verification id {identifier!r} repeats {identifiers[identifier]}",
                    "duplicate_identifier",
                )
            identifiers[identifier] = path
        if op == "unique":
            names = payload.get("columns")
            if not isinstance(names, list) or not names:
                return invalid(path, "REQ-0397", "columns names at least one column")
            if any(not isinstance(name, str) for name in names):
                return invalid(path, "REQ-0397", "columns names are text")
            for name in names:
                if name not in outputs:
                    return invalid(
                        f"{path}.columns",
                        "REQ-0405",
                        f"unknown column {name!r}",
                        "unknown_field",
                    )
            check = {op: [outputs[name] for name in names]}
        elif op in {"assert", "implies"}:
            predicates = {}
            for field in ("expr",) if op == "assert" else ("when", "then"):
                if field == "then":
                    # Validate when natively before any later syntax/name error
                    # in then, without evaluating rows or emitting a record.
                    lowered.append(
                        {
                            "path": path,
                            "check": {"predicate_declaration": predicates["when"]},
                        }
                    )
                text = payload.get(field)
                predicate_path = f"{path}.{field}"
                if not isinstance(text, str):
                    return invalid(
                        predicate_path, "REQ-0397", "a predicate must be text"
                    )
                try:
                    ast = parse_predicate(text)
                except PredicateError as error:
                    return invalid(
                        predicate_path,
                        error.requirement,
                        str(error),
                        "invalid_predicate",
                    )
                unknown = sorted(set(predicate_identifiers(ast)) - outputs.keys())
                if unknown:
                    return invalid(
                        predicate_path,
                        "REQ-0405",
                        f"predicate names unknown column {unknown[0]!r}",
                        "unknown_field",
                        {"identifier": unknown[0]},
                    )
                predicates[field] = _native_predicate_plan.lower(
                    ast,
                    text,
                    predicate_path,
                    lambda name: {"column": outputs[name]},
                    literal,
                )
            if op == "implies":
                lowered.pop()  # Both declarations are complete; use the ordinary check.
            check = {op: predicates["expr"] if op == "assert" else predicates}
        else:
            minimum, maximum = payload.get("min"), payload.get("max")
            if any(
                value is not None and type(value) is not int
                for value in (minimum, maximum)
            ):
                return invalid(path, "REQ-0397", "a row_count bound is an int")
            if minimum is None and maximum is None:
                return invalid(path, "REQ-0399", "row_count requires one bound")
            if minimum is not None and maximum is not None and minimum > maximum:
                return invalid(path, "REQ-0399", "row_count min exceeds max")
            check = {
                op: {
                    bound: str(payload[bound])
                    if payload.get(bound) is not None
                    else None
                    for bound in ("min", "max")
                }
            }
        lowered.append({"path": path, "check": check})
    return lowered, None
