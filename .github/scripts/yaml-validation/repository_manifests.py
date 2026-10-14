"""Repository manifest and authored diagnostic contract checks."""
import re
from pathlib import Path
import yaml
from _repository_yaml import UniqueKeyLoader
from functools import lru_cache
import _native_schema
from editorial import example_spec_paths
from validation_diagnostics import RESOURCE_PATH_MESSAGES, VALIDATION_CONTEXT_FIELDS, ValidationDiagnostic


VALIDATION_CONDITION_REGISTRY = {
    key: {
        'allowed_phases': {'validation'},
        'required_context': required_context,
    }
    for key, required_context in VALIDATION_CONTEXT_FIELDS.items()
}


def load_validation_manifest(root: Path):
    path = root / 'benchmarks' / 'validation-manifest.yaml'
    if not path.is_file():
        if not validation_phase_contracts(root):
            return {'version': '1.0', 'fixtures': {}}, []
        return None, [
            f"ERROR: {path.relative_to(root)}: validation manifest is missing"
        ]
    try:
        with open(path, 'r', encoding='utf-8') as handle:
            manifest = yaml.load(handle, Loader=UniqueKeyLoader)
    except (OSError, UnicodeError, yaml.YAMLError) as exc:
        return None, [f"ERROR: {path.relative_to(root)}: {exc}"]
    return manifest, []


def validation_phase_contracts(root: Path):
    contracts = {}
    examples_dir = root / 'benchmarks'
    if not examples_dir.is_dir():
        return contracts
    for example_dir in sorted(examples_dir.glob('negative-*')):
        error_path = example_dir / 'expected' / 'error.yaml'
        if not error_path.is_file():
            continue
        try:
            with open(error_path, 'r', encoding='utf-8') as handle:
                contract = yaml.load(handle, Loader=UniqueKeyLoader)
        except (OSError, UnicodeError, yaml.YAMLError):
            continue
        if (
            isinstance(contract, dict)
            and contract.get('phase') == 'validation'
        ):
            contracts[example_dir.name] = contract
    return contracts


def validate_validation_manifest(root: Path, manifest):
    errors = []
    label = 'benchmarks/validation-manifest.yaml'
    if not isinstance(manifest, dict):
        return [f"ERROR: {label}: expected a mapping"]
    if manifest.get('version') != '1.0':
        errors.append(f"ERROR: {label}.version: expected '1.0'")
    unknown_root = sorted(set(manifest) - {'version', 'fixtures'})
    for field in unknown_root:
        errors.append(f"ERROR: {label}.{field}: unknown field")
    fixtures = manifest.get('fixtures')
    if not isinstance(fixtures, dict):
        errors.append(f"ERROR: {label}.fixtures: expected a mapping")
        return errors

    contracts = validation_phase_contracts(root)
    missing = sorted(set(contracts) - set(fixtures))
    stale = sorted(set(fixtures) - set(contracts))
    for name in missing:
        errors.append(
            f"ERROR: {label}.fixtures: missing validation fixture {name!r}"
        )
    for name in stale:
        errors.append(
            f"ERROR: {label}.fixtures.{name}: stale or non-validation fixture"
        )

    for name in sorted(set(fixtures) & set(contracts)):
        entry = fixtures[name]
        path = f"{label}.fixtures.{name}"
        if not isinstance(entry, dict):
            errors.append(f"ERROR: {path}: expected a mapping")
            continue
        unknown = sorted(
            set(entry)
            - {'rule', 'condition', 'spec_paths', 'validator', 'blocked_by'}
        )
        for field in unknown:
            errors.append(f"ERROR: {path}.{field}: unknown field")
        rule = entry.get('rule')
        condition = entry.get('condition')
        spec_paths = entry.get('spec_paths')
        contract = contracts[name]
        if not isinstance(rule, str) or re.fullmatch(r'R[0-9]{3}', rule) is None:
            errors.append(f"ERROR: {path}.rule: expected a rule id")
        registration = VALIDATION_CONDITION_REGISTRY.get((rule, condition))
        if registration is None:
            errors.append(
                f"ERROR: {path}.condition: unregistered condition "
                f"{condition!r} for {rule!r}"
            )
        elif contract.get('phase') not in registration['allowed_phases']:
            errors.append(
                f"ERROR: {path}.condition: condition {condition!r} is not "
                f"allowed during phase {contract.get('phase')!r}"
            )
        if not (
            isinstance(spec_paths, list)
            and spec_paths
            and len(spec_paths) == len(set(spec_paths))
            and all(isinstance(item, str) and item for item in spec_paths)
        ):
            errors.append(
                f"ERROR: {path}.spec_paths: expected unique non-empty paths"
            )
        if condition != contract.get('condition'):
            errors.append(
                f"ERROR: {path}.condition: does not match expected/error.yaml"
            )
        if spec_paths != contract.get('spec_paths'):
            errors.append(
                f"ERROR: {path}.spec_paths: do not match expected/error.yaml"
            )
        required_context = (
            registration['required_context']
            if registration is not None
            else set()
        )
        context = contract.get('context', {})
        if not isinstance(context, dict):
            context = {}
        absent = sorted(required_context - set(context))
        if absent:
            errors.append(
                f"ERROR: {path}: expected/error.yaml context is missing "
                f"{absent!r}"
            )

        has_validator = 'validator' in entry
        has_blocker = 'blocked_by' in entry
        if has_validator == has_blocker:
            errors.append(
                f"ERROR: {path}: declare exactly one of validator or "
                'blocked_by'
            )
        if has_validator and not (
            isinstance(entry['validator'], str)
            and re.fullmatch(
                r'[a-z][a-z0-9]*(?:_[a-z0-9]+)*', entry['validator']
            )
        ):
            errors.append(f"ERROR: {path}.validator: expected snake case")
        if has_blocker and not (
            isinstance(entry['blocked_by'], str)
            and re.fullmatch(r'#[1-9][0-9]*', entry['blocked_by'])
        ):
            errors.append(
                f"ERROR: {path}.blocked_by: expected a GitHub issue '#N'"
            )
    return errors


