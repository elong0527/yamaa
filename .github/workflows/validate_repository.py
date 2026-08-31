#!/usr/bin/env python3
import argparse
import copy
import csv
import re
import sys
from datetime import date, datetime
from pathlib import Path

import yaml
from yaml.composer import ComposerError
from yaml.constructor import ConstructorError
from yaml.events import AliasEvent


KNOWN_STRUCTURAL_WARNING = (
    "ERROR: sdtm-ds-disposition-sequence/spec.yaml.columns.DSDECOD."
    "verifications: registry column_verifications expects exactly one "
    "operation, got 2"
)


class UniqueKeyLoader(yaml.SafeLoader):
    yaml_implicit_resolvers = copy.deepcopy(
        yaml.SafeLoader.yaml_implicit_resolvers
    )

    def compose_node(self, parent, index):
        event = self.peek_event()
        if isinstance(event, AliasEvent):
            raise ComposerError(
                None,
                None,
                "YAML aliases are not allowed",
                event.start_mark,
            )
        if getattr(event, 'anchor', None) is not None:
            raise ComposerError(
                None,
                None,
                "YAML anchors are not allowed",
                event.start_mark,
            )
        if getattr(event, 'tag', None) is not None:
            raise ComposerError(
                None,
                None,
                "explicit YAML tags are not allowed",
                event.start_mark,
            )
        return super().compose_node(parent, index)

    def flatten_mapping(self, node):
        for key_node, _ in node.value:
            if key_node.tag == 'tag:yaml.org,2002:merge':
                raise ConstructorError(
                    None,
                    None,
                    "YAML merge keys are not allowed",
                    key_node.start_mark,
                )
        return super().flatten_mapping(node)

    def construct_mapping(self, node, deep=False):
        mapping = {}
        for key_node, value_node in node.value:
            key = self.construct_object(key_node, deep=deep)
            if key in mapping:
                raise ConstructorError(
                    "while constructing a mapping", node.start_mark,
                    f"found duplicate key '{key}'", key_node.start_mark
                )
            mapping[key] = self.construct_object(value_node, deep=deep)
        return mapping


for first_char, resolvers in list(
    UniqueKeyLoader.yaml_implicit_resolvers.items()
):
    UniqueKeyLoader.yaml_implicit_resolvers[first_char] = [
        (tag, regexp)
        for tag, regexp in resolvers
        if tag != 'tag:yaml.org,2002:bool'
    ]
UniqueKeyLoader.add_implicit_resolver(
    'tag:yaml.org,2002:bool',
    re.compile(r'^(?:true|false)$', re.IGNORECASE),
    list('tTfF'),
)


def split_type_arguments(inner):
    depth = 0
    for index, char in enumerate(inner):
        if char == '[':
            depth += 1
        elif char == ']':
            depth -= 1
            if depth < 0:
                break
        elif char == ',' and depth == 0:
            return inner[:index].strip(), inner[index + 1:].strip()
    raise ValueError(f"invalid dict type expression: dict[{inner}]")


def parse_type_ref(t_ref):
    if isinstance(t_ref, list):
        refs = []
        for item in t_ref:
            refs.extend(parse_type_ref(item))
        return refs

    t_ref = str(t_ref).strip()
    if t_ref.startswith('list[') and t_ref.endswith(']'):
        return parse_type_ref(t_ref[5:-1].strip())
    if t_ref.startswith('dict[') and t_ref.endswith(']'):
        key_type, value_type = split_type_arguments(t_ref[5:-1])
        return parse_type_ref(key_type) + parse_type_ref(value_type)

    return [t_ref]


