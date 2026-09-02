#!/usr/bin/env python3
import argparse
import copy
import csv
import datetime as dt
import re
import sys
from pathlib import Path

import yaml
from yaml.composer import ComposerError
from yaml.constructor import ConstructorError
from yaml.events import AliasEvent


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


# PyYAML defaults to YAML 1.1 scalar resolution. Replace the resolvers whose
# YAML 1.2 core behavior differs: timestamps are plain strings, sexagesimal
# numbers are not numbers, octal uses 0o, and exponent-only decimals are
# floats. Null resolution is already compatible with the core schema.
for first_char, resolvers in list(
    UniqueKeyLoader.yaml_implicit_resolvers.items()
):
    UniqueKeyLoader.yaml_implicit_resolvers[first_char] = [
        (tag, regexp)
        for tag, regexp in resolvers
        if tag not in {
            'tag:yaml.org,2002:bool',
            'tag:yaml.org,2002:float',
            'tag:yaml.org,2002:int',
            'tag:yaml.org,2002:timestamp',
        }
    ]
UniqueKeyLoader.add_implicit_resolver(
    'tag:yaml.org,2002:bool',
    re.compile(r'^(?:true|True|TRUE|false|False|FALSE)$'),
    list('tTfF'),
)
UniqueKeyLoader.add_implicit_resolver(
    'tag:yaml.org,2002:int',
    re.compile(r'^(?:[-+]?[0-9]+|0o[0-7]+|0x[0-9a-fA-F]+)$'),
    list('-+0123456789'),
)
UniqueKeyLoader.add_implicit_resolver(
    'tag:yaml.org,2002:float',
    re.compile(
        r'^(?:'
        r'[-+]?(?:\.[0-9]+|[0-9]+(?:\.[0-9]*)?)(?:[eE][-+]?[0-9]+)?'
        r'|[-+]?\.(?:inf|Inf|INF)'
        r'|\.(?:nan|NaN|NAN)'
        r')$'
    ),
    list('-+0123456789.'),
)


def construct_yaml_12_int(loader, node):
    value = loader.construct_scalar(node)
    sign = 1
    if value.startswith('-'):
        sign = -1
    if value.startswith(('-', '+')):
        value = value[1:]
    if value.startswith('0o'):
        return sign * int(value[2:], 8)
    if value.startswith('0x'):
        return sign * int(value[2:], 16)
    return sign * int(value, 10)


UniqueKeyLoader.add_constructor(
    'tag:yaml.org,2002:int', construct_yaml_12_int
)


class PredicateError(ValueError):
    """A portable predicate cannot be tokenized or parsed."""

    def __init__(self, message, position):
        super().__init__(f"{message} at character {position + 1}")
        self.position = position


def tokenize_predicate(text):
    """Tokenize the closed R004 predicate language."""
    tokens = []
    index = 0
    length = len(text)
    while index < length:
        char = text[index]
        if char.isspace():
            index += 1
            continue

        if char == "'":
            start = index
            index += 1
            value = []
            while index < length:
                if text[index] != "'":
                    value.append(text[index])
                    index += 1
                    continue
                if index + 1 < length and text[index + 1] == "'":
                    value.append("'")
                    index += 2
                    continue
                index += 1
                tokens.append(('STRING', ''.join(value), start))
                break
            else:
                raise PredicateError('unterminated string literal', start)
            continue

        number = re.match(
            r'[+-]?[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?',
            text[index:],
        )
        if number is not None:
            value = number.group(0)
            tokens.append(('NUMBER', value, index))
            index += len(value)
            continue

        name = re.match(r'[A-Za-z_][A-Za-z0-9_]*', text[index:])
        if name is not None:
            value = name.group(0)
            end = index + len(value)
            if end < length and text[end] == '.':
                suffix = re.match(
                    r'[A-Za-z_][A-Za-z0-9_]*', text[end + 1:]
                )
                if suffix is None:
                    raise PredicateError('invalid qualified identifier', index)
                value += '.' + suffix.group(0)
                end += 1 + len(suffix.group(0))
            tokens.append(('NAME', value, index))
            index = end
            continue

        two_char = text[index:index + 2]
        if two_char in {'<>', '<=', '>='}:
            tokens.append(('OP', two_char, index))
            index += 2
            continue
        if char in {'=', '<', '>'}:
            tokens.append(('OP', char, index))
            index += 1
            continue
        if char == '(':
            tokens.append(('LPAREN', char, index))
            index += 1
            continue
        if char == ')':
            tokens.append(('RPAREN', char, index))
            index += 1
            continue
        if char == ',':
            tokens.append(('COMMA', char, index))
            index += 1
            continue

        raise PredicateError(f"unexpected character {char!r}", index)

    tokens.append(('EOF', '', length))
    return tokens


def valid_temporal_literal(kind, value):
    if kind == 'date':
        if re.fullmatch(r'[0-9]{4}-[0-9]{2}-[0-9]{2}', value) is None:
            return False
        try:
            dt.date.fromisoformat(value)
            return True
        except ValueError:
            return False

    if re.fullmatch(
        r'[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}'
        r'(?::[0-9]{2})?',
        value,
    ) is None:
        return False
    completed = value if len(value) == 19 else value + ':00'
    try:
        dt.datetime.strptime(completed, '%Y-%m-%dT%H:%M:%S')
        return True
    except ValueError:
        return False


def like_pattern_has_dangling_escape(pattern, escape):
    escaped = False
    for character in pattern:
        if escaped:
            escaped = False
        elif character == escape:
            escaped = True
    return escaped