def diagnostic_matches_path(diagnostic, spec_labels, expected_path):
    actual = diagnostic.path
    for spec_label in spec_labels:
        prefix = f"{spec_label}."
        if actual.startswith(prefix):
            actual = actual[len(prefix):]
            break
    return (
        actual == expected_path
        or actual.startswith(expected_path + '.')
        or actual.startswith(expected_path + '[')
    )


def validate_registered_fixture_diagnostics(
    name, entry, spec_errors, spec_labels
):
    expected_condition = entry['condition']
    expected_paths = entry['spec_paths']
    matches = {path: [] for path in expected_paths}
    expected_ids = set()
    for diagnostic in spec_errors:
        if not isinstance(diagnostic, ValidationDiagnostic):
            continue
        if diagnostic.condition != expected_condition:
            continue
        for expected_path in expected_paths:
            if diagnostic_matches_path(
                diagnostic, spec_labels, expected_path
            ):
                matches[expected_path].append(diagnostic)
                expected_ids.add(id(diagnostic))

    manifest_path = f"benchmarks/validation-manifest.yaml.fixtures.{name}"
    if 'blocked_by' in entry:
        if all(matches.values()):
            return [
                f"ERROR: {manifest_path}.blocked_by: stale block; "
                f"{expected_condition!r} is emitted at every declared path"
            ]
        return [
            error for error in spec_errors
            if (
                not isinstance(error, ValidationDiagnostic)
                or id(error) not in expected_ids
            )
        ]

    errors = []
    for expected_path, diagnostics in matches.items():
        if not diagnostics:
            errors.append(
                f"ERROR: {manifest_path}.spec_paths: condition "
                f"{expected_condition!r} was not emitted at "
                f"{expected_path!r}"
            )
    errors.extend(
        error for error in spec_errors
        if (
            not isinstance(error, ValidationDiagnostic)
            or id(error) not in expected_ids
        )
    )
    return errors


def check_declared_validation_error(
    err_spec, matched_errors, spec_label, label,
):
    """Require a declared condition this validator decides to be reported.

    A negative example may declare a condition the validator does not yet
    decide; its non-goals list which. A condition the validator does decide
    must actually be the one it reports, so a fixture cannot go on passing
    once it stops failing the way it claims to.
    """
    condition = err_spec.get('condition')
    if condition not in RESOURCE_PATH_MESSAGES:
        return []
    reported = {message.split(': ', 1)[0] for message in matched_errors}
    if condition in reported:
        return []
    return [
        f"ERROR: {label}.condition: {condition!r} was not reported for "
        f"{spec_label}; got {sorted(reported)!r}"
    ]