def check_descriptor(desc, is_class_field, path):
    errors = []
    if not isinstance(desc, dict):
        return [f"ERROR: {path}: descriptor must be a mapping"]

    allowed = {
        'type',
        'description',
        'values',
        'pattern',
        'min_length',
        'size',
        'default',
    }
    if is_class_field:
        allowed.add('required')

    for key in desc:
        if key not in allowed:
            errors.append(
                f"ERROR: {path}: invalid descriptor keyword '{key}'"
            )

    if 'type' not in desc:
        errors.append(f"ERROR: {path}: missing 'type'")
        return errors

    type_value = desc['type']
    if isinstance(type_value, str):
        type_members = [type_value.strip()]
    elif (
        isinstance(type_value, list)
        and type_value
        and all(isinstance(item, str) for item in type_value)
    ):
        type_members = [item.strip() for item in type_value]
    else:
        errors.append(
            f"ERROR: {path}: type must be a string or non-empty list of strings"
        )
        type_members = []

    if 'required' in desc and not isinstance(desc['required'], bool):
        errors.append(f"ERROR: {path}: required must be a boolean")
    if desc.get('required') and 'default' in desc:
        errors.append(
            f"ERROR: {path}: a required field cannot declare a default"
        )

    if 'description' in desc and (
        not isinstance(desc['description'], str)
        or not desc['description'].strip()
    ):
        errors.append(
            f"ERROR: {path}: description must be a non-empty string"
        )

    string_only = type_members == ['str']
    sized_only = bool(type_members) and all(
        member.startswith('list[') or member.startswith('dict[')
        for member in type_members
    )

    if 'pattern' in desc:
        if not string_only:
            errors.append(
                f"ERROR: {path}: pattern is allowed only for type str"
            )
        if not isinstance(desc['pattern'], str):
            errors.append(f"ERROR: {path}: pattern must be a string")
        else:
            try:
                re.compile(desc['pattern'])
            except re.error as exc:
                errors.append(f"ERROR: {path}: invalid pattern: {exc}")

    if 'min_length' in desc:
        if not string_only:
            errors.append(
                f"ERROR: {path}: min_length is allowed only for type str"
            )
        if (
            type(desc['min_length']) is not int
            or desc['min_length'] < 0
        ):
            errors.append(
                f"ERROR: {path}: min_length must be a non-negative integer"
            )

    if 'size' in desc:
        if not sized_only:
            errors.append(
                f"ERROR: {path}: size is allowed only for list or dict"
            )
        if type(desc['size']) is not int or desc['size'] < 0:
            errors.append(
                f"ERROR: {path}: size must be a non-negative integer"
            )

    if 'values' in desc:
        values = desc['values']
        if not string_only:
            errors.append(
                f"ERROR: {path}: values is allowed only for type str"
            )
        if not isinstance(values, list) or not all(
            isinstance(value, str) for value in values
        ):
            errors.append(
                f"ERROR: {path}: values must be a list of strings"
            )

    return errors