class PredicateParser:
    def __init__(self, text):
        self.text = text
        self.tokens = tokenize_predicate(text)
        self.index = 0

    @property
    def token(self):
        return self.tokens[self.index]

    def advance(self):
        token = self.token
        self.index += 1
        return token

    def keyword(self, value):
        return self.token[0] == 'NAME' and self.token[1].upper() == value

    def take_keyword(self, value):
        if self.keyword(value):
            return self.advance()
        return None

    def require(self, kind, message):
        if self.token[0] != kind:
            raise PredicateError(message, self.token[2])
        return self.advance()

    def parse(self):
        node = self.parse_disjunction()
        if self.token[0] != 'EOF':
            raise PredicateError('unexpected trailing token', self.token[2])
        return node

    def parse_disjunction(self):
        node = self.parse_conjunction()
        while self.take_keyword('OR') is not None:
            node = {
                'kind': 'or',
                'left': node,
                'right': self.parse_conjunction(),
            }
        return node

    def parse_conjunction(self):
        node = self.parse_negation()
        while self.take_keyword('AND') is not None:
            node = {
                'kind': 'and',
                'left': node,
                'right': self.parse_negation(),
            }
        return node

    def parse_negation(self):
        if self.take_keyword('NOT') is not None:
            return {'kind': 'not', 'value': self.parse_negation()}
        return self.parse_boolean()

    def parse_boolean(self):
        if self.token[0] == 'LPAREN':
            self.advance()
            node = self.parse_disjunction()
            self.require('RPAREN', "expected ')' to close predicate")
            return node
        if self.take_keyword('TRUE') is not None:
            return {'kind': 'boolean', 'value': True}
        if self.take_keyword('FALSE') is not None:
            return {'kind': 'boolean', 'value': False}

        left = self.parse_operand()
        if self.token[0] == 'OP':
            operator = self.advance()[1]
            return {
                'kind': 'comparison',
                'operator': operator,
                'left': left,
                'right': self.parse_operand(),
            }
        if self.take_keyword('IS') is not None:
            negated = self.take_keyword('NOT') is not None
            if self.take_keyword('NULL') is None:
                raise PredicateError("expected NULL after IS", self.token[2])
            return {'kind': 'null_test', 'value': left, 'negated': negated}

        negated = False
        if self.take_keyword('NOT') is not None:
            negated = True
        if self.take_keyword('IN') is not None:
            self.require('LPAREN', "expected '(' after IN")
            values = [self.parse_operand()]
            while self.token[0] == 'COMMA':
                self.advance()
                values.append(self.parse_operand())
            self.require('RPAREN', "expected ')' after IN operands")
            return {
                'kind': 'in',
                'value': left,
                'values': values,
                'negated': negated,
            }
        if self.take_keyword('BETWEEN') is not None:
            lower = self.parse_operand()
            if self.take_keyword('AND') is None:
                raise PredicateError(
                    'expected AND in BETWEEN predicate', self.token[2]
                )
            return {
                'kind': 'between',
                'value': left,
                'lower': lower,
                'upper': self.parse_operand(),
                'negated': negated,
            }
        if self.take_keyword('LIKE') is not None:
            pattern = self.parse_operand()
            escape = None
            if self.take_keyword('ESCAPE') is not None:
                token = self.require(
                    'STRING', 'ESCAPE requires a string literal'
                )
                if len(token[1]) != 1:
                    raise PredicateError(
                        'ESCAPE requires exactly one code point', token[2]
                    )
                escape = token[1]
            if (
                escape is not None
                and pattern.get('kind') == 'literal'
                and pattern.get('type') == 'str'
                and like_pattern_has_dangling_escape(
                    pattern.get('value', ''), escape
                )
            ):
                raise PredicateError(
                    'LIKE pattern has a dangling escape', pattern['position']
                )
            return {
                'kind': 'like',
                'value': left,
                'pattern': pattern,
                'escape': escape,
                'negated': negated,
            }
        if negated:
            raise PredicateError(
                'NOT must precede IN, BETWEEN, or LIKE', self.token[2]
            )
        raise PredicateError(
            'operand must be followed by a Boolean operator', self.token[2]
        )

    def parse_operand(self):
        token = self.token
        if token[0] == 'NUMBER':
            self.advance()
            value_type = (
                'float'
                if '.' in token[1] or 'e' in token[1].lower()
                else 'int'
            )
            return {
                'kind': 'literal',
                'type': value_type,
                'value': token[1],
                'position': token[2],
            }
        if token[0] == 'STRING':
            self.advance()
            return {
                'kind': 'literal',
                'type': 'str',
                'value': token[1],
                'position': token[2],
            }
        if self.take_keyword('NULL') is not None:
            return {
                'kind': 'literal',
                'type': None,
                'value': None,
                'position': token[2],
            }
        if self.keyword('DATE') or self.keyword('DATETIME'):
            value_type = self.advance()[1].lower()
            text_token = self.require(
                'STRING', f'{value_type.upper()} requires a string literal'
            )
            if not valid_temporal_literal(value_type, text_token[1]):
                raise PredicateError(
                    f'invalid {value_type} literal', text_token[2]
                )
            return {
                'kind': 'literal',
                'type': value_type,
                'value': text_token[1],
                'position': token[2],
            }
        if token[0] == 'NAME':
            if token[1].upper() in {
                'AND', 'BETWEEN', 'ESCAPE', 'FALSE', 'IN', 'IS', 'LIKE',
                'NOT', 'OR', 'TRUE',
            }:
                raise PredicateError('expected operand', token[2])
            self.advance()
            return {
                'kind': 'identifier',
                'name': token[1],
                'position': token[2],
            }
        raise PredicateError('expected operand', token[2])


def parse_predicate(text):
    if not isinstance(text, str) or not text:
        raise PredicateError('predicate must be a non-empty string', 0)
    return PredicateParser(text).parse()


def predicate_operand_type(operand, resolver, errors):
    if operand['kind'] == 'literal':
        return operand['type']
    resolved = resolver(operand['name'])
    if resolved is None:
        errors.append(f"unknown identifier {operand['name']!r}")
    return resolved


def predicate_types_comparable(left, right):
    if left is None or right is None:
        return True
    if left in {'int', 'float'} and right in {'int', 'float'}:
        return True
    return left == right and left in {'str', 'date', 'datetime'}


