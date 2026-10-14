"""Installed native schema, inheritance and declaration facts."""

from pathlib import Path

import _native_csv
import _native_json
import _native_schema
import _native_specification
from _repository_yaml import UniqueKeyLoader
from editorial import validate_column_labels
from validation_diagnostics import RESOURCE_PATH_MESSAGES, validation_diagnostic

import yaml


def native_diagnostics(rows, label):
    errors = []
    for phase, condition, requirement, paths, encoded in rows:
        context = _native_json.loads(encoded)
        message = condition + ": " + encoded
        if condition == 'missing_required_field':
            message = f"missing required field {context['field']!r}"
        elif condition == 'unknown_field' and 'class' in context:
            message = f"unknown field {context['field']!r} for class {context['class']}"
        elif condition == 'invalid_field_type' and {'expected', 'actual'} <= context.keys():
            message = f"expected {context['expected']}, got {context['actual']}"
        elif condition == 'value_not_permitted' and {'value', 'permitted'} <= context.keys():
            message = f"value {context['value']!r} is outside allowed values {context['permitted']!r}"
        elif condition == 'invalid_size':
            message = f"expected size {context['size']}"
        elif condition == 'invalid_operation_count':
            message = 'expected exactly one operation: ' + encoded
        for path in paths or [""]:
            location = label + ("." + path if path else "")
            finding = validation_diagnostic(
                location, condition, message, context=context
            )
            finding.phase, finding.requirement = phase, requirement
            errors.append(finding)
    return errors


def build_schema_env(root: Path, entrypoint="schema.yaml"):
    try:
        native = _native_schema.capture(root / "yaml", entrypoint)
    except (OSError, UnicodeError, yaml.YAMLError, ValueError) as error:
        return None, _native_schema.schema_errors(
            error, str(root / "yaml" / entrypoint)
        )
    errors = native_diagnostics(native.defaults(), entrypoint)
    return {
        "native": native,
        "root": str(root.resolve()),
        "entrypoint": entrypoint,
        "version": native.version(),
    }, errors


def validate_type(data, t_refs, env, path, fragment=False):
    types = t_refs if isinstance(t_refs, list) else [t_refs]
    native = _native_schema.fixture(env)
    return native_diagnostics(
        native.validate(_native_schema.bytes_for(data), types, fragment), path
    )


def normalize_single_type_value(data, type_ref, env, fragment=False):
    native = _native_schema.fixture(env)
    return _native_schema.decode_document(
        native.normalize(_native_schema.bytes_for(data), [type_ref], fragment)
    )


def schema_class_fields(env, class_name):
    return {name: {} for name in _native_schema.fixture(env).fields(class_name)}


def check_descriptor(desc, is_class_field, path):
    authored = (
        {"version": "1.0", "root_class": [{"value": desc}]}
        if is_class_field
        else {"version": "1.0", "root_class": [], "authored_alias": desc}
    )
    try:
        _native_schema._capture_schema_metadata(
            [("schema.yaml", _native_schema.bytes_for(authored))], 0, "root_class"
        )
    except ValueError as error:
        if isinstance(desc.get('pattern'), str):
            from regex_checks import regex_pattern_errors
            pattern_errors = regex_pattern_errors(desc['pattern'], path)
            if pattern_errors:
                return pattern_errors
        return _native_schema.schema_errors(error, path)
    return []


def validate_schemas(root: Path):
    _, errors = build_schema_env(root)
    _, environment_errors = build_schema_env(root, "schema_environment.yaml")
    return errors + environment_errors


def valid_temporal_literal(kind, text):
    """Tagged R018 defaults/call arguments use the shared typed source admission."""
    if kind not in {"date", "datetime"} or not isinstance(text, str):
        return False
    try:
        _native_csv.scalar_text(text, kind)
    except ValueError:
        return False
    return True