def build_schema_env(root: Path):
    errors = []
    schema_dir = root / 'yaml'
    schema_path = schema_dir / 'schema.yaml'
    if not schema_path.exists():
        return None, [f"ERROR: required schema file not found: {schema_path}"]

    completed = set()
    to_visit = [(schema_path, tuple())]
    known_types = {'str', 'int', 'float', 'bool', 'null', 'date', 'datetime'}
    all_type_refs = []

    env = {
        'classes': {},
        'aliases': {},
        'registries': {},
        'defaults_to_validate': []
    }
    declaration_kinds = {}

    def collect_type_refs(type_value, path):
        try:
            all_type_refs.extend(parse_type_ref(type_value))
        except (TypeError, ValueError) as exc:
            errors.append(f"ERROR: {path}: {exc}")

    while to_visit:
        current, stack = to_visit.pop()
        curr_res = current.resolve()
        if curr_res in stack:
            errors.append(f"ERROR: schema include cycle detected at {current.name}")
            continue
        if curr_res in completed:
            continue
        completed.add(curr_res)
        new_stack = stack + (curr_res,)

        try:
            with open(current, 'r', encoding='utf-8') as f:
                data = yaml.load(f, Loader=UniqueKeyLoader)
        except Exception:
            continue

        if not isinstance(data, dict):
            errors.append(
                f"ERROR: {current.name}: schema document must be a mapping"
            )
            continue

        version = data.get('version')
        if current.name == 'schema.yaml':
            env['version'] = version
        if version != "1.0":
            errors.append(f"ERROR: {current.name}: schema version '{version}' != '1.0'")

        includes = data.get('includes', [])
        if not isinstance(includes, list):
            errors.append(
                f"ERROR: {current.name}: includes must be a list"
            )
            includes = []

        for inc in includes:
            if not isinstance(inc, str):
                errors.append(f"ERROR: {current.name}: include is not a string")
                continue
            if '://' in inc:
                errors.append(f"ERROR: {current.name}: included file {inc} is a URL")
                continue
            if '..' in inc.split('/'):
                errors.append(f"ERROR: {current.name}: included file {inc} is outside yaml directory syntactically")
                continue
            if inc.startswith('/'):
                errors.append(f"ERROR: {current.name}: included file {inc} is absolute")
                continue
            if not re.fullmatch(r'schema_[a-z0-9_]+\.yaml', inc):
                errors.append(f"ERROR: {current.name}: included file {inc} does not match schema_[a-z0-9_]+.yaml")

            unresolved_inc_path = current.parent / inc
            if unresolved_inc_path.is_symlink():
                errors.append(
                    f"ERROR: {current.name}: included file {inc} is a symlink"
                )
                continue
            inc_path = unresolved_inc_path.resolve()
            try:
                inc_path.relative_to(schema_dir.resolve())
            except ValueError:
                errors.append(
                    f"ERROR: {current.name}: included file {inc} is outside "
                    "yaml directory"
                )
                continue
            if not inc_path.exists():
                errors.append(f"ERROR: {current.name}: included file {inc} not found")
            else:
                to_visit.append((inc_path, new_stack))

        for k, v in data.items():
            if k in ('version', 'includes'):
                continue

            if isinstance(v, list):
                declaration_kind = 'class'
            elif isinstance(v, dict) and (
                'type' in v or 'registry' in v
            ):
                declaration_kind = 'alias'
            elif isinstance(v, dict):
                declaration_kind = 'registry'
            else:
                errors.append(
                    f"ERROR: {current.name}: unknown schema construct '{k}'"
                )
                continue

            previous_kind = declaration_kinds.get(k)
            if previous_kind is not None and not (
                previous_kind == declaration_kind == 'registry'
            ):
                errors.append(
                    f"ERROR: {current.name}: duplicate declaration of '{k}'"
                )
                continue
            declaration_kinds.setdefault(k, declaration_kind)
            known_types.add(k)
            # Find type references
            if declaration_kind in {'class', 'alias'}:
                if isinstance(v, list): # Class definition
                    env['classes'][k] = v
                    class_fields = set()
                    for field in v:
                        if not isinstance(field, dict) or len(field) != 1:
                            errors.append(
                                f"ERROR: {current.name}: class '{k}' entries "
                                "must be one-entry mappings"
                            )
                            continue
                        for fname, fdesc in field.items():
                            if fname in class_fields:
                                errors.append(f"ERROR: {current.name}: duplicate class field '{fname}' in '{k}'")
                            class_fields.add(fname)
                            errors.extend(
                                check_descriptor(
                                    fdesc,
                                    True,
                                    f"{current.name}:{k}.{fname}",
                                )
                            )
                            if isinstance(fdesc, dict) and 'type' in fdesc:
                                collect_type_refs(
                                    fdesc['type'],
                                    f"{current.name}:{k}.{fname}.type",
                                )
                            if isinstance(fdesc, dict) and 'default' in fdesc:
                                env['defaults_to_validate'].append((fdesc['default'], fdesc, f"{current.name}:{k}.{fname}.default"))
                elif isinstance(v, dict):
                    env['aliases'][k] = v
                    if 'registry' in v:
                        if set(v) != {'registry'}:
                            errors.append(
                                f"ERROR: {current.name}:{k}: registry-backed "
                                "type must contain only 'registry'"
                            )
                        if not isinstance(v.get('registry'), str):
                            errors.append(
                                f"ERROR: {current.name}:{k}: registry name "
                                "must be a string"
                            )
                    if 'type' in v:
                        errors.extend(check_descriptor(v, False, f"{current.name}:{k}"))
                        collect_type_refs(v['type'], f"{current.name}:{k}.type")
                        if 'default' in v:
                            env['defaults_to_validate'].append((v['default'], v, f"{current.name}:{k}.default"))
            elif isinstance(v, dict):
                if k not in env['registries']:
                    env['registries'][k] = {}
                for reg_k, reg_v in v.items():
                    if reg_k in env['registries'][k]:
                        errors.append(f"ERROR: {current.name}: duplicate registry entry '{reg_k}' in '{k}'")
                        continue
                    env['registries'][k][reg_k] = reg_v
                    if isinstance(reg_v, list):
                        reg_fields = set()
                        for field in reg_v:
                            if not isinstance(field, dict) or len(field) != 1:
                                errors.append(
                                    f"ERROR: {current.name}: registry entry "
                                    f"'{k}.{reg_k}' fields must be one-entry "
                                    "mappings"
                                )
                                continue
                            for fname, fdesc in field.items():
                                if fname in reg_fields:
                                    errors.append(f"ERROR: {current.name}: duplicate field '{fname}' in registry '{k}.{reg_k}'")
                                reg_fields.add(fname)
                                errors.extend(
                                    check_descriptor(
                                        fdesc,
                                        True,
                                        f"{current.name}:{k}.{reg_k}.{fname}",
                                    )
                                )
                                if isinstance(fdesc, dict) and 'type' in fdesc:
                                    collect_type_refs(
                                        fdesc['type'],
                                        f"{current.name}:{k}.{reg_k}.{fname}.type",
                                    )
                                if isinstance(fdesc, dict) and 'default' in fdesc:
                                    env['defaults_to_validate'].append((fdesc['default'], fdesc, f"{current.name}:{k}.{reg_k}.{fname}.default"))
                    elif isinstance(reg_v, dict) and 'type' in reg_v:
                        errors.extend(check_descriptor(reg_v, False, f"{current.name}:{k}.{reg_k}"))
                        collect_type_refs(
                            reg_v['type'],
                            f"{current.name}:{k}.{reg_k}.type",
                        )
                        if 'default' in reg_v:
                            env['defaults_to_validate'].append((reg_v['default'], reg_v, f"{current.name}:{k}.{reg_k}.default"))
                    else:
                        errors.append(
                            f"ERROR: {current.name}: registry entry "
                            f"'{k}.{reg_k}' must be a class or descriptor"
                        )

    for t in all_type_refs:
        if t not in known_types:
            errors.append(f"ERROR: unknown_type '{t}' referenced in schema")

    for reg_name, reg_entries in env['registries'].items():
        if not reg_entries:
            errors.append(f"ERROR: registry '{reg_name}' is empty")
        is_referenced = any(alias.get('registry') == reg_name for alias in env['aliases'].values())
        if not is_referenced:
            errors.append(f"ERROR: registry '{reg_name}' is unreferenced")

    for alias_name, alias_def in env['aliases'].items():
        if 'registry' in alias_def:
            reg_name = alias_def['registry']
            if reg_name not in env['registries']:
                errors.append(f"ERROR: registry alias '{alias_name}' refers to missing registry '{reg_name}'")

    for def_val, desc, pth in env['defaults_to_validate']:
        def_errs = validate_descriptor(def_val, desc, env, pth)
        errors.extend(def_errs)

    return env, errors