def validate_predicate_types(ast, resolver):
    """Return static R004 name and operand-type errors for a parsed AST."""
    errors = []

    def operand_type(operand):
        return predicate_operand_type(operand, resolver, errors)

    def require_comparable(left_operand, right_operand):
        left_type = operand_type(left_operand)
        right_type = operand_type(right_operand)
        if not predicate_types_comparable(left_type, right_type):
            errors.append(
                'incompatible predicate operand types '
                f'{left_type!r} and {right_type!r}'
            )

    def visit(node):
        kind = node['kind']
        if kind in {'and', 'or'}:
            visit(node['left'])
            visit(node['right'])
        elif kind == 'not':
            visit(node['value'])
        elif kind == 'comparison':
            require_comparable(node['left'], node['right'])
        elif kind == 'null_test':
            operand_type(node['value'])
        elif kind == 'in':
            for value in node['values']:
                require_comparable(node['value'], value)
        elif kind == 'between':
            require_comparable(node['value'], node['lower'])
            require_comparable(node['value'], node['upper'])
        elif kind == 'like':
            value_type = operand_type(node['value'])
            pattern_type = operand_type(node['pattern'])
            for actual in (value_type, pattern_type):
                if actual is not None and actual != 'str':
                    errors.append(
                        'LIKE requires str operands; '
                        f'found {actual!r}'
                    )

    visit(ast)
    return errors


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
        member in {'list', 'dict'}
        or member.startswith('list[')
        or member.startswith('dict[')
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
    known_types = {'str', 'int', 'float', 'bool', 'null', 'list', 'dict'}
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
            if not isinstance(version, str) or not version:
                errors.append(
                    "ERROR: schema.yaml: version must be a non-empty string"
                )
        elif version != env.get('version'):
            errors.append(
                f"ERROR: {current.name}: schema version {version!r} does not "
                f"match bundle version {env.get('version')!r}"
            )

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
            if declaration_kind in {'class', 'alias'}:
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
    if t == 'list':
        return [] if isinstance(data, list) else [
            f"ERROR: {path}: expected list, got {type(data).__name__}"
        ]
    if t == 'dict':
        return [] if isinstance(data, dict) else [
            f"ERROR: {path}: expected dict, got {type(data).__name__}"
        ]

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


def validate_spec_names(spec, spec_label):
    """Validate cross-field names and uniqueness required by R002/R005/R015."""
    errors = []

    def duplicate_errors(values, path, noun):
        if not isinstance(values, list):
            return
        strings = [value for value in values if isinstance(value, str)]
        for value in sorted(set(strings)):
            if strings.count(value) > 1:
                errors.append(
                    f"ERROR: {spec_label}.{path}: duplicate {noun} {value!r}"
                )

    datasets = spec.get('datasets')
    dataset_names = set(datasets) if isinstance(datasets, dict) else set()
    domain = spec.get('domain')
    if isinstance(domain, str) and domain in dataset_names:
        errors.append(
            f"ERROR: {spec_label}.datasets.{domain}: dataset identifier "
            "must not equal the output domain"
        )

    base = spec.get('base')
    if isinstance(base, str) and base not in dataset_names:
        errors.append(
            f"ERROR: {spec_label}.base: undeclared dataset {base!r}"
        )

    columns = spec.get('columns')
    column_names = []
    if isinstance(columns, list):
        column_names = [
            column.get('name') for column in columns
            if isinstance(column, dict) and isinstance(column.get('name'), str)
        ]
    duplicate_errors(column_names, 'columns', 'column name')
    declared_columns = set(column_names)

    keys = spec.get('keys')
    duplicate_errors(keys, 'keys', 'key column')
    if isinstance(keys, list):
        if not keys:
            errors.append(
                f"ERROR: {spec_label}.keys: at least one key column is required"
            )
        for index, key in enumerate(keys):
            if isinstance(key, str) and key not in declared_columns:
                errors.append(
                    f"ERROR: {spec_label}.keys[{index}]: undeclared column "
                    f"{key!r}"
                )

    output = spec.get('output')
    output_columns = output.get('columns') if isinstance(output, dict) else None
    duplicate_errors(output_columns, 'output.columns', 'output column')
    if isinstance(output_columns, list):
        for index, name in enumerate(output_columns):
            if isinstance(name, str) and name not in declared_columns:
                errors.append(
                    f"ERROR: {spec_label}.output.columns[{index}]: "
                    f"undeclared column {name!r}"
                )
        output_names = set(output_columns)
        if isinstance(keys, list):
            for index, key in enumerate(keys):
                if isinstance(key, str) and key not in output_names:
                    errors.append(
                        f"ERROR: {spec_label}.keys[{index}]: key column "
                        f"{key!r} is not in output.columns"
                    )

    rows = spec.get('rows')
    if isinstance(rows, list):
        row_ids = [
            row.get('id') for row in rows
            if isinstance(row, dict) and isinstance(row.get('id'), str)
        ]
        duplicate_errors(row_ids, 'rows', 'row id')
        for index, row in enumerate(rows):
            if not isinstance(row, dict):
                continue
            driver = row.get('dataset')
            if isinstance(driver, str) and driver not in dataset_names:
                errors.append(
                    f"ERROR: {spec_label}.rows[{index}].dataset: "
                    f"undeclared dataset {driver!r}"
                )

    lookups = spec.get('record_lookups')
    if isinstance(lookups, list):
        lookup_ids = [
            lookup.get('id') for lookup in lookups
            if isinstance(lookup, dict) and isinstance(lookup.get('id'), str)
        ]
        duplicate_errors(lookup_ids, 'record_lookups', 'record lookup id')
        reserved_names = dataset_names | ({domain} if isinstance(domain, str) else set())
        for index, lookup in enumerate(lookups):
            if not isinstance(lookup, dict):
                continue
            lookup_id = lookup.get('id')
            if isinstance(lookup_id, str) and lookup_id in reserved_names:
                errors.append(
                    f"ERROR: {spec_label}.record_lookups[{index}].id: "
                    f"identifier {lookup_id!r} conflicts with a dataset or domain"
                )
            lookup_dataset = lookup.get('dataset')
            if isinstance(lookup_dataset, str) and lookup_dataset not in dataset_names:
                errors.append(
                    f"ERROR: {spec_label}.record_lookups[{index}].dataset: "
                    f"undeclared dataset {lookup_dataset!r}"
                )

    return errors