def iter_function_calls(value, path):
    """Yield function payloads and stable paths from a derivation tree."""
    if isinstance(value, dict):
        if len(value) == 1 and "function" in value:
            yield value["function"], f"{path}.function"
            return
        for key, child in value.items():
            yield from iter_function_calls(child, f"{path}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            yield from iter_function_calls(child, f"{path}[{index}]")


def validate_spec_functions(spec, spec_label, spec_path, schema_env, sources=None):
    """Validate calls against R018 when an implementation is supplied."""
    # This bridge validates logical calls, not ordinary domain execution or
    # submission generation. Independent schema and metadata validators still
    # inspect a specification without calls. Inspect the complete nested set:
    # intermediate calls require native validation even when no final column
    # calls a function directly.
    if not any(iter_function_calls(spec, spec_label)):
        return []
    environment_paths = project_environment_paths(spec_path)
    if not environment_paths:
        # A portable specification can declare logical calls before a project
        # supplies their implementation. R018 requires this environment when
        # project code is validated, activated, or executed.
        return []
    # Every root is held to the same calls, so a call that fails against two
    # of them reports one finding rather than one per root; a finding only
    # one root produces still stands on its own.
    return list(
        dict.fromkeys(
            error
            for environment_path in environment_paths
            for error in validate_spec_functions_against(
                spec,
                spec_label,
                spec_path,
                schema_env,
                [],
                {},
                environment_path,
                sources,
            )
        )
    )


def project_environment_paths(spec_path):
    """Return every project root a benchmark offers for one specification.

    A project keeps `environment.yaml` beside the specification it
    implements. A benchmark demonstrating one logical contract in more than
    one runtime language keeps a root per language beside it as
    `<language>/`, because one environment declares one language. Every root
    present must provide the contracts the specification calls.
    """
    beside = spec_path.parent / "environment.yaml"
    found = [beside] if beside.exists() else []
    for language in ("python", "r"):
        candidate = spec_path.parent / language / "environment.yaml"
        if candidate.exists():
            found.append(candidate)
    return found


def validate_spec_functions_against(
    spec,
    spec_label,
    spec_path,
    schema_env,
    calls,
    column_types,
    environment_path,
    sources=None,
):
    """Shared Rust static admission; no installed function or study authority."""
    try:
        source = yaml.load(
            environment_path.read_text(encoding="utf-8"), Loader=UniqueKeyLoader
        )
    except (OSError, UnicodeError, yaml.YAMLError) as exc:
        return [f"ERROR: {environment_path}: {exc}"]
    if not isinstance(source, dict):
        return [f"ERROR: {environment_path}: expected a mapping"]
    rows = _native_specification.validate(
        spec,
        spec_path,
        environments=[environment_path],
        schema_root=Path(schema_env.get("root", _native_specification.ROOT)),
    )
    errors = []
    for phase, condition, requirement, paths, encoded in rows:
        facts = _native_json.loads(encoded) if encoded else {}
        for path in paths or [None]:
            finding = validation_diagnostic(
                f"{spec_label}.{path}" if path is not None else str(environment_path),
                condition,
                condition,
                context=facts,
            )
            finding.phase = phase
            finding.requirement = requirement
            errors.append(finding)
    return errors


def prepare_spec_document(spec, spec_label, spec_path, env):
    """Read held shared-engine structural facts without executing the project."""
    metadata, provenance = {}, {}
    rows = _native_specification.validate(
        spec,
        spec_path,
        schema_root=Path(env.get("root", _native_specification.ROOT)),
        origins=provenance,
        metadata=metadata,
        document_only=True,
    )
    if "document" not in metadata:
        return None, native_diagnostics(rows, spec_label), provenance
    return metadata["document"], [], provenance


def resolve_spec_inheritance(entry_spec, spec_label, spec_path, env):
    return prepare_spec_document(entry_spec, spec_label, spec_path, env)


def validate_spec_document(
    spec,
    spec_label,
    spec_path,
    env,
    spec_stack=None,
    project_root=None,
    snapshots=None,
):
    """Native static/profile/header facts, without project execution or Save."""
    if not isinstance(spec, dict) or not spec:
        return [f"ERROR: {spec_label}: spec is empty or not a mapping"]
    origins = {}
    rows = _native_specification.validate(
        spec,
        spec_path,
        project_root=project_root,
        origins=origins,
        environments=project_environment_paths(spec_path),
        schema_root=Path(env.get("root", _native_specification.ROOT)),
    )
    errors = []
    for phase, condition, requirement, paths, encoded in rows:
        if phase != "validation":
            # Ingest fixtures are independently proved by validate_csv_shapes.
            continue
        context = _native_json.loads(encoded)
        if condition in RESOURCE_PATH_MESSAGES and isinstance(context.get("path"), str):
            message = (
                condition
                + ": "
                + repr(context["path"])
                + " "
                + RESOURCE_PATH_MESSAGES[condition]
            )
        elif condition == "schema_version_mismatch":
            message = "schema_version does not match bundle version: " + encoded
        elif condition == "missing_derivation":
            message = "column has no derivation: " + encoded
        elif condition == "driver_unavailable":
            message = "base is required: " + encoded
        else:
            message = condition + ": " + encoded
        sources = context.get("declaring_sources") or []
        sources = [sources] if isinstance(sources, str) else list(sources)
        sources.extend(
            [
                context[key]
                for key in ("source_identity", "source")
                if isinstance(context.get(key), str)
            ]
        )
        contributed = next(
            (source for source in sources if source != str(spec_path.resolve())), None
        )
        if contributed is not None:
            message += f" (contributed by {contributed})"
        for path in paths or [""]:
            location = spec_label + ("." + path if path else "")
            attributed = origins.get(path)
            displayed = message
            if attributed is not None and attributed != str(spec_path.resolve()):
                displayed += f" (contributed by {attributed})"
            finding = validation_diagnostic(
                location, condition, displayed, context=context
            )
            finding.phase = phase
            finding.requirement = requirement
            errors.append(finding)
    # Approved repository authoring policy; cross-field names are native facts.
    authoring = spec
    if "parents" in spec:
        # This existing authoring view is used only for editorial label policy;
        # native diagnostics above own all semantic admission and binding.
        authoring, _, _ = prepare_spec_document(spec, spec_label, spec_path, env)
    if isinstance(authoring, dict):
        errors.extend(validate_column_labels(authoring, spec_label))
    return errors