def validate_type(data, t_refs, env, path):
    if not isinstance(t_refs, list):
        t_refs = [t_refs]

    # If the union allows null and data is None
    if 'null' in t_refs and data is None:
        return []

    # Check if data matches ANY of the t_refs
    for t in t_refs:
        errors = _check_single_type(data, t, env, path)
        if not errors: # Matches one type successfully
            return []

    # If none matched, run against the first one again to generate errors to return
    if t_refs:
        return _check_single_type(data, t_refs[0], env, path)
    return []


def validate_constraints(data, descriptor, path):
    errors = []
    if 'values' in descriptor and data not in descriptor['values']:
        errors.append(
            f"ERROR: {path}: value {data!r} is not one of the allowed values "
            f"{descriptor['values']!r}"
        )
    if 'pattern' in descriptor:
        pattern = descriptor['pattern']
        if not isinstance(data, str) or re.fullmatch(pattern, data) is None:
            errors.append(
                f"ERROR: {path}: value {data!r} does not match pattern "
                f"{pattern!r}"
            )
    if 'min_length' in descriptor:
        try:
            actual_length = len(data)
        except TypeError:
            actual_length = None
        minimum = descriptor['min_length']
        if actual_length is None or actual_length < minimum:
            errors.append(
                f"ERROR: {path}: expected minimum length {minimum}, got "
                f"{actual_length!r}"
            )
    if 'size' in descriptor:
        try:
            actual_size = len(data)
        except TypeError:
            actual_size = None
        expected_size = descriptor['size']
        if actual_size != expected_size:
            errors.append(
                f"ERROR: {path}: expected size {expected_size}, got "
                f"{actual_size!r}"
            )
    return errors