def validate_spec_contracts(spec, spec_label, spec_path=None):
    """Validate static cross-field contracts from normative rules."""
    errors = []

    rows = spec.get('rows')
    row_entries = rows if isinstance(rows, list) else []
    base = spec.get('base')
    full_spec = all(
        field in spec
        for field in ('domain', 'datasets', 'keys', 'output', 'columns')
    )
    if full_spec and not row_entries and not isinstance(base, str):
        errors.append(
            f"ERROR: {spec_label}.base: base is required when rows is absent "
            "or empty"
        )
    for index, row in enumerate(row_entries):
        if (
            full_spec
            and isinstance(row, dict)
            and 'dataset' not in row
            and not isinstance(base, str)
        ):
            errors.append(
                f"ERROR: {spec_label}.rows[{index}].dataset: row requires a "
                "dataset or root base"
            )

    columns = spec.get('columns')
    column_entries = columns if isinstance(columns, list) else []
    declared = {
        column.get('name')
        for column in column_entries
        if isinstance(column, dict) and isinstance(column.get('name'), str)
    }
    column_derivations = {
        column.get('name')
        for column in column_entries
        if (
            isinstance(column, dict)
            and isinstance(column.get('name'), str)
            and 'derivation' in column
        )
    }
    row_derivations = []
    for index, row in enumerate(row_entries):
        derivations = row.get('derivations') if isinstance(row, dict) else None
        names = set(derivations) if isinstance(derivations, dict) else set()
        row_derivations.append(names)
        undeclared = sorted(names - declared) if full_spec else []
        for name in undeclared:
            errors.append(
                f"ERROR: {spec_label}.rows[{index}].derivations.{name}: "
                f"undeclared column {name!r}"
            )

    covered_columns = sorted(declared) if full_spec else []
    for name in covered_columns:
        at_column = name in column_derivations
        at_rows = [name in names for names in row_derivations]
        if at_column and any(at_rows):
            errors.append(
                f"ERROR: {spec_label}.columns.{name}.derivation: column "
                "is also derived by a row"
            )
        elif row_entries and not at_column and not all(at_rows):
            missing_rows = [
                str(index) for index, present in enumerate(at_rows)
                if not present
            ]
            errors.append(
                f"ERROR: {spec_label}.columns.{name}.derivation: column is "
                "not derived by row(s) " + ', '.join(missing_rows)
            )
        elif not row_entries and not at_column:
            errors.append(
                f"ERROR: {spec_label}.columns.{name}.derivation: column has "
                "no derivation"
            )

    lookups = spec.get('record_lookups')
    if isinstance(lookups, list):
        for index, lookup in enumerate(lookups):
            if not isinstance(lookup, dict):
                continue
            path = f"{spec_label}.record_lookups[{index}]"
            has_source = 'source' in lookup
            has_key = 'key' in lookup
            if has_source != has_key:
                errors.append(
                    f"ERROR: {path}: source and key must be declared together"
                )
            elif has_source:
                sources = (
                    lookup['source']
                    if isinstance(lookup['source'], list)
                    else [lookup['source']]
                )
                keys = (
                    lookup['key']
                    if isinstance(lookup['key'], list)
                    else [lookup['key']]
                )
                if len(sources) != len(keys):
                    errors.append(
                        f"ERROR: {path}: source and key must have equal length"
                    )
            if ('order_by' in lookup) != ('keep' in lookup):
                errors.append(
                    f"ERROR: {path}: order_by and keep must be declared "
                    "together"
                )

    verification_ids = []
    verifications = spec.get('verifications')
    if isinstance(verifications, dict):
        verifications = [verifications]
    if isinstance(verifications, list):
        for index, verification in enumerate(verifications):
            if not isinstance(verification, dict) or len(verification) != 1:
                continue
            keyword, payload = next(iter(verification.items()))
            if not isinstance(payload, dict):
                continue
            path = f"{spec_label}.verifications[{index}].{keyword}"
            if keyword in {'all_or_none', 'implies', 'predicate'}:
                verification_id = payload.get('id')
                if isinstance(verification_id, str):
                    verification_ids.append((verification_id, path))
            if keyword in {'unique', 'all_or_none'}:
                names = payload.get('columns')
                if isinstance(names, list):
                    for name in names:
                        if isinstance(name, str) and name not in declared:
                            errors.append(
                                f"ERROR: {path}.columns: unknown column "
                                f"{name!r}"
                            )
                if (
                    keyword == 'all_or_none'
                    and isinstance(names, list)
                    and len(set(names)) < 2
                ):
                    errors.append(
                        f"ERROR: {path}.columns: requires at least two "
                        "distinct columns"
                    )
            if keyword == 'row_count':
                minimum = payload.get('min')
                maximum = payload.get('max')
                if minimum is None and maximum is None:
                    errors.append(
                        f"ERROR: {path}: requires at least one bound"
                    )
                elif (
                    type(minimum) is int
                    and type(maximum) is int
                    and minimum > maximum
                ):
                    errors.append(f"ERROR: {path}: min must not exceed max")

    seen_ids = set()
    for verification_id, path in verification_ids:
        if verification_id in seen_ids:
            errors.append(
                f"ERROR: {path}.id: duplicate dataset verification id "
                f"{verification_id!r}"
            )
        seen_ids.add(verification_id)

    for column in column_entries:
        if not isinstance(column, dict):
            continue
        column_name = column.get('name', '<unnamed>')
        column_type = column.get('type')
        verifications = column.get('verifications')
        if isinstance(verifications, dict):
            verifications = [verifications]
        if not isinstance(verifications, list):
            continue
        for index, verification in enumerate(verifications):
            if not isinstance(verification, dict) or len(verification) != 1:
                continue
            keyword, payload = next(iter(verification.items()))
            if not isinstance(payload, dict):
                continue
            path = (
                f"{spec_label}.columns.{column_name}.verifications[{index}]."
                f"{keyword}"
            )
            if keyword == 'range':
                if column_type not in {'int', 'float'}:
                    errors.append(
                        f"ERROR: {path}: range requires an int or float column"
                    )
                minimum = payload.get('min')
                maximum = payload.get('max')
                if minimum is None and maximum is None:
                    errors.append(
                        f"ERROR: {path}: requires at least one bound"
                    )
                elif (
                    type(minimum) in (int, float)
                    and type(maximum) in (int, float)
                    and minimum > maximum
                ):
                    errors.append(f"ERROR: {path}: min must not exceed max")
            if (
                keyword == 'max_length'
                and type(payload.get('max')) is int
                and payload['max'] < 1
            ):
                errors.append(f"ERROR: {path}.max: must be at least 1")
            if keyword in {'max_length', 'matches'} and column_type != 'str':
                errors.append(
                    f"ERROR: {path}: {keyword} requires a str column"
                )

    datasets = spec.get('datasets')
    if spec_path is not None and isinstance(datasets, dict):
        for dataset_id, source in datasets.items():
            source_path = source if isinstance(source, str) else None
            types = None
            if isinstance(source, dict):
                source_path = source.get('path')
                types = source.get('types')
            if not isinstance(source_path, str):
                continue
            resolved = spec_path.parent / source_path
            path = f"{spec_label}.datasets.{dataset_id}"
            if not resolved.is_file():
                errors.append(
                    f"ERROR: {path}: source path does not exist: "
                    f"{source_path}"
                )
                continue
            if resolved.suffix.lower() != '.csv' or not isinstance(types, dict):
                continue
            try:
                with open(resolved, 'r', encoding='utf-8', newline='') as f:
                    header = next(csv.reader(f, strict=True), [])
            except (OSError, csv.Error) as exc:
                errors.append(f"ERROR: {path}: cannot read CSV header: {exc}")
                continue
            for field in sorted(set(types) - set(header)):
                errors.append(
                    f"ERROR: {path}.types.{field}: field is absent from "
                    f"{source_path}"
                )

    return errors


