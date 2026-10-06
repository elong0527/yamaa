"""Temporary syntax-to-typed-predicate lowering; all data evaluation remains native."""

from yamaa.expressions import PredicateError, predicate_identifiers
from yamaa.models import INT64_MAX, INT64_MIN, DateTimeValue, DateValue
from yamaa.planning import (
    ExecutionDiagnostic,
    ExecutionPlanningError,
    UnsupportedFeature,
    UnsupportedPlanningError,
)
from yamaa.planning.predicate_syntax import analyze_predicate


def admit(text, path, *, predicate_analyzer=None):
    """Parse the whole predicate and refuse unsupported representations before IO."""
    try:
        ast = (analyze_predicate if predicate_analyzer is None else predicate_analyzer)(
            text
        ).ast
    except PredicateError as error:
        raise ExecutionPlanningError(
            [
                ExecutionDiagnostic(
                    phase="validation",
                    condition="invalid_predicate",
                    spec_paths=(path,),
                    requirement=error.requirement,
                    context={
                        "predicate": text,
                        "position": error.position,
                        **getattr(error, "native_context", {}),
                    },
                )
            ]
        ) from error
    unsupported = []

    def visit(node):
        """Inspect literal/call admission without evaluating any row or operand."""
        if isinstance(node, list):
            for item in node:
                visit(item)
        elif isinstance(node, dict):
            feature = None
            if node.get("kind") == "like" and any(
                0xD800 <= ord(c) <= 0xDFFF for c in (node.get("escape") or "")
            ):
                feature = "predicate_non_scalar_text"
            if node.get("kind") == "call" and any(
                0xD800 <= ord(c) <= 0xDFFF for c in node["pattern"]
            ):
                feature = "predicate_non_scalar_text"
            if node.get("kind") == "literal":
                if node["type"] == "int":
                    digits = node["value"].lstrip("+-").lstrip("0") or "0"
                    canonical = ("-" if node["value"].startswith("-") else "") + digits
                    if len(digits) > 19 or not INT64_MIN <= int(canonical) <= INT64_MAX:
                        feature = "predicate_wide_integer_literal"
                elif node["type"] == "str" and any(
                    0xD800 <= ord(c) <= 0xDFFF for c in node["value"]
                ):
                    feature = "predicate_non_scalar_text"
            if feature and feature not in unsupported:
                unsupported.append(feature)
            for child in node.values():
                if isinstance(child, (dict, list)):
                    visit(child)

    visit(ast)
    if unsupported:
        raise UnsupportedPlanningError(
            [
                UnsupportedFeature(operation=feature, spec_path=path)
                for feature in unsupported
            ]
        )
    return ast


def row_scope(ast, row, path, available):
    """Check source-independent phase boundaries for the admitted no-window subset.

    Available defaults have already passed native expression admission. Qualified
    source-field existence still requires the provider's actual schema.
    """
    grouped = row.group_by is not None
    diagnostics = []
    names = predicate_identifiers(ast)
    if grouped:
        # Match the planner: qualified scope errors precede bare phase errors.
        names = tuple(name for name in names if "." in name) + tuple(
            name for name in names if "." not in name
        )
    for name in names:
        qualified = "." in name
        if (grouped and qualified) or (not qualified and name not in available):
            diagnostics.append(
                ExecutionDiagnostic(
                    phase="validation",
                    condition="phase_boundary",
                    spec_paths=(path,),
                    context={
                        "identifier": name,
                        "row": row.id,
                        "available_phase": "row_construction"
                        if qualified
                        else "column_derivation",
                        "required_phase": "grouped_row_filter"
                        if grouped
                        else "row_filter",
                    },
                )
            )
    return diagnostics


def lower(ast, text, path, reference, encode_scalar):
    """Preserve AST association and written occurrences in a flat native arena."""
    nodes = []
    bindings = {}

    def scalar(node):
        """Encode a normalized literal or bind a named scalar without reading data."""
        if node["kind"] == "identifier":
            name = node["name"]
            bindings.setdefault(name, {"name": name, "read": reference(name)})
            return {"identifier": name}
        kind, value = node["type"], node["value"]
        if kind in {"date", "datetime"}:
            cls = DateValue if kind == "date" else DateTimeValue
            temporal = cls.parse(value)
            encoded = {
                kind: {
                    "text": temporal.to_text(),
                    "precision": temporal.collected_precision,
                }
            }
        else:
            if kind == "int":
                # Admission proved the significant digits fit i64; avoid the host
                # integer-string limit on arbitrarily many insignificant zeros.
                digits = value.lstrip("+-").lstrip("0") or "0"
                value = int(("-" if value.startswith("-") else "") + digits)
            elif kind == "float":
                value = float(value)
            encoded = encode_scalar(value)
        return {"literal": encoded}

    def predicate(node):
        """Append children before parents while retaining the original operator."""
        kind = node["kind"]
        if kind == "call":
            encoded = {
                "contains": {
                    "value": scalar(node["source"]),
                    "pattern": node["pattern"],
                }
            }
        elif kind == "boolean":
            encoded = {"boolean": node["value"]}
        elif kind == "not":
            encoded = {"not": predicate(node["value"])}
        elif kind in {"and", "or"}:
            encoded = {kind: [predicate(node["left"]), predicate(node["right"])]}
        elif kind == "comparison":
            encoded = {
                "compare": {
                    "operator": {
                        "=": "eq",
                        "<>": "ne",
                        "<": "lt",
                        "<=": "le",
                        ">": "gt",
                        ">=": "ge",
                    }[node["operator"]],
                    "left": scalar(node["left"]),
                    "right": scalar(node["right"]),
                }
            }
        elif kind == "null_test":
            encoded = {
                "is_null": {"value": scalar(node["value"]), "negated": node["negated"]}
            }
        elif kind in {"in", "between", "like"}:
            payload = {"value": scalar(node["value"]), "negated": node["negated"]}
            if kind == "in":
                payload["items"] = [scalar(item) for item in node["values"]]
            elif kind == "between":
                payload.update(lower=scalar(node["lower"]), upper=scalar(node["upper"]))
            else:
                payload.update(pattern=scalar(node["pattern"]), escape=node["escape"])
            encoded = {kind: payload}
        else:
            raise ValueError("unadmitted native predicate node")
        nodes.append(encoded)
        return len(nodes) - 1

    root = predicate(ast)
    return {
        "path": path,
        "text": text,
        "nodes": nodes,
        "root": root,
        "bindings": list(bindings.values()),
    }