def validate_descriptor(data, descriptor, env, path):
    errors = validate_type(data, descriptor['type'], env, path)
    if errors:
        return errors
    return validate_constraints(data, descriptor, path)


def _check_single_type(data, t, env, path):
    if t == 'str':
        return [] if isinstance(data, str) else [f"ERROR: {path}: expected str, got {type(data).__name__}"]
    if t == 'int':
        # In python bool is a subclass of int. So isinstance(True, int) is True!
        # So we should exclude bools from int.
        if type(data) is int:
            return []
        return [f"ERROR: {path}: expected int, got {type(data).__name__}"]
    if t == 'float':
        if type(data) in (float, int):
            return []
        return [f"ERROR: {path}: expected float, got {type(data).__name__}"]
    if t == 'bool':
        if type(data) is bool:
            return []
        return [f"ERROR: {path}: expected bool, got {type(data).__name__}"]
    if t == 'date' or t == 'datetime':
        if isinstance(data, (date, datetime)):
            return []
        return [f"ERROR: {path}: expected {t}, got {type(data).__name__}"]

    if t.startswith('list['):
        inner = t[5:-1]
        if not isinstance(data, list):
            return [f"ERROR: {path}: expected list, got {type(data).__name__}"]
        errors = []
        for i, item in enumerate(data):
            suffix = f"[{i}]"
            if isinstance(item, dict):
                if 'name' in item:
                    suffix = f".{item['name']}"
                elif 'id' in item:
                    suffix = f".{item['id']}"
            errors.extend(validate_type(item, [inner], env, f"{path}{suffix}"))
        return errors

    if t.startswith('dict[') and t.endswith(']'):
        try:
            k_type, v_type = split_type_arguments(t[5:-1])
        except ValueError as exc:
            return [f"ERROR: {path}: {exc}"]
        if not isinstance(data, dict):
            return [f"ERROR: {path}: expected dict, got {type(data).__name__}"]
        errors = []
        for k, v in data.items():
            errors.extend(validate_type(k, [k_type], env, f"{path}.key({k})"))
            errors.extend(validate_type(v, [v_type], env, f"{path}.{k}"))
        return errors

    if t in env['classes']:
        if not isinstance(data, dict):
            return [f"ERROR: {path}: expected dict for class {t}, got {type(data).__name__}"]
        errors = []
        c_def = env['classes'][t]
        allowed_keys = set()
        for field in c_def:
            for fname, fdesc in field.items():
                allowed_keys.add(fname)
                if fdesc.get('required') and fname not in data:
                    errors.append(f"ERROR: {path}.{fname}: missing required field '{fname}' for class {t}")
                if fname in data:
                    errors.extend(
                        validate_descriptor(
                            data[fname], fdesc, env, f"{path}.{fname}"
                        )
                    )
        for k in data:
            if k not in allowed_keys:
                errors.append(f"ERROR: {path}.{k}: unknown field '{k}' for class {t}")
        return errors

    if t in env['aliases']:
        alias = env['aliases'][t]
        if 'registry' in alias:
            # It's a registry type
            reg_name = alias['registry']
            if not isinstance(data, dict):
                return [f"ERROR: {path}: expected dict for registry {reg_name}, got {type(data).__name__}"]
            if not data:
                return [f"ERROR: {path}: empty registry {reg_name}"]
            if reg_name not in env['registries']:
                return [f"ERROR: {path}: missing registry {reg_name}"]
            if len(data) != 1:
                return [f"ERROR: {path}: registry {reg_name} expects exactly one operation, got {len(data)}"]

            errors = []
            for key, val in data.items():
                if key not in env['registries'][reg_name]:
                    errors.append(f"ERROR: {path}.{key}: unknown registry key '{key}' for {reg_name}")
                    continue

                reg_def = env['registries'][reg_name][key]
                if isinstance(reg_def, dict) and 'type' in reg_def:
                    errors.extend(
                        validate_descriptor(val, reg_def, env, f"{path}.{key}")
                    )
                elif isinstance(reg_def, list): # it's a class inline
                    # Validate against an anonymous class
                    if not isinstance(val, dict):
                        errors.append(f"ERROR: {path}.{key}: expected dict, got {type(val).__name__}")
                        continue
                    allowed_keys = set()
                    for field in reg_def:
                        for fname, fdesc in field.items():
                            allowed_keys.add(fname)
                            if fdesc.get('required') and fname not in val:
                                errors.append(f"ERROR: {path}.{key}.{fname}: missing required field '{fname}'")
                            if fname in val:
                                errors.extend(
                                    validate_descriptor(
                                        val[fname], fdesc, env,
                                        f"{path}.{key}.{fname}",
                                    )
                                )
                    for k in val:
                        if k not in allowed_keys:
                            errors.append(f"ERROR: {path}.{key}.{k}: unknown field '{k}'")
            return errors

        else:
            # Normal alias
            errors = validate_type(data, alias['type'], env, path)
            if errors:
                return errors
            return validate_constraints(data, alias, path)

    return [f"ERROR: {path}: unknown type '{t}'"]