def specification_column_types(spec):
    columns = spec.get('columns')
    if not isinstance(columns, list):
        return {}
    return {
        column['name']: column['type']
        for column in columns
        if (
            isinstance(column, dict)
            and isinstance(column.get('name'), str)
            and column.get('type') in {'str', 'int', 'float', 'date', 'datetime'}
        )
    }


def dataset_type_catalog(spec, spec_path):
    """Return statically discoverable field types for each dataset."""
    catalog = {}
    datasets = spec.get('datasets')
    if not isinstance(datasets, dict):
        return catalog

    for dataset_id, source in datasets.items():
        if not isinstance(dataset_id, str):
            continue
        fields = {}
        source_path = source if isinstance(source, str) else None
        declared_types = None
        producer_path = None
        if isinstance(source, dict):
            source_path = source.get('path')
            declared_types = source.get('types')
            producer_path = source.get('schema')

        if isinstance(producer_path, str) and spec_path is not None:
            resolved = spec_path.parent / producer_path
            try:
                with open(resolved, 'r', encoding='utf-8') as handle:
                    producer = yaml.load(handle, Loader=UniqueKeyLoader)
                fields.update(specification_column_types(producer))
            except (OSError, yaml.YAMLError):
                pass

        if (
            isinstance(source_path, str)
            and spec_path is not None
            and source_path.lower().endswith(('.csv', '.tsv'))
        ):
            resolved = spec_path.parent / source_path
            delimiter = '\t' if source_path.lower().endswith('.tsv') else ','
            try:
                with open(resolved, 'r', encoding='utf-8', newline='') as handle:
                    header = next(
                        csv.reader(handle, delimiter=delimiter, strict=True), []
                    )
                for field in header:
                    if isinstance(field, str) and field:
                        fields.setdefault(field, 'str')
            except (OSError, csv.Error):
                pass

        if isinstance(declared_types, dict):
            for field, value_type in declared_types.items():
                if (
                    isinstance(field, str)
                    and value_type in {'str', 'int', 'float', 'date', 'datetime'}
                ):
                    fields[field] = value_type
        catalog[dataset_id] = fields

    return catalog


def predicate_resolver(unqualified=None, qualified=None):
    unqualified = unqualified or {}
    qualified = qualified or {}

    def resolve(name):
        if '.' not in name:
            return unqualified.get(name)
        qualifier, field = name.split('.', 1)
        relation = qualified.get(qualifier)
        return relation.get(field) if isinstance(relation, dict) else None

    return resolve


def validate_predicate_at(text, path, resolver):
    try:
        ast = parse_predicate(text)
    except PredicateError as exc:
        return [f"ERROR: {path}: invalid predicate: {exc}"]
    return [
        f"ERROR: {path}: {message}"
        for message in dict.fromkeys(validate_predicate_types(ast, resolver))
    ]


def aggregate_filter_resolver(payload, default_resolver, datasets):
    if not isinstance(payload, dict):
        return default_resolver
    expression = payload.get('expr')
    if not isinstance(expression, str):
        return default_resolver
    qualifiers = re.findall(
        r'(?<![A-Za-z0-9_])([A-Za-z_][A-Za-z0-9_]*)\.', expression
    )
    if not qualifiers:
        return default_resolver
    qualifier = qualifiers[0]
    return predicate_resolver(qualified={qualifier: datasets.get(qualifier, {})})


