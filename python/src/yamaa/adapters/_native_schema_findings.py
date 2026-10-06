"""Render structured Rust admission findings; this module does not admit schemas."""

from yamaa.specification.schema import _schema_failure

_DESCRIPTOR_MESSAGES = {
    "expected_mapping": "descriptor must be a mapping",
    "missing_type": "missing 'type'",
    "invalid_type_value": "type must be a string or non-empty list of strings",
    "required_boolean": "required must be a boolean",
    "required_default": "a required field cannot declare a default",
    "description": "description must be a non-empty string",
    "pattern_requires_string": "pattern is allowed only for type str",
    "pattern_text": "pattern must be a string",
    "minimum_requires_string": "min_length is allowed only for type str",
    "minimum_nonnegative": "min_length must be a non-negative integer",
    "size_requires_collection": "size is allowed only for list or dict",
    "size_nonnegative": "size must be a non-negative integer",
    "values_requires_string": "values is allowed only for type str",
    "values_text_sequence": "values must be a list of strings",
}
_MESSAGES = {
    "expected_mapping": "schema document must be a mapping",
    "version": "version must be a non-empty string",
    "includes_list": "includes must be a list",
    "declaration_name": "declaration names must be strings",
    "field_entry": "class fields must be one-entry mappings",
    "field_name": "field name must be a non-empty string",
    "duplicate_field": "duplicate class field",
    "registry_only": "registry-backed type must contain only 'registry'",
    "registry_name": "registry name must be a non-empty string",
    "empty_registry": "registry is empty",
    "unreferenced_registry": "registry is unreferenced",
    "registry_entry_name": "entry name must be a non-empty string",
    "registry_entry_shape": "entry must be a class or descriptor",
    "fields_from": "fields_from must name one class",
}


def _reason(finding, modules, module_names):
    path, issue = finding["path"], finding["issue"]
    code = issue["code"]
    if code == "descriptor":
        issue = issue["issue"]
        code = issue["code"]
        if code == "unknown_keyword":
            key = modules[finding["module"]][issue["key"]]
            reason = f"invalid descriptor keyword {key!r}"
        elif code == "type_syntax":
            return f"{path}.type: invalid type expression {issue['member']!r}"
        elif code == "invalid_pattern":
            reason = f"invalid pattern {issue['pattern']!r}: {issue['reason']}"
        else:
            reason = _DESCRIPTOR_MESSAGES[code]
        return f"{path}: {reason}"
    if code in _MESSAGES:
        reason = _MESSAGES[code]
        return reason if path == "$" else f"{path}: {reason}"
    if code == "version_mismatch":
        return f"version {issue['actual']!r} does not match {issue['expected']!r}"
    if code == "unsafe_include":
        return f"unsafe include {modules[finding['module']][issue['node']]!r}"
    if code == "missing_include":
        return f"invalid include {issue['name']!r}"
    if code == "include_cycle":
        return f"schema include cycle at {module_names[finding['module']]}"
    if code == "missing_root":
        return f"{issue['name']} is not declared"
    if code in ("unknown_declaration", "duplicate_declaration"):
        return f"{code.replace('_', ' ')} {issue['name']!r}"
    if code == "duplicate_registry_entry":
        return f"duplicate registry entry {issue['name']}"
    if code == "unknown_fields_from":
        return f"unknown fields_from class {issue['name']!r}"
    if code == "fields_from_cycle":
        return f"class fields_from cycle: {' -> '.join(issue['names'])}"
    if code == "unknown_registry":
        return f"registry {issue['name']!r} is not declared"
    if code == "unknown_type":
        return f"unknown schema type {issue['name']!r}"
    # Module identity defects cannot arise from this closed filesystem loader.
    # Preserve the shared finding if a future transport adds another category.
    return f"{path}: {code}"


def admission_error(outcome, entrypoint, modules, module_names):
    """Retain the shared finding alongside the application's schema error envelope."""
    if outcome["status"] == "invalid_schema":
        reason = "; ".join(_reason(f, modules, module_names) for f in outcome["issues"])
    elif outcome["status"] == "invalid_defaults":
        descriptors = outcome["schema"]["descriptors"]
        reasons = []
        for failure in outcome["defaults"]:
            path = descriptors[failure["descriptor"]]["path"]
            conditions = ", ".join(d["condition"] for d in failure["diagnostics"])
            reasons.append(f"{path}.default: invalid default ({conditions})")
        reason = "; ".join(reasons)
    else:
        raise ValueError("invalid native schema admission response")
    error = _schema_failure(entrypoint, reason)
    error.native_outcome = outcome
    return error