EXPECTED_ERROR_PHASES = {
    'parse',
    'planning',
    'engine',
    'validation',
    'ingest',
    'row_construction',
    'derivation',
    'output',
    'verification',
    'bind',
    'join',
    'mapping',
    'cut',
    'extract',
    'template',
    'impute',
    'convert',
    'final',
}


def load_condition_registry(root: Path):
    relative_path = Path('yaml/conditions.yaml')
    path = root / relative_path
    if not path.is_file():
        examples_dir = root / 'benchmarks'
        if not examples_dir.is_dir() or not any(
            examples_dir.glob('negative-*/expected/error.yaml')
        ):
            return {'version': '1.0', 'conditions': {}}, []
        return None, [f"ERROR: {relative_path}: condition registry is missing"]
    try:
        with open(path, 'r', encoding='utf-8') as handle:
            registry = yaml.load(handle, Loader=UniqueKeyLoader)
    except (OSError, UnicodeError, yaml.YAMLError) as exc:
        return None, [f"ERROR: {relative_path}: {exc}"]
    return registry, []


def validate_condition_registry(root: Path, registry):
    errors = []
    label = 'yaml/conditions.yaml'
    if not isinstance(registry, dict):
        return [f"ERROR: {label}: expected a mapping"]
    if registry.get('version') != '1.0':
        errors.append(f"ERROR: {label}.version: expected '1.0'")
    for field in sorted(set(registry) - {'version', 'conditions'}):
        errors.append(f"ERROR: {label}.{field}: unknown field")
    conditions = registry.get('conditions')
    if not isinstance(conditions, dict):
        errors.append(f"ERROR: {label}.conditions: expected a mapping")
        return errors
    condition_names = list(conditions)
    if (
        all(isinstance(condition, str) for condition in condition_names)
        and condition_names != sorted(condition_names)
    ):
        errors.append(f"ERROR: {label}.conditions: expected sorted keys")

    for condition, registration in conditions.items():
        path = f"{label}.conditions.{condition}"
        if not (
            isinstance(condition, str)
            and re.fullmatch(
                r'[a-z][a-z0-9]*(?:_[a-z0-9]+)*', condition
            )
        ):
            errors.append(f"ERROR: {path}: expected a snake-case name")
        if not isinstance(registration, dict):
            errors.append(f"ERROR: {path}: expected a mapping")
            continue
        for field in sorted(set(registration) - {'phases'}):
            errors.append(f"ERROR: {path}.{field}: unknown field")
        phases = registration.get('phases')
        if not (
            isinstance(phases, list)
            and phases
            and all(
                isinstance(phase, str) and phase in EXPECTED_ERROR_PHASES
                for phase in phases
            )
            and phases == sorted(set(phases))
        ):
            errors.append(
                f"ERROR: {path}.phases: expected unique sorted phases"
            )

    examples_dir = root / 'benchmarks'
    if not examples_dir.is_dir():
        return errors
    for error_path in sorted(
        examples_dir.glob('negative-*/expected/error.yaml')
    ):
        try:
            with open(error_path, 'r', encoding='utf-8') as handle:
                contract = yaml.load(handle, Loader=UniqueKeyLoader)
        except (OSError, UnicodeError, yaml.YAMLError):
            continue
        if not isinstance(contract, dict):
            continue
        condition = contract.get('condition')
        phase = contract.get('phase')
        if not isinstance(condition, str) or not isinstance(phase, str):
            continue
        fixture_label = error_path.relative_to(root)
        registration = conditions.get(condition)
        if registration is None:
            errors.append(
                f"ERROR: {fixture_label}.condition: unregistered condition "
                f"{condition!r}"
            )
            continue
        if (
            isinstance(registration, dict)
            and isinstance(registration.get('phases'), list)
            and phase not in registration['phases']
        ):
            errors.append(
                f"ERROR: {fixture_label}.phase: condition {condition!r} is "
                f"not registered for phase {phase!r}"
            )
    return errors