def validate_expression_predicates(
    expression, path, resolver, datasets
):
    errors = []
    if not isinstance(expression, dict) or len(expression) != 1:
        return errors
    keyword, payload = next(iter(expression.items()))

    if keyword == 'case' and isinstance(payload, dict):
        branches = payload.get('branches')
        if isinstance(branches, list):
            for index, branch in enumerate(branches):
                if not isinstance(branch, dict):
                    continue
                branch_path = f"{path}.case.branches[{index}]"
                if isinstance(branch.get('when'), str):
                    errors.extend(
                        validate_predicate_at(
                            branch['when'], f"{branch_path}.when", resolver
                        )
                    )
                errors.extend(
                    validate_expression_predicates(
                        branch.get('then'),
                        f"{branch_path}.then",
                        resolver,
                        datasets,
                    )
                )
        if 'otherwise' in payload:
            errors.extend(
                validate_expression_predicates(
                    payload['otherwise'],
                    f"{path}.case.otherwise",
                    resolver,
                    datasets,
                )
            )

    elif keyword == 'source' and isinstance(payload, dict):
        multiple = payload.get('multiple_matches')
        variable = payload.get('variable')
        if (
            isinstance(multiple, dict)
            and isinstance(multiple.get('filter'), str)
        ):
            qualifier = (
                variable.split('.', 1)[0]
                if isinstance(variable, str) and '.' in variable
                else None
            )
            right_resolver = predicate_resolver(
                qualified={qualifier: datasets.get(qualifier, {})}
                if qualifier is not None
                else {}
            )
            errors.extend(
                validate_predicate_at(
                    multiple['filter'],
                    f"{path}.source.multiple_matches.filter",
                    right_resolver,
                )
            )

    elif keyword == 'aggregate' and isinstance(payload, dict):
        if isinstance(payload.get('filter'), str):
            errors.extend(
                validate_predicate_at(
                    payload['filter'],
                    f"{path}.aggregate.filter",
                    aggregate_filter_resolver(payload, resolver, datasets),
                )
            )

    elif keyword in {'row_number', 'rank'} and isinstance(payload, dict):
        if isinstance(payload.get('filter'), str):
            errors.extend(
                validate_predicate_at(
                    payload['filter'], f"{path}.{keyword}.filter", resolver
                )
            )

    elif keyword == 'str_concat' and isinstance(payload, dict):
        sources = payload.get('sources')
        if isinstance(sources, list):
            for index, source in enumerate(sources):
                errors.extend(
                    validate_expression_predicates(
                        source,
                        f"{path}.str_concat.sources[{index}]",
                        resolver,
                        datasets,
                    )
                )

    elif keyword == 'function' and isinstance(payload, dict):
        arguments = payload.get('args')
        if isinstance(arguments, dict):
            for name, argument in arguments.items():
                if isinstance(argument, dict):
                    errors.extend(
                        validate_expression_predicates(
                            argument,
                            f"{path}.function.args.{name}",
                            resolver,
                            datasets,
                        )
                    )

    return errors


def validate_derivation_predicates(derivation, path, resolver, datasets):
    errors = []
    if not isinstance(derivation, dict):
        return errors
    if 'value' not in derivation:
        return validate_expression_predicates(
            derivation, path, resolver, datasets
        )

    errors.extend(
        validate_expression_predicates(
            derivation.get('value'), f"{path}.value", resolver, datasets
        )
    )
    overrides = derivation.get('override')
    if isinstance(overrides, list):
        for index, override in enumerate(overrides):
            if not isinstance(override, dict):
                continue
            override_path = f"{path}.override[{index}]"
            if isinstance(override.get('when'), str):
                errors.extend(
                    validate_predicate_at(
                        override['when'], f"{override_path}.when", resolver
                    )
                )
            errors.extend(
                validate_expression_predicates(
                    override.get('value'),
                    f"{override_path}.value",
                    resolver,
                    datasets,
                )
            )
    return errors


def validate_spec_predicates(spec, spec_label, spec_path=None):
    """Parse, resolve, and type-check every R004 predicate in a spec."""
    errors = []
    datasets = dataset_type_catalog(spec, spec_path)
    output_types = specification_column_types(spec)
    lookups = {}
    lookup_entries = spec.get('record_lookups')
    if isinstance(lookup_entries, list):
        for index, lookup in enumerate(lookup_entries):
            if not isinstance(lookup, dict):
                continue
            lookup_id = lookup.get('id')
            dataset_id = lookup.get('dataset')
            if isinstance(lookup_id, str) and isinstance(dataset_id, str):
                lookups[lookup_id] = datasets.get(dataset_id, {})
            if isinstance(lookup.get('filter'), str):
                resolver = predicate_resolver(
                    qualified={dataset_id: datasets.get(dataset_id, {})}
                    if isinstance(dataset_id, str)
                    else {}
                )
                errors.extend(
                    validate_predicate_at(
                        lookup['filter'],
                        f"{spec_label}.record_lookups[{index}].filter",
                        resolver,
                    )
                )

    column_resolver = predicate_resolver(
        unqualified=output_types, qualified={**datasets, **lookups}
    )
    columns = spec.get('columns')
    if isinstance(columns, list):
        for index, column in enumerate(columns):
            if not isinstance(column, dict):
                continue
            name = column.get('name', index)
            if 'derivation' in column:
                errors.extend(
                    validate_derivation_predicates(
                        column['derivation'],
                        f"{spec_label}.columns.{name}.derivation",
                        column_resolver,
                        datasets,
                    )
                )

    rows = spec.get('rows')
    if isinstance(rows, list):
        for index, row in enumerate(rows):
            if not isinstance(row, dict):
                continue
            driver = row.get('dataset', spec.get('base'))
            driver_fields = (
                datasets.get(driver, {}) if isinstance(driver, str) else {}
            )
            derivations = row.get('derivations')
            row_output = {
                name: output_types[name]
                for name in derivations or {}
                if name in output_types
            } if isinstance(derivations, dict) else {}
            is_grouped = isinstance(row.get('group_by'), list)
            row_resolver = predicate_resolver(
                unqualified=row_output,
                qualified={driver: driver_fields}
                if isinstance(driver, str)
                else {},
            )
            if isinstance(row.get('filter'), str):
                filter_resolver = (
                    predicate_resolver(unqualified=row_output)
                    if is_grouped
                    else predicate_resolver(
                        qualified={driver: driver_fields}
                        if isinstance(driver, str)
                        else {}
                    )
                )
                errors.extend(
                    validate_predicate_at(
                        row['filter'],
                        f"{spec_label}.rows[{index}].filter",
                        filter_resolver,
                    )
                )
            if isinstance(derivations, dict):
                for name, derivation in derivations.items():
                    errors.extend(
                        validate_derivation_predicates(
                            derivation,
                            f"{spec_label}.rows[{index}].derivations.{name}",
                            row_resolver,
                            datasets,
                        )
                    )

    verifications = spec.get('verifications')
    if isinstance(verifications, dict):
        verifications = [verifications]
    if isinstance(verifications, list):
        output_resolver = predicate_resolver(
            unqualified=output_types, qualified=lookups
        )
        for index, verification in enumerate(verifications):
            if not isinstance(verification, dict) or len(verification) != 1:
                continue
            keyword, payload = next(iter(verification.items()))
            if not isinstance(payload, dict):
                continue
            fields = (
                ('when', 'then') if keyword == 'implies'
                else ('assert',) if keyword == 'predicate'
                else ()
            )
            for field in fields:
                if isinstance(payload.get(field), str):
                    errors.extend(
                        validate_predicate_at(
                            payload[field],
                            f"{spec_label}.verifications[{index}]."
                            f"{keyword}.{field}",
                            output_resolver,
                        )
                    )

    return errors