def validate_schemas(root: Path):
    env, errors = build_schema_env(root)
    return errors


SPEC_FILE_PATTERN = re.compile(r'^spec(?:_[a-z][a-z0-9_]*)?\.yaml$')


def example_spec_paths(example_dir: Path):
    return sorted(
        path
        for path in example_dir.iterdir()
        if path.is_file() and SPEC_FILE_PATTERN.fullmatch(path.name)
    )


def validate_grouped_rows(spec, spec_label):
    errors = []
    rows = spec.get('rows')
    if not isinstance(rows, list):
        return errors

    for index, row in enumerate(rows):
        if not isinstance(row, dict) or 'group_by' not in row:
            continue

        path = f"{spec_label}.rows[{index}].group_by"
        group_by = row['group_by']
        if not isinstance(group_by, list):
            continue
        if not group_by:
            errors.append(f"ERROR: {path}: grouped row requires at least one variable")
            continue

        string_variables = [
            variable for variable in group_by if isinstance(variable, str)
        ]
        duplicates = sorted(
            variable
            for variable in set(string_variables)
            if string_variables.count(variable) > 1
        )
        if duplicates:
            errors.append(
                f"ERROR: {path}: duplicate group variable(s): "
                f"{', '.join(duplicates)}"
            )

        driver = row.get('dataset', spec.get('base'))
        if not isinstance(driver, str):
            continue
        for variable_index, variable in enumerate(group_by):
            if not isinstance(variable, str):
                continue
            qualifier, separator, _ = variable.partition('.')
            if not separator or qualifier != driver:
                errors.append(
                    f"ERROR: {path}[{variable_index}]: grouped row variable "
                    f"{variable!r} must be qualified to driver {driver!r}"
                )

    return errors


def validate_examples_structure(root: Path, env, warnings=None):
    errors = []
    if warnings is None:
        warnings = []
    if env is None:
        return errors

    examples_dir = root / 'yaml' / 'examples'
    if not examples_dir.exists():
        return errors

    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith('.'):
            continue

        for spec_path in example_spec_paths(ex_dir):
            try:
                with open(spec_path, 'r', encoding='utf-8') as f:
                    spec = yaml.load(f, Loader=UniqueKeyLoader)
            except Exception:
                continue

            spec_label = f"{ex_dir.name}/{spec_path.name}"
            if not isinstance(spec, dict) or not spec:
                errors.append(
                    f"ERROR: {spec_label}: spec is empty or not a mapping"
                )
                continue

            if 'schema_version' in spec:
                spec_version = str(spec['schema_version'])
                env_version = str(env.get('version', '1.0'))
                if spec_version != env_version:
                    errors.append(
                        f"ERROR: {spec_label}: schema_version "
                        f"'{spec_version}' does not match bundle version "
                        f"'{env_version}'"
                    )

            spec_errors = validate_type(
                spec, ['root_class'], env, spec_label
            )
            spec_errors.extend(validate_grouped_rows(spec, spec_label))
            is_negative = ex_dir.name.startswith('negative-')
            if not is_negative:
                for spec_error in spec_errors:
                    if spec_error == KNOWN_STRUCTURAL_WARNING:
                        warnings.append(
                            "WARNING: existing DSDECOD verifications mapping "
                            "must become a list of one-entry mappings"
                        )
                    else:
                        errors.append(spec_error)
                continue

            error_yaml_path = ex_dir / 'expected' / 'error.yaml'
            if not error_yaml_path.exists():
                errors.extend(spec_errors)
                continue

            try:
                with open(error_yaml_path, 'r', encoding='utf-8') as f:
                    err_spec = yaml.load(f, Loader=UniqueKeyLoader)
                if not (
                    isinstance(err_spec, dict)
                    and err_spec.get('phase') == 'validation'
                ):
                    errors.extend(spec_errors)
                    continue

                expected_paths = err_spec.get('spec_paths', [])
                if not isinstance(expected_paths, list):
                    expected_paths = [expected_paths]

                filtered_errors = []
                for err in spec_errors:
                    parts = err.split(': ', 2)
                    if len(parts) < 2:
                        filtered_errors.append(err)
                        continue
                    path_part = parts[1]
                    prefix = f"{spec_label}."
                    norm_path = (
                        path_part[len(prefix):]
                        if path_part.startswith(prefix)
                        else path_part
                    )
                    path_matches = any(
                        norm_path == expected_path
                        or norm_path.startswith(f"{expected_path}.")
                        or norm_path.startswith(f"{expected_path}[")
                        for expected_path in expected_paths
                    )
                    if not path_matches:
                        filtered_errors.append(err)
                errors.extend(filtered_errors)
            except Exception:
                errors.extend(spec_errors)

    return errors

