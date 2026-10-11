"""Approved citation-only disposition; historical source reports remain unchanged."""

import copy


def reconciled_schema_requirements(report, *, resolved_output=False):
    expected = copy.deepcopy(report)

    def visit(value):
        if isinstance(value, list):
            for item in value:
                visit(item)
        elif isinstance(value, dict):
            diagnostic = (
                {"condition", "requirement", "context"} <= value.keys()
                and ("path" in value or "spec_paths" in value)
                and value.get("phase", "validation") == "validation"
            )
            if not diagnostic:
                for key in ("outcome", "results", "diagnostics"):
                    if key in value:
                        visit(value[key])
                return
            # Context is retained source data, even if it resembles a diagnostic.
            if value["requirement"] is not None:
                return
            condition = value["condition"]
            requirement = {
                "schema_version_mismatch": "REQ-0281",
                "unknown_field": "REQ-0285",
                "missing_required_field": "REQ-0285",
                "invalid_operation_count": "REQ-0286",
                "unknown_operation": "REQ-0286",
            }.get(condition)
            if (
                condition == "missing_required_field"
                and resolved_output
                and value.get("path") == "output"
            ):
                requirement = "REQ-0657"
            if condition == "model_contract_mismatch":
                context = value["context"]
                reason = (
                    context.get("reason")
                    if isinstance(context, dict)
                    else next(
                        (item["value"]["value"] for item in context if item["name"] == "reason"),
                        None,
                    )
                )
                requirement = {
                    "Field required": "REQ-0285",
                    "Extra inputs are not permitted": "REQ-0285",
                    "Value error, an expression must contain exactly one operation": "REQ-0286",
                }.get(reason, "REQ-0287")
            if requirement is not None:
                value["requirement"] = requirement

    visit(expected)
    return expected