def validate_spec_document(spec, spec_label, spec_path, env, spec_stack=None):
    """Validate one complete specification and its producer dependencies."""
    if not isinstance(spec, dict) or not spec:
        return [
            f"ERROR: {spec_label}: spec is empty or not a mapping"
        ]

    errors = []
    if 'schema_version' in spec:
        spec_version = str(spec['schema_version'])
        env_version = str(env.get('version', '1.0'))
        if spec_version != env_version:
            errors.append(
                f"ERROR: {spec_label}: schema_version '{spec_version}' "
                f"does not match bundle version '{env_version}'"
            )

    errors.extend(validate_type(spec, ['root_class'], env, spec_label))
    errors.extend(validate_grouped_rows(spec, spec_label))
    errors.extend(validate_spec_names(spec, spec_label))
    errors.extend(validate_spec_contracts(spec, spec_label, spec_path))
    errors.extend(validate_spec_predicates(spec, spec_label, spec_path))

    next_stack = set(spec_stack or ())
    next_stack.add(spec_path.resolve())
    errors.extend(
        validate_producing_specs(
            spec, spec_label, spec_path, env, next_stack
        )
    )
    return errors


def validate_producer_output_contract(producer, path):
    """Require each stored producer column to carry a usable label."""
    errors = []
    output = producer.get('output')
    output_columns = output.get('columns') if isinstance(output, dict) else None
    selected = (
        {name for name in output_columns if isinstance(name, str)}
        if isinstance(output_columns, list)
        else set()
    )
    columns = producer.get('columns')
    if not isinstance(columns, list):
        return errors
    for index, column in enumerate(columns):
        if not isinstance(column, dict) or column.get('name') not in selected:
            continue
        label = column.get('label')
        if not isinstance(label, str) or not label.strip():
            errors.append(
                f"ERROR: {path}.columns[{index}].label: stored producer "
                "column requires a non-empty label"
            )
    return errors