def spec_path_exists(spec, path):
    node = spec
    previous = None
    parts = path.split('.')
    for position, part in enumerate(parts):
        if isinstance(node, str) and (previous == 'derivation' or position == 2 and parts[0] == 'input'):
            selected = 'derivation' if previous == 'derivation' else 'dataset_source'
            schema = _contract_schema()
            findings = schema.validate(_native_schema.bytes_for(node), [selected], True)
            if findings:
                # A native path refusal retains the authored source operand in
                # its diagnostic context. This proves that leaf's occurrence,
                # while exposing no normalized document after failed admission.
                return parts[position:] == ['path'] and any(
                    json.loads(context).get('path') == node
                    for _phase, _condition, _requirement, _paths, context in findings)
            node = _native_schema.decode_document(schema.normalize(
                _native_schema.bytes_for(node), [selected], True))
            if previous == 'derivation':
                node = node['value']
        match = re.fullmatch(r'([A-Za-z_][A-Za-z0-9_]*)(?:\[([0-9]+)\])?', part)
        if match is None:
            return False
        name, index_text = match.groups()
        if isinstance(node, dict):
            if name not in node:
                return False
            node = node[name]
        elif isinstance(node, list):
            candidates = [
                item for item in node
                if (
                    isinstance(item, dict)
                    and (item.get('name') == name or item.get('id') == name)
                )
            ]
            if len(candidates) != 1:
                return False
            node = candidates[0]
        else:
            return False
        if index_text is not None:
            if not isinstance(node, list):
                return False
            index = int(index_text)
            if index >= len(node):
                return False
            node = node[index]
        previous = name
    return True


def validate_expected_error_contracts(root: Path):
    errors = []
    examples_dir = root / 'benchmarks'
    if not examples_dir.exists():
        return errors
    for ex_dir in sorted(examples_dir.glob('negative-*')):
        if not ex_dir.is_dir():
            continue
        error_path = ex_dir / 'expected' / 'error.yaml'
        if not error_path.exists():
            continue
        label = error_path.relative_to(root)
        try:
            with open(error_path, 'r', encoding='utf-8') as f:
                contract = yaml.load(f, Loader=UniqueKeyLoader)
        except Exception:
            continue
        if not isinstance(contract, dict):
            errors.append(f"ERROR: {label}: expected a mapping")
            continue
        unknown = sorted(
            set(contract) - {'phase', 'condition', 'spec_paths', 'requirement', 'context'}
        )
        for field in unknown:
            errors.append(f"ERROR: {label}.{field}: unknown field")
        if contract.get('phase') not in EXPECTED_ERROR_PHASES:
            errors.append(
                f"ERROR: {label}.phase: expected one of "
                f"{sorted(EXPECTED_ERROR_PHASES)!r}"
            )
        condition = contract.get('condition')
        if not (
            isinstance(condition, str)
            and re.fullmatch(r'[a-z][a-z0-9]*(?:_[a-z0-9]+)*', condition)
        ):
            errors.append(
                f"ERROR: {label}.condition: expected a snake-case name"
            )
        paths = contract.get('spec_paths')
        if not (
            isinstance(paths, list)
            and paths
            and all(isinstance(path, str) and path for path in paths)
        ):
            errors.append(
                f"ERROR: {label}.spec_paths: expected a non-empty string list"
            )
            paths = []
        if 'context' in contract and not isinstance(contract['context'], dict):
            errors.append(f"ERROR: {label}.context: expected a mapping")

        for spec_path in example_spec_paths(ex_dir):
            try:
                with open(spec_path, 'r', encoding='utf-8') as f:
                    spec = yaml.load(f, Loader=UniqueKeyLoader)
            except Exception:
                continue
            if not isinstance(spec, dict):
                continue
            for path in paths:
                if condition in {
                    'missing_required_field',
                    'missing_value_required',
                    'month_required',
                }:
                    # The diagnostic points at the field that should exist.
                    continue
                if not spec_path_exists(spec, path):
                    errors.append(
                        f"ERROR: {label}.spec_paths: {path!r} does not exist "
                        f"in {spec_path.name}"
                    )
    return errors


@lru_cache(maxsize=1)
def _contract_schema():
    return _native_schema.capture(Path(__file__).resolve().parents[3] / "yaml")