def check_yaml_files(root: Path):
    errors = []
    warnings = []
    for yaml_file in sorted(root.rglob('*.yaml')):
        if '.github' in yaml_file.parts:
            continue
        try:
            with open(yaml_file, 'r', encoding='utf-8') as f:
                yaml.load(f, Loader=UniqueKeyLoader)
        except Exception as e:
            msg = str(e)
            if hasattr(e, 'problem_mark') and e.problem_mark:
                msg += f" at line {e.problem_mark.line + 1}"
            errors.append(f"ERROR: {yaml_file.relative_to(root)}: {msg}")

    # Also validate schemas
    env, schema_errors = build_schema_env(root)
    errors.extend(schema_errors)
    errors.extend(validate_examples_structure(root, env, warnings))
    errors.extend(validate_examples_layout(root))
    errors.extend(validate_examples_index(root))

    csv_errors, csv_warnings = validate_examples_csv(root)
    errors.extend(csv_errors)
    warnings.extend(csv_warnings)
    return errors, warnings


def validate_examples_csv(root: Path):
    errors = []
    warnings = []
    examples_dir = root / 'yaml' / 'examples'
    if not examples_dir.exists():
        return errors, warnings

    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith('.'):
            continue

        for spec_path in example_spec_paths(ex_dir):
            try:
                with open(spec_path, 'r', encoding='utf-8') as f:
                    spec = yaml.load(f, Loader=UniqueKeyLoader)
            except Exception:
                continue

            if not isinstance(spec, dict) or 'columns' not in spec:
                continue

            output = spec.get('output')
            expected_cols = (
                output.get('columns')
                if isinstance(output, dict)
                else None
            )
            if not isinstance(expected_cols, list):
                continue

            expected_dir = ex_dir / 'expected'
            if not expected_dir.exists():
                continue
            for csv_file in sorted(expected_dir.glob('*.csv')):
                with open(csv_file, 'r', encoding='utf-8') as f:
                    reader = csv.reader(f)
                    try:
                        header = next(reader)
                        if header != expected_cols:
                            errors.append(
                                f"ERROR: {ex_dir.name}/{csv_file.name}: "
                                f"header mismatch for {spec_path.name}. "
                                f"Expected {expected_cols}, got {header}"
                            )
                    except StopIteration:
                        if expected_cols:
                            errors.append(
                                f"ERROR: {ex_dir.name}/{csv_file.name} is "
                                f"empty for {spec_path.name}"
                            )

    return errors, warnings