def validate_producing_specs(spec, spec_label, spec_path, env, spec_stack):
    """Validate producer workflow edges and referenced artifact headers."""
    errors = []
    datasets = spec.get('datasets')
    if not isinstance(datasets, dict):
        return errors

    for dataset_id, source in datasets.items():
        if not isinstance(source, dict) or 'schema' not in source:
            continue

        path = f"{spec_label}.datasets.{dataset_id}"
        if 'types' in source:
            types = source['types']
            if isinstance(types, dict) and types:
                for field in sorted(types, key=str):
                    errors.append(
                        f"ERROR: {path}.types.{field}: field type is already "
                        "supplied by the producing specification"
                    )
            else:
                errors.append(
                    f"ERROR: {path}.types: inline types cannot be combined "
                    "with a producing specification"
                )

        schema_ref = source.get('schema')
        if not isinstance(schema_ref, str):
            continue
        producer_path = spec_path.parent / schema_ref
        resolved_producer = producer_path.resolve()
        if resolved_producer in spec_stack:
            errors.append(
                f"ERROR: {path}.schema: producer workflow dependency cycle "
                f"through {schema_ref}"
            )
            continue
        if not producer_path.is_file():
            errors.append(
                f"ERROR: {path}.schema: producing specification does not "
                f"exist: {schema_ref}"
            )
            continue

        try:
            with open(producer_path, 'r', encoding='utf-8') as f:
                producer = yaml.load(f, Loader=UniqueKeyLoader)
        except (OSError, UnicodeError, yaml.YAMLError) as exc:
            errors.append(
                f"ERROR: {path}.schema: cannot read producing specification "
                f"{schema_ref}: {exc}"
            )
            continue

        producer_errors = validate_spec_document(
            producer,
            f"{path}.schema",
            producer_path,
            env,
            spec_stack,
        )
        if isinstance(producer, dict):
            producer_errors.extend(
                validate_producer_output_contract(producer, f"{path}.schema")
            )
        errors.extend(producer_errors)
        if producer_errors or not isinstance(producer, dict):
            continue

        source_ref = source.get('path')
        if not isinstance(source_ref, str):
            continue
        source_path = spec_path.parent / source_ref
        if not source_path.is_file() or source_path.suffix.lower() != '.csv':
            continue
        try:
            with open(source_path, 'r', encoding='utf-8', newline='') as f:
                header = next(csv.reader(f, strict=True), [])
        except (OSError, UnicodeError, csv.Error) as exc:
            errors.append(
                f"ERROR: {path}.schema: cannot read CSV header for "
                f"{source_ref}: {exc}"
            )
            continue

        if not header:
            errors.append(
                f"ERROR: {path}.schema: stored CSV artifact is empty: "
                f"{source_ref}"
            )
            continue

        output = producer.get('output')
        output_columns = (
            output.get('columns') if isinstance(output, dict) else None
        )
        if isinstance(output_columns, list) and header != output_columns:
            errors.append(
                f"ERROR: {path}.schema.output.columns: producer output must "
                f"match the artifact header exactly; expected "
                f"{output_columns!r}, got {header!r}"
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
            spec_errors = validate_spec_document(
                spec, spec_label, spec_path, env
            )
            is_negative = ex_dir.name.startswith('negative-')
            if not is_negative:
                errors.extend(spec_errors)
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


EXPECTED_ERROR_PHASES = {
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


def spec_path_exists(spec, path):
    node = spec
    for part in path.split('.'):
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
    return True


def validate_expected_error_contracts(root: Path):
    errors = []
    examples_dir = root / 'yaml' / 'examples'
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
            set(contract) - {'phase', 'condition', 'spec_paths', 'context'}
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
                if not spec_path_exists(spec, path):
                    errors.append(
                        f"ERROR: {label}.spec_paths: {path!r} does not exist "
                        f"in {spec_path.name}"
                    )
    return errors


def validate_csv_shapes(root: Path):
    errors = []
    examples_dir = root / 'yaml' / 'examples'
    if not examples_dir.exists():
        return errors
    csv_paths = sorted(examples_dir.glob('*/input/*.csv'))
    csv_paths.extend(sorted(examples_dir.glob('*/expected/*.csv')))
    for csv_path in csv_paths:
        label = csv_path.relative_to(root)
        try:
            with open(csv_path, 'r', encoding='utf-8', newline='') as f:
                rows = list(csv.reader(f, strict=True))
        except (OSError, csv.Error) as exc:
            errors.append(f"ERROR: {label}: invalid CSV: {exc}")
            continue
        if not rows:
            errors.append(f"ERROR: {label}: CSV is empty")
            continue
        header = rows[0]
        if not header or any(not name for name in header):
            errors.append(f"ERROR: {label}: header contains an empty name")
        duplicates = sorted(
            name for name in set(header) if header.count(name) > 1
        )
        if duplicates:
            errors.append(
                f"ERROR: {label}: duplicate header(s): "
                + ', '.join(duplicates)
            )
        for line_number, row in enumerate(rows[1:], 2):
            if len(row) != len(header):
                errors.append(
                    f"ERROR: {label}:{line_number}: expected {len(header)} "
                    f"fields, got {len(row)}"
                )
    return errors


README_FORBIDDEN_PATTERN = re.compile(
    r'\b(?:derivation|schema|handler|verification)s?\b'
    r'|R0[01][0-9]|output\.columns',
    re.IGNORECASE,
)
README_KEY_COLUMNS = {
    'STUDYID', 'USUBJID', 'DOMAIN', 'SUBJID', 'AESEQ', 'VSSEQ', 'LBSEQ',
    'RSSEQ', 'ASEQ', 'PARAMCD', 'PARAM', 'AVISIT', 'VISIT', 'RDOMAIN',
    'IDVAR', 'QNAM',
}


def validate_example_readmes(root: Path):
    errors = []
    examples_dir = root / 'yaml' / 'examples'
    if not examples_dir.exists():
        return errors
    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith('.'):
            continue
        readme_path = ex_dir / 'README.md'
        if not readme_path.exists():
            continue
        label = readme_path.relative_to(root)
        text = readme_path.read_text(encoding='utf-8')
        for line_number, line in enumerate(text.splitlines(), 1):
            if len(line) > 79:
                errors.append(
                    f"ERROR: {label}:{line_number}: line has {len(line)} "
                    "characters; maximum is 79"
                )

        marker = '\n## How to fix\n'
        contract = text.split(marker, 1)[0]
        for line_number, line in enumerate(contract.splitlines(), 1):
            if README_FORBIDDEN_PATTERN.search(line):
                errors.append(
                    f"ERROR: {label}:{line_number}: schema vocabulary is "
                    "not allowed in the data contract"
                )

        headings = [
            line for line in text.splitlines() if line.startswith('## ')
        ]
        if ex_dir.name.startswith('negative-'):
            if headings != ['## How to fix']:
                errors.append(
                    f"ERROR: {label}: negative README must contain exactly "
                    "one '## How to fix' section and no other level-two section"
                )
        else:
            invalid = [
                heading for heading in headings
                if heading != '## Specification variants'
            ]
            if invalid:
                errors.append(
                    f"ERROR: {label}: unsupported level-two section(s): "
                    + ', '.join(invalid)
                )
            if (
                '## Specification variants' in headings
                and len(example_spec_paths(ex_dir)) < 2
            ):
                errors.append(
                    f"ERROR: {label}: Specification variants requires at "
                    "least two variant specs"
                )

        for csv_path in sorted((ex_dir / 'expected').glob('*.csv')):
            try:
                with open(csv_path, 'r', encoding='utf-8', newline='') as f:
                    header = next(csv.reader(f, strict=True), [])
            except (OSError, csv.Error):
                continue
            missing = [
                name for name in header
                if name not in README_KEY_COLUMNS and name not in text
            ]
            if missing:
                errors.append(
                    f"ERROR: {label}: expected columns not described: "
                    + ', '.join(missing)
                )
    return errors


def validate_rule_metadata(root: Path):
    errors = []
    rules_dir = root / 'yaml' / 'rules'
    index_path = rules_dir / 'README.md'
    if not rules_dir.is_dir() or not index_path.is_file():
        return errors
    index = index_path.read_text(encoding='utf-8')

    for rule_path in sorted(rules_dir.glob('R[0-9][0-9][0-9]-*.md')):
        label = rule_path.relative_to(root)
        text = rule_path.read_text(encoding='utf-8')
        match = re.match(r'\A---\n(.*?)\n---\n', text, re.DOTALL)
        if match is None:
            errors.append(f"ERROR: {label}: missing YAML front matter")
            continue
        try:
            metadata = yaml.load(match.group(1), Loader=UniqueKeyLoader)
        except yaml.YAMLError as exc:
            errors.append(f"ERROR: {label}: invalid front matter: {exc}")
            continue
        if not isinstance(metadata, dict):
            errors.append(f"ERROR: {label}: front matter must be a mapping")
            continue

        expected_id = rule_path.name[:4]
        if metadata.get('id') != expected_id:
            errors.append(
                f"ERROR: {label}: id must be {expected_id!r}, got "
                f"{metadata.get('id')!r}"
            )
        if metadata.get('status') != 'normative':
            errors.append(
                f"ERROR: {label}: maintained rule status must be "
                "'normative'"
            )

        index_row = re.search(
            rf'^\| {re.escape(expected_id)} \|.*?\| ([^|]+) \|',
            index,
            re.MULTILINE,
        )
        if index_row is None:
            errors.append(f"ERROR: {label}: rule is absent from rules/README.md")
        elif index_row.group(1).strip() != 'normative':
            errors.append(
                f"ERROR: yaml/rules/README.md: {expected_id} status must be "
                "'normative'"
            )

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
    errors.extend(validate_expected_error_contracts(root))
    errors.extend(validate_csv_shapes(root))
    errors.extend(validate_example_readmes(root))
    errors.extend(validate_rule_metadata(root))

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
                domain = spec.get('domain')
                if (
                    isinstance(domain, str)
                    and csv_file.name != f"{domain.lower()}.csv"
                ):
                    errors.append(
                        f"ERROR: {ex_dir.name}/{csv_file.name}: expected "
                        f"artifact name {domain.lower()}.csv"
                    )
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