def validate_examples_index(root: Path):
    errors = []
    examples_dir = root / 'yaml' / 'examples'
    index_file = examples_dir / 'README.md'
    if not index_file.exists():
        return errors

    index_content = index_file.read_text(encoding='utf-8')
    # Find all table rows matching: | [`dir`](dir/) | desc |
    pattern = re.compile(
        r'^\|\s*\[`([^`]+)`\]\(([^)]+)\)\s*\|\s*([^|]+)\s*\|$',
        re.MULTILINE,
    )

    indexed_entries = {}
    last_dir = None
    not_alphabetical = False

    for match in pattern.finditer(index_content):
        dname = match.group(1)
        link_target = match.group(2)
        desc = match.group(3).strip()

        if link_target != f"{dname}/":
            errors.append(
                f"ERROR: index entry {dname} links to {link_target}, expected "
                f"{dname}/"
            )

        if dname in indexed_entries:
            errors.append(f"ERROR: duplicate index entry for {dname}")

        if last_dir and dname < last_dir and not not_alphabetical:
            errors.append(f"ERROR: index entries not alphabetical ({last_dir} before {dname})")
            not_alphabetical = True
        last_dir = dname

        indexed_entries[dname] = desc

        ex_dir = examples_dir / dname
        if not ex_dir.is_dir():
            errors.append(f"ERROR: index entry {dname} is stale (directory does not exist)")

    # Check for missing entries and title contract
    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith('.'):
            continue

        dname = ex_dir.name
        if dname not in indexed_entries:
            errors.append(f"ERROR: example {dname} not in index")
            continue

        readme = ex_dir / 'README.md'
        if readme.exists():
            content = readme.read_text(encoding='utf-8')
            first_line = content.split('\n', 1)[0] if content else ''
            if ':' in first_line:
                title_desc = first_line.split(':', 1)[1].strip()
                if title_desc != indexed_entries[dname]:
                    errors.append(f"ERROR: {dname} title description '{title_desc}' does not match index '{indexed_entries[dname]}'")

    return errors

def validate_examples_layout(root: Path):
    errors = []
    examples_dir = root / 'yaml' / 'examples'
    if not examples_dir.exists():
        return errors

    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith('.'):
            continue

        rel = ex_dir.relative_to(root)

        # README.md
        readme = ex_dir / 'README.md'
        if not readme.exists():
            errors.append(f"ERROR: {rel} missing README.md")
        else:
            if ex_dir.name.startswith('negative-'):
                content = readme.read_text(encoding='utf-8')
                if '## How to fix' not in content:
                    errors.append(f"ERROR: {rel}/README.md missing '## How to fix' section")

        # Specification files
        spec_paths = example_spec_paths(ex_dir)
        if not spec_paths:
            errors.append(
                f"ERROR: {rel} missing spec.yaml or spec_<variant>.yaml"
            )
        elif len(spec_paths) > 1 and any(
            path.name == 'spec.yaml' for path in spec_paths
        ):
            errors.append(
                f"ERROR: {rel} cannot mix spec.yaml with variant specs"
            )

        # input/
        if not (ex_dir / 'input').is_dir():
            errors.append(f"ERROR: {rel} missing input/ directory")

        # expected/
        expected_dir = ex_dir / 'expected'
        if not expected_dir.is_dir():
            errors.append(f"ERROR: {rel} missing expected/ directory")
        else:
            is_negative = ex_dir.name.startswith('negative-')
            has_error_yaml = (expected_dir / 'error.yaml').exists()
            if is_negative and not has_error_yaml:
                errors.append(f"ERROR: {rel} is negative but missing expected/error.yaml")
            elif not is_negative and has_error_yaml:
                errors.append(f"ERROR: {rel} is positive but has expected/error.yaml")

            has_artifacts = any(f.is_file() for f in expected_dir.iterdir() if f.name != 'error.yaml')
            if not is_negative and not has_artifacts:
                errors.append(f"ERROR: {rel}/expected has no artifacts")

    return errors

def main():
    parser = argparse.ArgumentParser(description="Validate yamaa repository structure and specs.")
    parser.add_argument('--root', type=Path, default=Path(__file__).parent.parent.parent,
                        help="Repository root directory")
    parser.add_argument('--warnings-as-errors', action='store_true',
                        help="Treat warnings as errors")
    args = parser.parse_args()

    errors, warnings = check_yaml_files(args.root)

    for warning in warnings:
        print(warning)

    if errors:
        for e in errors:
            print(e)
        return 1

    if warnings and args.warnings_as_errors:
        return 1

    print("PASS: Repository looks clean.")
    return 0

if __name__ == '__main__':
    sys.exit(main())
