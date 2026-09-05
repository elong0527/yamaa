#!/usr/bin/env python3
import argparse
import copy
import csv
import datetime as dt
import decimal
import hashlib
import io
import json
import keyword
import math
import os
import re
import stat
import struct
import sys
from pathlib import Path, PurePosixPath

import yaml
from yaml.composer import ComposerError
from yaml.constructor import ConstructorError
from yaml.events import AliasEvent


class ValidationDiagnostic(str):
    """Rendered validation error with stable machine-readable identity."""

    def __new__(
        cls, path, condition, message, *, context=None, span=None,
        rendered=None,
    ):
        location = (
            f" at characters [{span[0]}, {span[1]})"
            if span is not None
            else ""
        )
        if rendered is None:
            rendered = f"ERROR: {path}: {message}{location} [{condition}]"
        value = super().__new__(cls, rendered)
        value.path = path
        value.condition = condition
        value.message = message
        value.context = dict(context or {})
        value.span = tuple(span) if span is not None else None
        return value


def validation_diagnostic(
    path, condition, message, *, context=None, span=None
):
    return ValidationDiagnostic(
        path,
        condition,
        message,
        context=context,
        span=span,
    )


# A condition is registered in the rule that owns its validation semantics.
# The same portable condition name may be registered by more than one rule
# when its required context differs by operation family.
VALIDATION_CONTEXT_FIELDS = {
    ('R001', 'dependency_cycle'): {'cycle'},
    ('R002', 'duplicate_identifier'): {'identifier'},
    ('R004', 'invalid_predicate'): {'predicate'},
    ('R004', 'incompatible_input_type'): {'left_type', 'right_type'},
    ('R004', 'unknown_field'): {'identifier'},
    ('R005', 'duplicate_order_term'): {'column'},
    ('R005', 'internal_column_in_keys'): {'column'},
    ('R005', 'undeclared_column'): {'column'},
    ('R006', 'invalid_field_type'): {'actual', 'expected'},
    ('R007', 'ambiguous_dictionary'): {'entries', 'folded_key'},
    ('R007', 'incomparable_sources'): {'sources', 'types'},
    ('R007', 'source_key_length_mismatch'): {
        'key', 'key_count', 'source', 'source_count',
    },
    ('R007', 'zero_offset'): {'offset'},
    ('R009', 'missing_verification_id'): set(),
    ('R010', 'incompatible_input_type'): {
        'actual', 'expected', 'expr', 'source',
    },
    ('R010', 'invalid_numeric_expression'): {'expr'},
    ('R010', 'prohibited_construct'): {'construct', 'expr'},
    ('R010', 'prohibited_function'): {'expr', 'function'},
    ('R010', 'qualified_identifier'): {'expr', 'identifier'},
    ('R010', 'unknown_field'): {'expr', 'identifier'},
    ('R011', 'value_not_permitted'): {'permitted', 'value'},
    ('R012', 'invalid_string_template'): {'placeholder', 'reason'},
    ('R013', 'aggregate_identifier_not_grouped'): {
        'dataset', 'identifier',
    },
    ('R013', 'incompatible_input_type'): {
        'actual', 'expected', 'source',
    },
    ('R014', 'unknown_field'): {'dataset', 'field'},
    ('R015', 'duplicate_identifier'): {'identifier'},
    ('R015', 'incomparable_range_types'): {
        'lower_type', 'record_lookup', 'upper_type', 'value_type',
    },
    ('R015', 'unpaired_fields'): {
        'declared', 'missing', 'record_lookup',
    },
    ('R016', 'month_out_of_range'): {'month'},
    ('R016', 'incompatible_input_type'): {'actual', 'expected', 'source'},
    ('R016', 'value_not_permitted'): {'permitted', 'value'},
    ('R017', 'inheritance_cycle'): {'reason'},
    ('R017', 'invalid_clear'): {'field'},
    ('R017', 'invalid_parent_path'): {'reason'},
    ('R017', 'missing_entry_output'): {'inherited_columns'},
    ('R017', 'redundant_field_type'): {'dataset', 'field', 'type'},
    ('R017', 'schema_version_mismatch'): {
        'entry_version', 'parent_version',
    },
    ('R018', 'function_contract_mismatch'): {
        'available', 'function', 'requested',
    },
    ('R021', 'resource_path_missing'): {'path'},
    ('R021', 'resource_path_not_regular_file'): {'path'},
    ('R021', 'resource_path_not_relative'): {'path'},
    ('R021', 'resource_path_parent_traversal'): {'path'},
    ('R021', 'resource_path_symlink'): {'path'},
    ('R021', 'resource_path_uri_scheme'): {'path'},
}
VALIDATION_CONDITION_REGISTRY = {
    key: {
        'allowed_phases': {'validation'},
        'required_context': required_context,
    }
    for key, required_context in VALIDATION_CONTEXT_FIELDS.items()
}


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


def normalize_non_finite_float(value):
    """Apply R011's normalization at a numeric value boundary."""
    if type(value) is float and not math.isfinite(value):
        return None
    return value


def construct_yaml_12_float(loader, node):
    value = yaml.SafeLoader.construct_yaml_float(loader, node)
    return normalize_non_finite_float(value)


UniqueKeyLoader.add_constructor(
    'tag:yaml.org,2002:float', construct_yaml_12_float
)


class PredicateError(ValueError):
    """A portable predicate cannot be tokenized or parsed."""

    def __init__(self, message, position):
        super().__init__(f"{message} at character {position + 1}")
        self.position = position


class PredicateSemanticIssue(str):
    def __new__(cls, message, condition, context, span):
        value = super().__new__(cls, message)
        value.condition = condition
        value.context = dict(context)
        value.span = tuple(span)
        return value


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
        errors.append(
            PredicateSemanticIssue(
                f"unknown identifier {operand['name']!r}",
                'unknown_field',
                {'identifier': operand['name']},
                (
                    operand['position'],
                    operand['position'] + len(operand['name']),
                ),
            )
        )
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
                PredicateSemanticIssue(
                    'incompatible predicate operand types '
                    f'{left_type!r} and {right_type!r}',
                    'incompatible_input_type',
                    {
                        'left_type': left_type,
                        'right_type': right_type,
                    },
                    (
                        left_operand['position'],
                        right_operand['position'] + max(
                            1,
                            len(
                                str(
                                    right_operand.get(
                                        'name', right_operand.get('value', '')
                                    )
                                    or ''
                                )
                            ),
                        ),
                    ),
                )
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
                        PredicateSemanticIssue(
                            'LIKE requires str operands; '
                            f'found {actual!r}',
                            'incompatible_input_type',
                            {'expected': 'str', 'actual': actual},
                            (
                                node['value']['position'],
                                node['pattern']['position'] + max(
                                    1,
                                    len(
                                        str(
                                            node['pattern'].get(
                                                'name',
                                                node['pattern'].get('value', ''),
                                            )
                                            or ''
                                        )
                                    ),
                                ),
                            ),
                        )
                    )

    visit(ast)
    return errors


class NumericExpressionError(ValueError):
    """An R010 expression cannot be tokenized or parsed."""

    def __init__(
        self,
        message,
        start,
        end=None,
        *,
        condition='invalid_numeric_expression',
        context=None,
    ):
        self.message = message
        self.span = (start, start + 1 if end is None else end)
        self.condition = condition
        self.context = dict(context or {})
        super().__init__(
            f"{message} at characters [{self.span[0]}, {self.span[1]})"
        )


NUMERIC_FUNCTION_ARITIES = {
    'ABS': (1, 1),
    'CEIL': (1, 1),
    'FLOOR': (1, 1),
    'TRUNC': (1, 1),
    'SQRT': (1, 1),
    'POWER': (2, 2),
    'EXP': (1, 1),
    'LN': (1, 1),
    'MOD': (2, 2),
    'GREATEST': (2, None),
    'LEAST': (2, None),
    'NULLIF': (2, 2),
    'COALESCE': (1, None),
}


PROHIBITED_NUMERIC_KEYWORDS = {
    'AND': 'boolean',
    'BETWEEN': 'comparison',
    'CASE': 'conditional',
    'ELSE': 'conditional',
    'END': 'conditional',
    'FALSE': 'boolean',
    'IN': 'comparison',
    'IS': 'comparison',
    'LIKE': 'comparison',
    'NOT': 'boolean',
    'OR': 'boolean',
    'OVER': 'window',
    'THEN': 'conditional',
    'TRUE': 'boolean',
    'WHEN': 'conditional',
}


def tokenize_numeric_expression(text):
    """Tokenize the closed R010 scalar numeric language."""
    tokens = []
    index = 0
    length = len(text)
    while index < length:
        char = text[index]
        if char.isspace():
            index += 1
            continue

        number = re.match(
            r'[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?',
            text[index:],
        )
        if number is not None:
            value = number.group(0)
            end = index + len(value)
            tokens.append(('NUMBER', value, index, end))
            index = end
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
                    raise NumericExpressionError(
                        'invalid qualified identifier', index, end + 1
                    )
                value += '.' + suffix.group(0)
                end += 1 + len(suffix.group(0))
            construct = PROHIBITED_NUMERIC_KEYWORDS.get(value.upper())
            if '.' not in value and construct is not None:
                raise NumericExpressionError(
                    f'{construct} construct is not permitted in a '
                    'numeric expression',
                    index,
                    end,
                    condition='prohibited_construct',
                    context={'construct': construct},
                )
            tokens.append(('NAME', value, index, end))
            index = end
            continue

        token_kinds = {
            '+': 'PLUS',
            '-': 'MINUS',
            '*': 'STAR',
            '/': 'SLASH',
            '(': 'LPAREN',
            ')': 'RPAREN',
            ',': 'COMMA',
        }
        if char in token_kinds:
            tokens.append((token_kinds[char], char, index, index + 1))
            index += 1
            continue

        if char in "<>=!":
            end = index + 1
            if end < length and text[end] == '=':
                end += 1
            raise NumericExpressionError(
                'comparison is not permitted in a numeric expression',
                index,
                end,
                condition='prohibited_construct',
                context={'construct': 'comparison'},
            )
        if char == "'":
            end = index + 1
            while end < length:
                if text[end] != "'":
                    end += 1
                    continue
                if end + 1 < length and text[end + 1] == "'":
                    end += 2
                    continue
                end += 1
                break
            raise NumericExpressionError(
                'string literals are not permitted in a numeric expression',
                index,
                end,
                condition='prohibited_construct',
                context={'construct': 'string'},
            )
        raise NumericExpressionError(
            f"unexpected character {char!r}", index, index + 1
        )

    tokens.append(('EOF', '', length, length))
    return tokens


class NumericExpressionParser:
    def __init__(self, text):
        self.text = text
        self.tokens = tokenize_numeric_expression(text)
        self.index = 0

    @property
    def token(self):
        return self.tokens[self.index]

    def advance(self):
        token = self.token
        self.index += 1
        return token

    def require(self, kind, message):
        if self.token[0] != kind:
            raise NumericExpressionError(
                message, self.token[2], self.token[3]
            )
        return self.advance()

    def parse(self):
        node = self.parse_expression()
        if self.token[0] != 'EOF':
            raise NumericExpressionError(
                'unexpected trailing token', self.token[2], self.token[3]
            )
        return node

    def parse_expression(self):
        node = self.parse_term()
        while self.token[0] in {'PLUS', 'MINUS'}:
            operator = self.advance()
            right = self.parse_term()
            node = {
                'kind': 'binary',
                'operator': operator[1],
                'left': node,
                'right': right,
                'operator_span': (operator[2], operator[3]),
                'span': (node['span'][0], right['span'][1]),
            }
        return node

    def parse_term(self):
        node = self.parse_factor()
        while self.token[0] in {'STAR', 'SLASH'}:
            operator = self.advance()
            right = self.parse_factor()
            node = {
                'kind': 'binary',
                'operator': operator[1],
                'left': node,
                'right': right,
                'operator_span': (operator[2], operator[3]),
                'span': (node['span'][0], right['span'][1]),
            }
        return node

    def parse_factor(self):
        if self.token[0] in {'PLUS', 'MINUS'}:
            operator = self.advance()
            value = self.parse_primary()
            return {
                'kind': 'unary',
                'operator': operator[1],
                'value': value,
                'operator_span': (operator[2], operator[3]),
                'span': (operator[2], value['span'][1]),
            }
        return self.parse_primary()

    def parse_primary(self):
        token = self.token
        if token[0] == 'NUMBER':
            self.advance()
            value_type = (
                'float'
                if '.' in token[1] or 'e' in token[1].lower()
                else 'int'
            )
            return {
                'kind': 'number',
                'type': value_type,
                'value': token[1],
                'span': (token[2], token[3]),
            }
        if token[0] == 'NAME':
            self.advance()
            keyword = token[1].upper()
            if self.token[0] != 'LPAREN':
                if keyword == 'NULL':
                    return {
                        'kind': 'null',
                        'span': (token[2], token[3]),
                    }
                if keyword in PROHIBITED_NUMERIC_KEYWORDS:
                    construct = PROHIBITED_NUMERIC_KEYWORDS[keyword]
                    raise NumericExpressionError(
                        f'{construct} construct is not permitted in a '
                        'numeric expression',
                        token[2],
                        token[3],
                        condition='prohibited_construct',
                        context={'construct': construct},
                    )
                return {
                    'kind': 'identifier',
                    'name': token[1],
                    'span': (token[2], token[3]),
                }

            self.advance()
            arguments = []
            if self.token[0] != 'RPAREN':
                arguments.append(self.parse_expression())
                while self.token[0] == 'COMMA':
                    self.advance()
                    arguments.append(self.parse_expression())
            close = self.require('RPAREN', "expected ')' to close function")
            return {
                'kind': 'call',
                'name': token[1],
                'arguments': arguments,
                'name_span': (token[2], token[3]),
                'span': (token[2], close[3]),
            }
        if token[0] == 'LPAREN':
            open_token = self.advance()
            node = self.parse_expression()
            close = self.require('RPAREN', "expected ')' to close expression")
            node = dict(node)
            node['span'] = (open_token[2], close[3])
            return node
        raise NumericExpressionError(
            'expected a number, identifier, function, NULL, or parenthesis',
            token[2],
            token[3],
        )


def parse_numeric_expression(text):
    if not isinstance(text, str) or not text:
        raise NumericExpressionError(
            'numeric expression must be a non-empty string', 0, 0
        )
    return NumericExpressionParser(text).parse()


def promote_numeric_types(types):
    concrete = [value_type for value_type in types if value_type is not None]
    if not concrete:
        return None
    return 'float' if 'float' in concrete else 'int'


def validate_numeric_expression_ast(ast, path, expression, resolver):
    """Resolve and type-check a parsed R010 expression."""
    errors = []

    def infer(node):
        kind = node['kind']
        if kind == 'number':
            return node['type']
        if kind == 'null':
            return None
        if kind == 'identifier':
            value_type, issue = resolver(node['name'])
            if issue is not None:
                condition, message, context = issue
                errors.append(
                    validation_diagnostic(
                        path,
                        condition,
                        message,
                        context={'expr': expression, **context},
                        span=node['span'],
                    )
                )
                return '<invalid>'
            if value_type not in {'int', 'float'}:
                errors.append(
                    validation_diagnostic(
                        path,
                        'incompatible_input_type',
                        f"identifier {node['name']!r} has non-numeric type "
                        f"{value_type!r}",
                        context={
                            'expr': expression,
                            'source': node['name'],
                            'expected': 'numeric',
                            'actual': value_type,
                        },
                        span=node['span'],
                    )
                )
                return '<invalid>'
            return value_type
        if kind == 'unary':
            return infer(node['value'])
        if kind == 'binary':
            left_type = infer(node['left'])
            right_type = infer(node['right'])
            if '<invalid>' in {left_type, right_type}:
                return '<invalid>'
            if node['operator'] == '/':
                return 'float'
            return promote_numeric_types([left_type, right_type])
        if kind == 'call':
            argument_types = [infer(argument) for argument in node['arguments']]
            name = node['name'].upper()
            arity = NUMERIC_FUNCTION_ARITIES.get(name)
            if arity is None:
                errors.append(
                    validation_diagnostic(
                        path,
                        'prohibited_function',
                        f"function {node['name']!r} is not permitted by R010",
                        context={
                            'expr': expression,
                            'function': node['name'],
                        },
                        span=node['name_span'],
                    )
                )
                return '<invalid>'
            minimum, maximum = arity
            actual = len(node['arguments'])
            if actual < minimum or (
                maximum is not None and actual > maximum
            ):
                expected = (
                    str(minimum)
                    if maximum == minimum
                    else f'at least {minimum}'
                )
                errors.append(
                    validation_diagnostic(
                        path,
                        'prohibited_function',
                        f"function {node['name']!r} requires {expected} "
                        f"argument(s), got {actual}",
                        context={
                            'expr': expression,
                            'function': node['name'],
                            'argument_count': actual,
                        },
                        span=node['span'],
                    )
                )
                return '<invalid>'
            if '<invalid>' in argument_types:
                return '<invalid>'
            if name in {'SQRT', 'POWER', 'EXP', 'LN'}:
                return 'float'
            if name in {'CEIL', 'FLOOR', 'TRUNC'}:
                return 'float'
            return promote_numeric_types(argument_types)
        raise AssertionError(f"unknown numeric AST node {kind!r}")

    result_type = infer(ast)
    return result_type, list(dict.fromkeys(errors))


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


def build_schema_env(root: Path, entrypoint='schema.yaml'):
    errors = []
    schema_dir = root / 'yaml'
    schema_path = schema_dir / entrypoint
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
        'defaults_to_validate': [],
        'entrypoint': entrypoint,
        'root': str(root.resolve()),
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
        if curr_res == schema_path.resolve():
            env['version'] = version
            if not isinstance(version, str) or not version:
                errors.append(
                    f"ERROR: {entrypoint}: version must be a non-empty string"
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

    attempted = []
    for t in t_refs:
        errors = _check_single_type(data, t, env, path)
        if not errors:
            return []
        attempted.append((t, errors))

    # Prefer a union branch whose outer runtime shape matched and whose
    # failure came from a narrower constraint. This keeps an int/string-enum
    # union from reporting only the first primitive mismatch.
    for t, errors in attempted:
        if _outer_type_matches(data, t, env):
            return errors
    if attempted:
        return attempted[0][1]
    return []


def _outer_type_matches(data, type_ref, env):
    if type_ref == 'str':
        return isinstance(data, str)
    if type_ref == 'int':
        return type(data) is int
    if type_ref == 'float':
        return type(data) in (int, float)
    if type_ref == 'bool':
        return type(data) is bool
    if type_ref == 'null':
        return data is None
    if type_ref == 'list' or type_ref.startswith('list['):
        return isinstance(data, list)
    if type_ref == 'dict' or type_ref.startswith('dict['):
        return isinstance(data, dict)
    if type_ref in env.get('classes', {}):
        return isinstance(data, dict)
    alias = env.get('aliases', {}).get(type_ref)
    if alias is None:
        return False
    if 'registry' in alias:
        return isinstance(data, dict)
    return any(
        _outer_type_matches(data, member, env)
        for member in _type_members(alias['type'])
    )


def validate_constraints(data, descriptor, path):
    errors = []
    if 'values' in descriptor and data not in descriptor['values']:
        errors.append(
            validation_diagnostic(
                path,
                'value_not_permitted',
                f"value {data!r} is not one of the allowed values "
                f"{descriptor['values']!r}",
                context={
                    'value': data,
                    'permitted': descriptor['values'],
                },
            )
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
        return [] if isinstance(data, str) else [
            validation_diagnostic(
                path,
                'invalid_field_type',
                f"expected str, got {type(data).__name__}",
                context={'expected': 'str', 'actual': type(data).__name__},
            )
        ]
    if t == 'int':
        # In python bool is a subclass of int. So isinstance(True, int) is True!
        # So we should exclude bools from int.
        if type(data) is int:
            return []
        return [validation_diagnostic(
            path,
            'invalid_field_type',
            f"expected int, got {type(data).__name__}",
            context={'expected': 'int', 'actual': type(data).__name__},
        )]
    if t == 'float':
        if type(data) in (float, int):
            return []
        return [validation_diagnostic(
            path,
            'invalid_field_type',
            f"expected float, got {type(data).__name__}",
            context={'expected': 'float', 'actual': type(data).__name__},
        )]
    if t == 'bool':
        if type(data) is bool:
            return []
        return [validation_diagnostic(
            path,
            'invalid_field_type',
            f"expected bool, got {type(data).__name__}",
            context={'expected': 'bool', 'actual': type(data).__name__},
        )]
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


INHERITANCE_KEYED_COLLECTIONS = {
    'datasets': ('mapping', None, 'dataset_class'),
    'record_lookups': ('list', 'id', 'record_lookup_class'),
    'columns': ('list', 'name', 'column_class'),
    'rows': ('list', 'id', 'row_class'),
}


def schema_class_fields(env, class_name):
    """Return a class definition as an insertion-ordered mapping."""
    return {
        name: descriptor
        for entry in env.get('classes', {}).get(class_name, [])
        for name, descriptor in entry.items()
    }


def _type_members(type_value):
    if isinstance(type_value, list):
        return [str(item).strip() for item in type_value]
    return [str(type_value).strip()]


def _type_matches(data, type_ref, env):
    return not _check_single_type(data, type_ref, env, '<normalization>')


def normalize_descriptor_value(data, descriptor, env):
    """Materialize the canonical R006 form of a descriptor value."""
    return normalize_type_value(data, descriptor['type'], env)


def normalize_type_value(data, type_value, env):
    """Normalize R006 list and single-required-field class shorthands."""
    members = _type_members(type_value)

    for member in members:
        if not (member.startswith('list[') and member.endswith(']')):
            continue
        inner = member[5:-1].strip()
        if inner in members and not isinstance(data, list):
            if _type_matches(data, inner, env):
                return [normalize_type_value(data, inner, env)]

    for class_name in members:
        fields = schema_class_fields(env, class_name)
        if not fields:
            continue
        required = [
            (name, descriptor)
            for name, descriptor in fields.items()
            if descriptor.get('required')
        ]
        if len(required) != 1:
            continue
        field_name, field_descriptor = required[0]
        field_types = _type_members(field_descriptor['type'])
        for member in members:
            if member == class_name or member not in field_types:
                continue
            if not _type_matches(data, member, env):
                continue
            expanded = {
                field_name: normalize_descriptor_value(
                    data, field_descriptor, env
                )
            }
            for name, descriptor in fields.items():
                if name not in expanded and 'default' in descriptor:
                    expanded[name] = copy.deepcopy(descriptor['default'])
            return expanded

    for member in members:
        if _type_matches(data, member, env):
            return normalize_single_type_value(data, member, env)
    return copy.deepcopy(data)


def normalize_inline_class(data, fields, env):
    if not isinstance(data, dict):
        return copy.deepcopy(data)
    normalized = {}
    descriptors = {
        name: descriptor
        for entry in fields
        for name, descriptor in entry.items()
    }
    for name, descriptor in descriptors.items():
        if name in data:
            normalized[name] = normalize_descriptor_value(
                data[name], descriptor, env
            )
        elif 'default' in descriptor:
            normalized[name] = copy.deepcopy(descriptor['default'])
    for name, value in data.items():
        if name not in normalized and name not in descriptors:
            normalized[name] = copy.deepcopy(value)
    return normalized


def normalize_single_type_value(data, type_ref, env):
    if type_ref.startswith('list[') and type_ref.endswith(']'):
        inner = type_ref[5:-1].strip()
        return [normalize_type_value(item, inner, env) for item in data]
    if type_ref.startswith('dict[') and type_ref.endswith(']'):
        key_type, value_type = split_type_arguments(type_ref[5:-1])
        return {
            normalize_type_value(key, key_type, env): normalize_type_value(
                value, value_type, env
            )
            for key, value in data.items()
        }
    if type_ref in env.get('classes', {}):
        return normalize_inline_class(data, env['classes'][type_ref], env)
    if type_ref in env.get('aliases', {}):
        alias = env['aliases'][type_ref]
        registry_name = alias.get('registry')
        if registry_name is not None:
            if not isinstance(data, dict) or len(data) != 1:
                return copy.deepcopy(data)
            keyword, payload = next(iter(data.items()))
            registry = env.get('registries', {}).get(registry_name, {})
            definition = registry.get(keyword)
            if isinstance(definition, list):
                payload = normalize_inline_class(payload, definition, env)
            elif isinstance(definition, dict) and 'type' in definition:
                payload = normalize_descriptor_value(payload, definition, env)
            return {keyword: payload}
        return normalize_type_value(data, alias['type'], env)
    return copy.deepcopy(data)


def validate_partial_inheritance_member(
    value, class_name, identity, label, env
):
    """Validate and normalize one direct keyed-collection member."""
    if not isinstance(value, dict):
        return value, [
            f"ERROR: {label}: expected mapping for {class_name}"
        ]

    fields = schema_class_fields(env, class_name)
    errors = []
    normalized = {}
    if identity is not None and identity not in value:
        errors.append(
            f"ERROR: {label}.{identity}: missing inheritance identifier"
        )

    for name in value:
        if name not in fields:
            errors.append(
                f"ERROR: {label}.{name}: unknown field '{name}' for class "
                f"{class_name}"
            )

    for name, descriptor in fields.items():
        if name not in value:
            continue
        field_value = value[name]
        path = f"{label}.{name}"
        if field_value is None:
            if descriptor.get('required') or name == identity:
                errors.append(
                    validation_diagnostic(
                        path,
                        'invalid_clear',
                        'cannot clear a required or identity field',
                        context={'field': name},
                    )
                )
            else:
                normalized[name] = None
            continue
        errors.extend(validate_descriptor(field_value, descriptor, env, path))
        normalized[name] = normalize_descriptor_value(
            field_value, descriptor, env
        )

    return normalized, errors


def validate_inheritance_layer(layer, label, env, require_output=False):
    """Validate one R017 layer without imposing final requiredness."""
    if not isinstance(layer, dict) or not layer:
        return layer, [
            f"ERROR: {label}: inheritance layer is empty or not a mapping"
        ]

    fields = schema_class_fields(env, 'root_class')
    errors = []
    normalized = {}

    if 'schema_version' not in layer:
        errors.append(
            f"ERROR: {label}.schema_version: schema_version_mismatch: every "
            "inheritance layer must declare schema_version"
        )
    if require_output and 'output' not in layer:
        errors.append(
            validation_diagnostic(
                f"{label}.parents",
                'missing_entry_output',
                'an inherited entry file must declare its complete output',
            )
        )

    for name in layer:
        if name not in fields:
            errors.append(
                f"ERROR: {label}.{name}: unknown field '{name}' for class "
                "root_class"
            )

    for name, descriptor in fields.items():
        if name not in layer:
            continue
        value = layer[name]
        path = f"{label}.{name}"

        if name == 'parents':
            errors.extend(validate_descriptor(value, descriptor, env, path))
            normalized[name] = normalize_descriptor_value(
                value, descriptor, env
            )
            continue

        collection = INHERITANCE_KEYED_COLLECTIONS.get(name)
        if collection is None:
            if value is None:
                if descriptor.get('required'):
                    errors.append(
                        validation_diagnostic(
                            path,
                            'invalid_clear',
                            'cannot clear a required root field',
                            context={'field': name},
                        )
                    )
                else:
                    normalized[name] = None
                continue
            errors.extend(validate_descriptor(value, descriptor, env, path))
            normalized[name] = normalize_descriptor_value(
                value, descriptor, env
            )
            continue

        kind, identity, class_name = collection
        if value is None:
            if descriptor.get('required'):
                errors.append(
                    validation_diagnostic(
                        path,
                        'invalid_clear',
                        'cannot clear a required root field',
                        context={'field': name},
                    )
                )
            else:
                normalized[name] = None
            continue

        if kind == 'mapping':
            if not isinstance(value, dict):
                errors.append(f"ERROR: {path}: expected mapping")
                continue
            normalized_mapping = {}
            for member_id, member in value.items():
                member_path = f"{path}.{member_id}"
                errors.extend(
                    validate_type(
                        member_id, ['dataset_id'], env,
                        f"{path}.key({member_id})",
                    )
                )
                if isinstance(member, str):
                    member_errors = validate_type(
                        member, ['project_path'], env, member_path
                    )
                    errors.extend(member_errors)
                    normalized_mapping[member_id] = {'path': member}
                else:
                    normalized_member, member_errors = (
                        validate_partial_inheritance_member(
                            member, class_name, identity, member_path, env
                        )
                    )
                    errors.extend(member_errors)
                    normalized_mapping[member_id] = normalized_member
            normalized[name] = normalized_mapping
            continue

        if not isinstance(value, list):
            errors.append(f"ERROR: {path}: expected list")
            continue
        normalized_list = []
        seen = set()
        for index, member in enumerate(value):
            member_id = (
                member.get(identity)
                if isinstance(member, dict) and identity is not None
                else None
            )
            member_path = (
                f"{path}.{member_id}"
                if isinstance(member_id, str)
                else f"{path}[{index}]"
            )
            normalized_member, member_errors = (
                validate_partial_inheritance_member(
                    member, class_name, identity, member_path, env
                )
            )
            errors.extend(member_errors)
            if isinstance(member_id, str):
                if member_id in seen:
                    errors.append(
                        f"ERROR: {member_path}.{identity}: "
                        f"duplicate_identifier: {member_id!r}"
                    )
                seen.add(member_id)
            normalized_list.append(normalized_member)
        normalized[name] = normalized_list

    version = layer.get('schema_version')
    bundle_version = env.get('version')
    if version is not None and str(version) != str(bundle_version):
        errors.append(
            validation_diagnostic(
                f"{label}.schema_version",
                'schema_version_mismatch',
                f"{version!r} does not match bundle version "
                f"{bundle_version!r}",
                context={
                    'entry_version': bundle_version,
                    'parent_version': version,
                },
            )
        )

    return normalized, errors


def _is_nonlocal_parent_reference(value):
    if not isinstance(value, str) or not value:
        return True
    if re.match(r'^[A-Za-z]:[\\/]', value):
        return False
    return bool(re.match(r'^[A-Za-z][A-Za-z0-9+.-]*:', value))


def _rebase_local_path(value, layer_path, entry_path):
    if not isinstance(value, str) or _is_nonlocal_parent_reference(value):
        return value
    written = Path(value)
    if written.is_absolute():
        return str(written.resolve())
    target = (layer_path.parent / written).resolve()
    try:
        relative = os.path.relpath(target, entry_path.parent.resolve())
    except ValueError:
        return str(target)
    return Path(relative).as_posix()


def rebase_layer_paths(layer, layer_path, entry_path):
    """Rebase current path-valued dataset fields to the entry file."""
    rebased = copy.deepcopy(layer)
    datasets = rebased.get('datasets')
    if not isinstance(datasets, dict):
        return rebased
    for source in datasets.values():
        if not isinstance(source, dict):
            continue
        for field in ('path', 'schema'):
            if isinstance(source.get(field), str):
                source[field] = _rebase_local_path(
                    source[field], layer_path, entry_path
                )
    return rebased


def _clear_provenance(provenance, prefix):
    for key in list(provenance):
        if key == prefix or key.startswith(prefix + '.'):
            del provenance[key]


def _merge_keyed_member(
    accumulated, incoming, class_name, logical_path, error_source,
    provenance_source, env, provenance, errors,
):
    fields = schema_class_fields(env, class_name)
    for name, value in incoming.items():
        if value is None:
            descriptor = fields.get(name, {})
            if descriptor.get('required') or name not in accumulated:
                errors.append(
                    f"ERROR: {error_source}.{logical_path}.{name}: "
                    "invalid_clear: "
                    "field is required or has no inherited value"
                )
                continue
            accumulated.pop(name, None)
            _clear_provenance(provenance, f"{logical_path}.{name}")
            continue
        accumulated[name] = copy.deepcopy(value)
        _clear_provenance(provenance, f"{logical_path}.{name}")
        provenance[f"{logical_path}.{name}"] = provenance_source


def merge_inheritance_layers(contributions, env):
    """Apply normalized contributions under the R017 shallow merge."""
    resolved = {}
    provenance = {}
    errors = []
    root_fields = schema_class_fields(env, 'root_class')

    for canonical_path, source, layer in contributions:
        provenance_source = str(canonical_path)
        for name, value in layer.items():
            if name == 'parents':
                continue
            if name == 'schema_version':
                resolved[name] = value
                provenance[name] = provenance_source
                continue

            collection = INHERITANCE_KEYED_COLLECTIONS.get(name)
            if value is None:
                descriptor = root_fields.get(name, {})
                if descriptor.get('required') or name not in resolved:
                    errors.append(
                        f"ERROR: {source}.{name}: invalid_clear: root field "
                        "is required or has no inherited value"
                    )
                    continue
                resolved.pop(name, None)
                _clear_provenance(provenance, name)
                continue

            if collection is None:
                resolved[name] = copy.deepcopy(value)
                _clear_provenance(provenance, name)
                provenance[name] = provenance_source
                continue

            kind, identity, class_name = collection
            if kind == 'mapping':
                target = resolved.setdefault(name, {})
                if not isinstance(target, dict):
                    target = {}
                    resolved[name] = target
                for member_id, member in value.items():
                    logical_path = f"{name}.{member_id}"
                    if member_id not in target:
                        target[member_id] = copy.deepcopy(member)
                        provenance[logical_path] = provenance_source
                        for field in member:
                            provenance[f"{logical_path}.{field}"] = (
                                provenance_source
                            )
                    else:
                        _merge_keyed_member(
                            target[member_id], member, class_name,
                            logical_path, source, provenance_source, env,
                            provenance, errors,
                        )
                continue

            target = resolved.setdefault(name, [])
            if not isinstance(target, list):
                target = []
                resolved[name] = target
            positions = {
                member.get(identity): index
                for index, member in enumerate(target)
                if isinstance(member, dict)
            }
            for member in value:
                if not isinstance(member, dict):
                    continue
                member_id = member.get(identity)
                logical_path = f"{name}.{member_id}"
                if member_id not in positions:
                    positions[member_id] = len(target)
                    target.append(copy.deepcopy(member))
                    provenance[logical_path] = provenance_source
                    for field in member:
                        provenance[f"{logical_path}.{field}"] = (
                            provenance_source
                        )
                else:
                    existing = target[positions[member_id]]
                    _merge_keyed_member(
                        existing, member, class_name, logical_path, source,
                        provenance_source, env, provenance, errors,
                    )

    return resolved, provenance, errors


def resolve_spec_inheritance(entry_spec, spec_label, spec_path, env):
    """Load and compose an R017 graph, returning data, errors, provenance."""
    entry_path = spec_path.resolve()
    contributions = []
    completed = set()
    active = []
    errors = []

    def visit(path, layer_label, supplied=None):
        canonical = path.resolve()
        if canonical in active:
            start = active.index(canonical)
            cycle = active[start:] + [canonical]
            errors.append(
                validation_diagnostic(
                    layer_label,
                    'inheritance_cycle',
                    ' -> '.join(str(item) for item in cycle),
                    context={'reason': 'parent_chain_returns_to_entry'},
                )
            )
            return
        if canonical in completed:
            return

        if supplied is None:
            try:
                with open(canonical, 'r', encoding='utf-8') as handle:
                    raw_layer = yaml.load(handle, Loader=UniqueKeyLoader)
            except (OSError, UnicodeError, yaml.YAMLError) as exc:
                errors.append(
                    f"ERROR: {layer_label}: parent_not_found: cannot read "
                    f"{canonical}: {exc}"
                )
                return
        else:
            raw_layer = supplied

        normalized, layer_errors = validate_inheritance_layer(
            raw_layer,
            layer_label,
            env,
            require_output=canonical == entry_path,
        )
        errors.extend(layer_errors)
        if layer_errors or not isinstance(normalized, dict):
            return

        active.append(canonical)
        parents = normalized.get('parents', [])
        if not isinstance(parents, list):
            parents = []
        for index, parent in enumerate(parents):
            parent_label = f"{layer_label}.parents[{index}]"
            if _is_nonlocal_parent_reference(parent):
                errors.append(
                    validation_diagnostic(
                        parent_label,
                        'invalid_parent_path',
                        f"{parent!r} is not a local filesystem path",
                        context={'reason': 'remote_reference'},
                    )
                )
                continue
            candidate = Path(parent)
            if not candidate.is_absolute():
                candidate = canonical.parent / candidate
            if not candidate.is_file():
                errors.append(
                    f"ERROR: {parent_label}: parent_not_found: "
                    f"{parent!r} is missing or is not a regular file"
                )
                continue
            visit(candidate.resolve(), parent_label)
        active.pop()

        if any(
            error.startswith(f"ERROR: {layer_label}.parents")
            for error in errors
        ):
            return
        completed.add(canonical)
        contributions.append(
            (
                canonical,
                layer_label,
                rebase_layer_paths(normalized, canonical, entry_path),
            )
        )

    visit(entry_path, spec_label, supplied=entry_spec)
    if errors:
        return None, errors, {}

    resolved, provenance, merge_errors = merge_inheritance_layers(
        contributions, env
    )
    errors.extend(merge_errors)
    if errors:
        return None, errors, provenance
    resolved, final_errors = finalize_resolved_inheritance(
        resolved, env
    )
    errors.extend(final_errors)
    if errors:
        return None, errors, provenance
    return resolved, errors, provenance


def predicate_identifier_names(text):
    try:
        ast = parse_predicate(text)
    except PredicateError:
        return set()

    names = set()

    def visit(node):
        if not isinstance(node, dict):
            return
        if node.get('kind') == 'identifier':
            names.add(node['name'])
        for value in node.values():
            if isinstance(value, dict):
                visit(value)
            elif isinstance(value, list):
                for item in value:
                    visit(item)

    visit(ast)
    return names


def closed_expression_identifier_names(text):
    """Collect identifiers from the R010/R013 closed expression grammars."""
    if not isinstance(text, str):
        return set()
    names = set()
    pattern = re.compile(
        r'(?<![A-Za-z0-9_.])'
        r'([A-Za-z_][A-Za-z0-9_]*(?:\.(?:[A-Za-z_][A-Za-z0-9_]*|\*))?)'
        r'(?![A-Za-z0-9_.])'
    )
    for match in pattern.finditer(text):
        name = match.group(1)
        tail = text[match.end():].lstrip()
        if name.upper() == 'NULL' or tail.startswith('('):
            continue
        names.add(name)
    return names


def numeric_expression_identifier_names(text):
    if not isinstance(text, str):
        return set()
    try:
        ast = parse_numeric_expression(text)
    except NumericExpressionError:
        return set()

    names = set()

    def visit(node):
        if node.get('kind') == 'identifier':
            names.add(node['name'])
        for value in node.values():
            if isinstance(value, dict):
                visit(value)
            elif isinstance(value, list):
                for item in value:
                    if isinstance(item, dict):
                        visit(item)

    visit(ast)
    return names


def string_template_identifier_names(text):
    if not isinstance(text, str):
        return set()
    names = set()
    index = 0
    while index < len(text):
        if text.startswith('{{', index) or text.startswith('}}', index):
            index += 2
            continue
        if text[index] != '{':
            index += 1
            continue
        end = text.find('}', index + 1)
        if end < 0:
            break
        candidate = text[index + 1:end]
        if re.fullmatch(
            r'[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)?',
            candidate,
        ):
            names.add(candidate)
        index = end + 1
    return names


def collect_descriptor_references(data, descriptor, env):
    return collect_type_references(data, descriptor['type'], env)


def collect_inline_class_references(data, fields, env):
    if not isinstance(data, dict):
        return set()
    references = set()
    descriptors = {
        name: descriptor
        for entry in fields
        for name, descriptor in entry.items()
    }
    for name, value in data.items():
        descriptor = descriptors.get(name)
        if descriptor is not None:
            references.update(
                collect_descriptor_references(value, descriptor, env)
            )
    return references


def collect_single_type_references(data, type_ref, env):
    if type_ref in {'variable', 'column_name'} and isinstance(data, str):
        return {('variable', data)}
    if type_ref == 'dataset_id' and isinstance(data, str):
        return {('dataset', data)}
    if type_ref == 'sql':
        return {
            ('variable', name)
            for name in predicate_identifier_names(data)
        }
    if type_ref == 'numeric_expression':
        return {
            ('variable', name)
            for name in numeric_expression_identifier_names(data)
        }
    if type_ref == 'aggregate_expression':
        return {
            ('variable', name)
            for name in closed_expression_identifier_names(data)
        }
    if type_ref == 'string_template':
        return {
            ('variable', name)
            for name in string_template_identifier_names(data)
        }

    if type_ref.startswith('list[') and type_ref.endswith(']'):
        if not isinstance(data, list):
            return set()
        inner = type_ref[5:-1].strip()
        references = set()
        for item in data:
            references.update(collect_type_references(item, inner, env))
        return references

    if type_ref.startswith('dict[') and type_ref.endswith(']'):
        if not isinstance(data, dict):
            return set()
        _, value_type = split_type_arguments(type_ref[5:-1])
        references = set()
        for value in data.values():
            references.update(
                collect_type_references(value, value_type, env)
            )
        return references

    if type_ref in env.get('classes', {}):
        return collect_inline_class_references(
            data, env['classes'][type_ref], env
        )

    alias = env.get('aliases', {}).get(type_ref)
    if alias is None:
        return set()
    registry_name = alias.get('registry')
    if registry_name is None:
        return collect_type_references(data, alias['type'], env)
    if not isinstance(data, dict) or len(data) != 1:
        return set()
    keyword, payload = next(iter(data.items()))
    definition = env.get('registries', {}).get(registry_name, {}).get(keyword)
    if isinstance(definition, list):
        return collect_inline_class_references(payload, definition, env)
    if isinstance(definition, dict) and 'type' in definition:
        return collect_descriptor_references(payload, definition, env)
    return set()


def collect_type_references(data, type_value, env):
    members = _type_members(type_value)
    for member in members:
        if _type_matches(data, member, env):
            return collect_single_type_references(data, member, env)
    return set()


def collect_member_field_references(member, class_name, fields, env):
    if not isinstance(member, dict):
        return set()
    descriptors = schema_class_fields(env, class_name)
    references = set()
    for field in fields:
        if field in member and field in descriptors:
            references.update(
                collect_descriptor_references(
                    member[field], descriptors[field], env
                )
            )
    return references


def _apply_reference(
    reference, datasets, lookups, live_columns, live_datasets, live_lookups
):
    kind, name = reference
    changed = False
    if kind == 'dataset':
        if name not in live_datasets:
            live_datasets.add(name)
            changed = True
        return changed
    if '.' not in name:
        if name not in live_columns:
            live_columns.add(name)
            changed = True
        return changed
    qualifier = name.split('.', 1)[0]
    if qualifier in datasets and qualifier not in live_datasets:
        live_datasets.add(qualifier)
        changed = True
    elif qualifier in lookups and qualifier not in live_lookups:
        live_lookups.add(qualifier)
        changed = True
    return changed


def order_term_variable(term):
    """Return the variable an R007 order term names, or None."""
    if isinstance(term, str):
        return term
    if isinstance(term, dict) and isinstance(term.get('variable'), str):
        return term['variable']
    return None


def prune_inheritance_collections(spec, env):
    """Remove keyed declarations unreachable from R017 semantic roots."""
    pruned = copy.deepcopy(spec)
    datasets = pruned.get('datasets')
    datasets = datasets if isinstance(datasets, dict) else {}
    columns = pruned.get('columns')
    column_entries = columns if isinstance(columns, list) else []
    column_map = {
        column.get('name'): column
        for column in column_entries
        if isinstance(column, dict) and isinstance(column.get('name'), str)
    }
    lookups = pruned.get('record_lookups')
    lookup_entries = lookups if isinstance(lookups, list) else []
    lookup_map = {
        lookup.get('id'): lookup
        for lookup in lookup_entries
        if isinstance(lookup, dict) and isinstance(lookup.get('id'), str)
    }
    rows = pruned.get('rows')
    row_entries = rows if isinstance(rows, list) else []

    live_columns = set()
    output = pruned.get('output')
    if isinstance(output, dict) and isinstance(output.get('columns'), list):
        live_columns.update(
            name for name in output['columns'] if isinstance(name, str)
        )
    if isinstance(output, dict) and isinstance(output.get('order_by'), list):
        live_columns.update(
            variable for variable in (
                order_term_variable(term) for term in output['order_by']
            )
            if variable is not None
        )
    keys = pruned.get('keys')
    if isinstance(keys, list):
        live_columns.update(name for name in keys if isinstance(name, str))
    for name, column in column_map.items():
        if 'verifications' in column:
            live_columns.add(name)

    live_datasets = set()
    base = pruned.get('base')
    if isinstance(base, str):
        live_datasets.add(base)
    live_lookups = set()

    references = set()
    root_fields = schema_class_fields(env, 'root_class')
    if 'verifications' in pruned and 'verifications' in root_fields:
        references.update(
            collect_descriptor_references(
                pruned['verifications'], root_fields['verifications'], env
            )
        )

    row_fields = schema_class_fields(env, 'row_class')
    for row in row_entries:
        if not isinstance(row, dict):
            continue
        driver = row.get('dataset', base)
        if isinstance(driver, str):
            live_datasets.add(driver)
        for field in ('group_by', 'filter'):
            if field in row and field in row_fields:
                references.update(
                    collect_descriptor_references(
                        row[field], row_fields[field], env
                    )
                )

    for reference in references:
        _apply_reference(
            reference, datasets, lookup_map, live_columns, live_datasets,
            live_lookups,
        )

    processed_columns = set()
    processed_lookups = set()
    processed_row_targets = set()
    while True:
        changed = False
        for name in list(live_columns - processed_columns):
            processed_columns.add(name)
            column = column_map.get(name)
            if column is None:
                continue
            refs = collect_member_field_references(
                column, 'column_class', ('derivation', 'verifications'), env
            )
            for reference in refs:
                changed |= _apply_reference(
                    reference, datasets, lookup_map, live_columns,
                    live_datasets, live_lookups,
                )

        for row_index, row in enumerate(row_entries):
            derivations = row.get('derivations') if isinstance(row, dict) else None
            if not isinstance(derivations, dict):
                continue
            for target in list(live_columns):
                marker = (row_index, target)
                if marker in processed_row_targets or target not in derivations:
                    continue
                processed_row_targets.add(marker)
                descriptor = row_fields.get('derivations')
                if descriptor is None:
                    continue
                refs = collect_type_references(
                    derivations[target], 'derivation', env
                )
                for reference in refs:
                    changed |= _apply_reference(
                        reference, datasets, lookup_map, live_columns,
                        live_datasets, live_lookups,
                    )

        lookup_fields = schema_class_fields(env, 'record_lookup_class')
        for lookup_id in list(live_lookups - processed_lookups):
            processed_lookups.add(lookup_id)
            lookup = lookup_map.get(lookup_id)
            if lookup is None:
                continue
            dataset_id = lookup.get('dataset')
            if isinstance(dataset_id, str) and dataset_id not in live_datasets:
                live_datasets.add(dataset_id)
                changed = True
            for field, value in lookup.items():
                if field in {'id', 'dataset'} or field not in lookup_fields:
                    continue
                refs = collect_descriptor_references(
                    value, lookup_fields[field], env
                )
                for reference in refs:
                    changed |= _apply_reference(
                        reference, datasets, lookup_map, live_columns,
                        live_datasets, live_lookups,
                    )

        if not changed:
            break

    if isinstance(pruned.get('columns'), list):
        pruned['columns'] = [
            column for column in pruned['columns']
            if isinstance(column, dict) and column.get('name') in live_columns
        ]
    if isinstance(pruned.get('datasets'), dict):
        pruned['datasets'] = {
            name: source for name, source in pruned['datasets'].items()
            if name in live_datasets
        }
    if isinstance(pruned.get('record_lookups'), list):
        pruned['record_lookups'] = [
            lookup for lookup in pruned['record_lookups']
            if isinstance(lookup, dict) and lookup.get('id') in live_lookups
        ]
        if not pruned['record_lookups']:
            del pruned['record_lookups']
    if isinstance(pruned.get('rows'), list):
        for row in pruned['rows']:
            derivations = row.get('derivations') if isinstance(row, dict) else None
            if isinstance(derivations, dict):
                row['derivations'] = {
                    name: derivation
                    for name, derivation in derivations.items()
                    if name in live_columns
                }
        if not pruned['rows']:
            del pruned['rows']
    return pruned


def column_dependency_names(column, rows, lookups, env):
    references = set()
    if isinstance(column, dict) and 'derivation' in column:
        references.update(
            collect_type_references(column['derivation'], 'derivation', env)
        )
    name = column.get('name') if isinstance(column, dict) else None
    for row in rows:
        derivations = row.get('derivations') if isinstance(row, dict) else None
        if isinstance(derivations, dict) and name in derivations:
            references.update(
                collect_type_references(derivations[name], 'derivation', env)
            )

    dependencies = set()
    for kind, reference in references:
        if kind != 'variable':
            continue
        if '.' not in reference:
            dependencies.add(reference)
            continue
        qualifier = reference.split('.', 1)[0]
        lookup = lookups.get(qualifier)
        if lookup is None:
            continue
        lookup_refs = collect_member_field_references(
            lookup,
            'record_lookup_class',
            ('source', 'between', 'filter', 'order_by'),
            env,
        )
        dependencies.update(
            value for ref_kind, value in lookup_refs
            if ref_kind == 'variable' and '.' not in value
        )
    return dependencies


def order_inherited_columns(spec, env):
    columns = spec.get('columns')
    if not isinstance(columns, list):
        return spec, []
    names = [
        column.get('name') if isinstance(column, dict) else None
        for column in columns
    ]
    if not all(isinstance(name, str) for name in names):
        return spec, []
    positions = {name: index for index, name in enumerate(names)}
    rows = spec.get('rows')
    rows = rows if isinstance(rows, list) else []
    lookup_entries = spec.get('record_lookups')
    lookup_entries = lookup_entries if isinstance(lookup_entries, list) else []
    lookups = {
        lookup.get('id'): lookup
        for lookup in lookup_entries
        if isinstance(lookup, dict) and isinstance(lookup.get('id'), str)
    }
    dependencies = {}
    errors = []
    for column in columns:
        name = column['name']
        dependencies[name] = column_dependency_names(
            column, rows, lookups, env
        )
        for dependency in sorted(dependencies[name]):
            if dependency not in positions:
                errors.append(
                    f"ERROR: columns.{name}.derivation: unknown_field: "
                    f"column dependency {dependency!r} is not declared"
                )
    if errors:
        return spec, errors

    dependents = {name: set() for name in names}
    indegree = {name: 0 for name in names}
    for name, required in dependencies.items():
        for dependency in required:
            dependents[dependency].add(name)
            indegree[name] += 1

    ready = [name for name in names if indegree[name] == 0]
    ready.sort(key=positions.get)
    ordered = []
    while ready:
        name = ready.pop(0)
        ordered.append(name)
        for dependent in sorted(dependents[name], key=positions.get):
            indegree[dependent] -= 1
            if indegree[dependent] == 0:
                ready.append(dependent)
                ready.sort(key=positions.get)
    if len(ordered) != len(names):
        cycle = [name for name in names if indegree[name] > 0]
        return spec, [
            "ERROR: columns: dependency cycle after inheritance: "
            + ' -> '.join(cycle)
        ]

    by_name = {column['name']: column for column in columns}
    reordered = copy.deepcopy(spec)
    reordered['columns'] = [by_name[name] for name in ordered]
    return reordered, []


def order_resolved_spec_fields(spec, env):
    root_fields = schema_class_fields(env, 'root_class')
    ordered = {}
    member_classes = {
        'datasets': 'dataset_class',
        'record_lookups': 'record_lookup_class',
        'columns': 'column_class',
        'rows': 'row_class',
    }
    for name in root_fields:
        if name not in spec or name == 'parents':
            continue
        value = spec[name]
        class_name = member_classes.get(name)
        if class_name is None:
            ordered[name] = value
            continue
        fields = schema_class_fields(env, class_name)
        if name == 'datasets' and isinstance(value, dict):
            ordered[name] = {
                member_id: {
                    field: member[field]
                    for field in fields
                    if isinstance(member, dict) and field in member
                }
                for member_id, member in value.items()
            }
        elif isinstance(value, list):
            ordered[name] = [
                {
                    field: member[field]
                    for field in fields
                    if isinstance(member, dict) and field in member
                }
                for member in value
            ]
        else:
            ordered[name] = value
    return ordered


def finalize_resolved_inheritance(spec, env):
    resolved = prune_inheritance_collections(spec, env)
    resolved, errors = order_inherited_columns(resolved, env)
    if errors:
        return resolved, errors
    return order_resolved_spec_fields(resolved, env), []

def validate_schemas(root: Path):
    _, errors = build_schema_env(root)
    _, environment_errors = build_schema_env(
        root, 'schema_environment.yaml'
    )
    return errors + environment_errors


SPEC_FILE_PATTERN = re.compile(r'^spec(?:_[a-z][a-z0-9_]*)?\.yaml$')


def example_spec_paths(example_dir: Path):
    return sorted(
        path
        for path in example_dir.iterdir()
        if path.is_file() and SPEC_FILE_PATTERN.fullmatch(path.name)
    )


def function_value_type(value):
    """Return the exact R018 scalar type, or a sentinel for invalid values."""
    value = normalize_non_finite_float(value)
    if value is None:
        return None
    if type(value) is bool:
        return 'bool'
    if type(value) is int:
        return 'int' if -(2 ** 63) <= value < 2 ** 63 else '<invalid>'
    if type(value) is float:
        return 'float'
    if isinstance(value, str):
        return 'str'
    if isinstance(value, dict) and len(value) == 1:
        kind, text = next(iter(value.items()))
        if (
            kind in {'date', 'datetime'}
            and isinstance(text, str)
            and valid_temporal_literal(kind, text)
        ):
            return kind
    return '<invalid>'


def function_value_matches(value, expected_type, accepts_missing=False):
    actual_type = function_value_type(value)
    if actual_type is None:
        return accepts_missing
    return actual_type == expected_type


def canonical_function_value(value, declared_type):
    """Encode an R018 value without losing its logical scalar type."""
    value = normalize_non_finite_float(value)
    actual_type = function_value_type(value)
    if actual_type is None:
        return {'type': 'missing'}
    if actual_type != declared_type:
        raise ValueError(
            f"expected {declared_type!r}, got {actual_type!r}"
        )
    if actual_type == 'float':
        encoded = struct.pack('>d', value).hex()
    elif actual_type == 'int':
        encoded = str(value)
    elif actual_type == 'bool':
        encoded = value
    elif actual_type in {'date', 'datetime'}:
        encoded = value[actual_type]
    else:
        encoded = value
    return {'type': actual_type, 'value': encoded}


def function_contract_fingerprint(name, contract):
    """Return the canonical language-neutral R018 contract identity."""
    params = []
    for parameter in contract.get('params', []):
        if not isinstance(parameter, dict):
            continue
        normalized = {
            'name': parameter.get('name'),
            'type': parameter.get('type'),
            'required': parameter.get('required', True),
            'accepts_missing': parameter.get('accepts_missing', False),
            'default': {'present': 'default' in parameter},
        }
        if 'default' in parameter:
            normalized['default']['value'] = canonical_function_value(
                parameter['default'], parameter.get('type')
            )
        params.append(normalized)
    logical = {
        'format': 'yamaa-r018-contract-v1',
        'name': name,
        'contract_version': contract.get('contract_version'),
        'params': params,
        'returns': contract.get('returns'),
        'may_return_missing': contract.get('may_return_missing', False),
        'comparison_decimals': str(contract.get('comparison_decimals', 4)),
    }
    # The normalized form has no JSON numeric values. Compact sorted JSON is
    # therefore the RFC 8785 representation without host-number formatting.
    payload = json.dumps(
        logical,
        ensure_ascii=False,
        allow_nan=False,
        sort_keys=True,
        separators=(',', ':'),
    )
    return 'sha256:' + hashlib.sha256(payload.encode('utf-8')).hexdigest()


R_RESERVED_NAMES = {
    'break', 'else', 'FALSE', 'for', 'function', 'if', 'Inf', 'in', 'NA',
    'NA_character_', 'NA_complex_', 'NA_integer_', 'NA_real_', 'NaN',
    'next', 'NULL', 'repeat', 'TRUE', 'while',
}


def valid_host_argument_name(language, name):
    if not isinstance(name, str):
        return False
    if language == 'python':
        return (
            re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', name) is not None
            and not keyword.iskeyword(name)
        )
    if language == 'r':
        syntactic = re.fullmatch(
            r'(?:[A-Za-z][A-Za-z0-9._]*|\.(?![0-9])[A-Za-z0-9._]+)',
            name,
        )
        return (
            syntactic is not None
            and name not in R_RESERVED_NAMES
            and name != '...'
            and not re.fullmatch(r'\.\.[0-9]+', name)
        )
    return False


def required_conformance_coverage(contract, parameters):
    required = {'normal', 'boundary'}
    if contract.get('returns') == 'float':
        required.add('numeric-comparison')
    if contract.get('may_return_missing', False):
        required.add('nullable-output')
    for parameter in parameters:
        name = parameter['name']
        if not parameter.get('required', True):
            required.add(f'default:{name}')
        missing_kind = (
            'accepted-missing'
            if parameter.get('accepts_missing', False)
            else 'short-circuit-missing'
        )
        required.add(f'{missing_kind}:{name}')
        if parameter.get('type') == 'bool':
            required.add(f'boolean-true:{name}')
            required.add(f'boolean-false:{name}')
    return required


def validate_case_coverage(
    covers, case_args, result, contract, parameters, short_circuits, path
):
    errors = []
    if not isinstance(covers, list):
        return errors, set()
    observed = {tag for tag in covers if isinstance(tag, str)}
    if not observed:
        errors.append(f"ERROR: {path}: covers must not be empty")
    if len(observed) != len(covers):
        errors.append(f"ERROR: {path}: coverage tags must be unique")
    by_name = {parameter['name']: parameter for parameter in parameters}

    for tag in observed:
        if tag in {'normal', 'boundary'}:
            continue
        if tag == 'nullable-output':
            if (
                not contract.get('may_return_missing', False)
                or result is not None
                or short_circuits
            ):
                errors.append(
                    f"ERROR: {path}: {tag!r} requires a missing result from "
                    "an invoked nullable binding"
                )
            continue
        if tag == 'numeric-comparison':
            if (
                contract.get('returns') != 'float'
                or function_value_type(result) != 'float'
            ):
                errors.append(
                    f"ERROR: {path}: {tag!r} requires a float result"
                )
            continue

        kind, _, name = tag.partition(':')
        parameter = by_name.get(name)
        if parameter is None:
            errors.append(
                f"ERROR: {path}: coverage tag {tag!r} names no parameter"
            )
            continue
        supplied = isinstance(case_args, dict) and name in case_args
        value = case_args.get(name) if supplied else '<omitted>'
        valid = False
        if kind == 'default':
            valid = not parameter.get('required', True) and not supplied
        elif kind == 'accepted-missing':
            valid = parameter.get('accepts_missing', False) and value is None
        elif kind == 'short-circuit-missing':
            valid = (
                not parameter.get('accepts_missing', False)
                and value is None
                and result is None
            )
        elif kind == 'boolean-true':
            valid = parameter.get('type') == 'bool' and value is True
        elif kind == 'boolean-false':
            valid = parameter.get('type') == 'bool' and value is False
        if not valid:
            errors.append(
                f"ERROR: {path}: case does not demonstrate {tag!r}"
            )
    return errors, observed


def validate_function_arguments(
    arguments, parameters, path, resolve_variable=None
):
    """Validate a closed logical argument set with exact R018 types."""
    errors = []
    if not isinstance(arguments, dict) or not isinstance(parameters, list):
        return errors
    declared = {
        parameter.get('name'): parameter
        for parameter in parameters
        if isinstance(parameter, dict) and isinstance(parameter.get('name'), str)
    }
    argument_names = {
        name for name in arguments if isinstance(name, str)
    }
    for name in sorted(argument_names - set(declared)):
        errors.append(
            f"ERROR: {path}.{name}: invalid_function_argument: unknown "
            f"argument {name!r}"
        )
    for name, parameter in declared.items():
        required = parameter.get('required', True)
        if required and name not in arguments:
            errors.append(
                f"ERROR: {path}.{name}: invalid_function_argument: missing "
                "required argument"
            )
        if name not in arguments:
            continue
        value = arguments[name]
        expected = parameter.get('type')
        accepts_missing = parameter.get('accepts_missing', False)
        if resolve_variable is not None and isinstance(value, str):
            actual = resolve_variable(value)
            if actual is None:
                errors.append(
                    f"ERROR: {path}.{name}: invalid_function_argument: "
                    f"unknown variable {value!r}"
                )
            elif actual != expected:
                errors.append(
                    f"ERROR: {path}.{name}: invalid_function_argument: "
                    f"expected exact type {expected!r}, got {actual!r} from "
                    f"variable {value!r}"
                )
            continue
        literal = value
        if resolve_variable is not None and isinstance(value, dict):
            if set(value) == {'literal'}:
                literal = value['literal']
        if literal is None:
            # Explicit missing is always valid authoring. A non-accepting
            # parameter short-circuits without invoking the binding.
            continue
        if not function_value_matches(literal, expected, accepts_missing):
            actual = function_value_type(literal)
            errors.append(
                f"ERROR: {path}.{name}: invalid_function_argument: expected "
                f"exact type {expected!r}, got {actual!r}"
            )
    return errors


def validate_project_environment(
    document, label, environment_path, schema_env
):
    """Validate one environment and its language-neutral conformance vectors."""
    errors = []
    if not isinstance(document, dict):
        return [f"ERROR: {label}: project environment must be a mapping"]
    structural_errors = (
        validate_type(document, ['environment_class'], schema_env, label)
    )
    if structural_errors:
        return structural_errors
    schema_version = document.get('schema_version')
    expected_version = str(schema_env.get('version', '1.0'))
    if str(schema_version) != expected_version:
        errors.append(
            f"ERROR: {label}.schema_version: schema_version_mismatch: "
            f"expected {expected_version!r}, got {schema_version!r}"
        )

    runtime = document.get('runtime')
    language = runtime.get('language') if isinstance(runtime, dict) else None
    functions = document.get('functions')
    if isinstance(functions, dict) and not functions:
        errors.append(
            f"ERROR: {label}.functions: project environment must declare "
            "at least one function"
        )
    if not isinstance(functions, dict):
        return errors

    callable_patterns = {
        'r': re.compile(
            r'^[A-Za-z][A-Za-z0-9.]*::[A-Za-z.][A-Za-z0-9._]*$'
        ),
        'python': re.compile(
            r'^(?:[A-Za-z_][A-Za-z0-9_]*\.)+'
            r'[A-Za-z_][A-Za-z0-9_]*$'
        ),
    }
    for name, contract in functions.items():
        if not isinstance(contract, dict):
            continue
        contract_path = f"{label}.functions.{name}"
        try:
            function_contract_fingerprint(name, contract)
        except (TypeError, ValueError, UnicodeError, struct.error) as exc:
            errors.append(
                f"ERROR: {contract_path}: cannot calculate canonical "
                f"contract fingerprint: {exc}"
            )
        comparison_decimals = contract.get('comparison_decimals', 4)
        if type(comparison_decimals) is int and comparison_decimals < 0:
            errors.append(
                f"ERROR: {contract_path}.comparison_decimals: must be "
                "non-negative"
            )

        parameters = contract.get('params')
        parameter_entries = parameters if isinstance(parameters, list) else []
        names = [
            parameter.get('name')
            for parameter in parameter_entries
            if isinstance(parameter, dict)
            and isinstance(parameter.get('name'), str)
        ]
        for parameter_name in sorted(set(names)):
            if names.count(parameter_name) > 1:
                errors.append(
                    f"ERROR: {contract_path}.params: duplicate parameter "
                    f"{parameter_name!r}"
                )
        for index, parameter in enumerate(parameter_entries):
            if not isinstance(parameter, dict):
                continue
            parameter_path = f"{contract_path}.params[{index}]"
            required = parameter.get('required', True)
            if required is False and 'default' not in parameter:
                errors.append(
                    f"ERROR: {parameter_path}.default: optional parameter "
                    "requires an environment default"
                )
            if required is True and 'default' in parameter:
                errors.append(
                    f"ERROR: {parameter_path}.default: required parameter "
                    "must not declare a default"
                )
            if 'default' in parameter and not function_value_matches(
                parameter['default'],
                parameter.get('type'),
                parameter.get('accepts_missing', False),
            ):
                actual = function_value_type(parameter['default'])
                errors.append(
                    f"ERROR: {parameter_path}.default: expected exact type "
                    f"{parameter.get('type')!r}, got {actual!r}"
                )

        binding = contract.get('binding')
        if isinstance(binding, dict):
            call = binding.get('call')
            pattern = callable_patterns.get(language)
            if (
                isinstance(call, str)
                and pattern is not None
                and pattern.fullmatch(call) is None
            ):
                errors.append(
                    f"ERROR: {contract_path}.binding.call: callable {call!r} "
                    f"is not fully qualified for runtime {language!r}"
                )
            binding_args = binding.get('args')
            if isinstance(binding_args, dict):
                missing = sorted(set(names) - set(binding_args))
                extra = sorted(set(binding_args) - set(names))
                if missing or extra:
                    errors.append(
                        f"ERROR: {contract_path}.binding.args: mapping must "
                        f"cover the closed signature exactly; missing={missing}, "
                        f"extra={extra}"
                    )
                host_names = list(binding_args.values())
                duplicates = sorted({
                    host_name for host_name in host_names
                    if host_names.count(host_name) > 1
                })
                if duplicates:
                    errors.append(
                        f"ERROR: {contract_path}.binding.args: duplicate host "
                        f"argument name(s): {', '.join(duplicates)}"
                    )
                for logical_name, host_name in binding_args.items():
                    if not valid_host_argument_name(language, host_name):
                        errors.append(
                            f"ERROR: {contract_path}.binding.args."
                            f"{logical_name}: host argument {host_name!r} is "
                            f"not a valid non-reserved {language} name"
                        )

        conformance = contract.get('conformance')
        if not isinstance(conformance, str):
            continue
        vector_path = environment_path.parent / conformance
        try:
            resolved_vector = vector_path.resolve()
            resolved_vector.relative_to(environment_path.parent.resolve())
        except (OSError, ValueError):
            errors.append(
                f"ERROR: {contract_path}.conformance: path must remain inside "
                "the selected project root"
            )
            continue
        if Path(conformance).is_absolute() or '..' in Path(conformance).parts:
            errors.append(
                f"ERROR: {contract_path}.conformance: path must be local and "
                "must not contain '..'"
            )
            continue
        if not resolved_vector.is_file():
            errors.append(
                f"ERROR: {contract_path}.conformance: file does not exist: "
                f"{conformance}"
            )
            continue
        try:
            with open(resolved_vector, 'r', encoding='utf-8') as handle:
                vectors = yaml.load(handle, Loader=UniqueKeyLoader)
        except (OSError, UnicodeError, yaml.YAMLError) as exc:
            errors.append(
                f"ERROR: {contract_path}.conformance: cannot read vectors: "
                f"{exc}"
            )
            continue
        vector_label = str(resolved_vector)
        vector_structure_errors = validate_type(
            vectors,
            ['function_conformance_class'],
            schema_env,
            vector_label,
        )
        errors.extend(vector_structure_errors)
        if vector_structure_errors:
            continue
        if not isinstance(vectors, dict):
            continue
        if vectors.get('schema_version') != schema_version:
            errors.append(
                f"ERROR: {vector_label}.schema_version: must match the "
                "project environment"
            )
        if vectors.get('function') != name:
            errors.append(
                f"ERROR: {vector_label}.function: expected {name!r}"
            )
        if vectors.get('contract_version') != contract.get('contract_version'):
            errors.append(
                f"ERROR: {vector_label}.contract_version: must match the "
                "logical contract"
            )
        cases = vectors.get('cases')
        if isinstance(cases, list) and not cases:
            errors.append(
                f"ERROR: {vector_label}.cases: at least one conformance case "
                "is required"
            )
        case_ids = [
            case.get('id') for case in cases or []
            if isinstance(case, dict) and isinstance(case.get('id'), str)
        ] if isinstance(cases, list) else []
        for case_id in sorted(set(case_ids)):
            if case_ids.count(case_id) > 1:
                errors.append(
                    f"ERROR: {vector_label}.cases: duplicate case id "
                    f"{case_id!r}"
                )
        covered = set()
        for index, case in enumerate(cases or []):
            if not isinstance(case, dict):
                continue
            case_path = f"{vector_label}.cases[{index}]"
            case_args = case.get('args')
            errors.extend(
                validate_function_arguments(
                    case_args, parameter_entries, f"{case_path}.args"
                )
            )
            result = case.get('result')
            short_circuits = False
            if isinstance(case_args, dict):
                by_name = {
                    parameter.get('name'): parameter
                    for parameter in parameter_entries
                    if isinstance(parameter, dict)
                }
                short_circuits = any(
                    value is None
                    and isinstance(by_name.get(argument_name), dict)
                    and not by_name[argument_name].get(
                        'accepts_missing', False
                    )
                    for argument_name, value in case_args.items()
                )
            if short_circuits and result is not None:
                errors.append(
                    f"ERROR: {case_path}.result: invalid_function_result: "
                    "a short-circuiting case must return missing"
                )
            elif not short_circuits and not function_value_matches(
                result,
                contract.get('returns'),
                contract.get('may_return_missing', False),
            ):
                errors.append(
                    f"ERROR: {case_path}.result: invalid_function_result: "
                    f"expected exact type {contract.get('returns')!r}, got "
                    f"{function_value_type(result)!r}"
                )
            coverage_errors, case_coverage = validate_case_coverage(
                case.get('covers'),
                case_args,
                result,
                contract,
                parameter_entries,
                short_circuits,
                f"{case_path}.covers",
            )
            errors.extend(coverage_errors)
            covered.update(case_coverage)
        required_coverage = required_conformance_coverage(
            contract, parameter_entries
        )
        missing_coverage = sorted(required_coverage - covered)
        if missing_coverage:
            errors.append(
                f"ERROR: {vector_label}.cases: missing required coverage: "
                f"{', '.join(missing_coverage)}"
            )
    return errors


def validate_repository_function_fingerprints(root, schema_env):
    """Require one logical contract per name/version across project roots."""
    errors = []
    examples_dir = root / 'yaml' / 'examples'
    if not examples_dir.is_dir() or schema_env is None:
        return errors
    seen = {}
    for environment_path in sorted(examples_dir.rglob('environment.yaml')):
        try:
            with open(environment_path, 'r', encoding='utf-8') as handle:
                document = yaml.load(handle, Loader=UniqueKeyLoader)
        except (OSError, UnicodeError, yaml.YAMLError):
            continue
        if validate_type(
            document,
            ['environment_class'],
            schema_env,
            str(environment_path),
        ):
            continue
        for name, contract in document['functions'].items():
            try:
                fingerprint = function_contract_fingerprint(name, contract)
            except (TypeError, ValueError, UnicodeError, struct.error):
                # Per-environment validation reports the malformed contract.
                continue
            identity = (name, contract['contract_version'])
            previous = seen.get(identity)
            if previous is None:
                seen[identity] = (fingerprint, environment_path)
                continue
            previous_fingerprint, previous_path = previous
            if fingerprint != previous_fingerprint:
                errors.append(
                    f"ERROR: {environment_path}.functions.{name}: "
                    "function_contract_mismatch: logical contract "
                    f"{name!r} version {identity[1]!r} has fingerprint "
                    f"{fingerprint}, but {previous_path} declares "
                    f"{previous_fingerprint}"
                )
    return errors


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
        for path in (f"datasets.{domain}", 'domain'):
            errors.append(
                validation_diagnostic(
                    f"{spec_label}.{path}",
                    'duplicate_identifier',
                    'dataset identifier must not equal the output domain',
                    context={'identifier': domain},
                )
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

    if isinstance(output, dict):
        declared_path = output.get('path')
        if isinstance(declared_path, str) and artifact_profile(output) is None:
            errors.append(
                f"ERROR: {spec_label}.output.path: "
                f"unknown_artifact_profile for {declared_path!r}; R020 maps "
                + ', '.join(sorted(ARTIFACT_PROFILES)) + " and nothing else"
            )

    if isinstance(output, dict) and 'decimals' in output:
        decimals = output.get('decimals')
        profile = artifact_profile(output)
        if isinstance(decimals, bool) or not isinstance(decimals, int):
            errors.append(
                f"ERROR: {spec_label}.output.decimals: must be a "
                f"non-negative integer, got {decimals!r}"
            )
        elif decimals < 0:
            errors.append(
                f"ERROR: {spec_label}.output.decimals: must be a "
                f"non-negative integer, got {decimals!r}"
            )
        if profile is not None and profile != 'csv':
            errors.append(
                f"ERROR: {spec_label}.output.decimals: "
                f"decimals_not_applicable under profile {profile!r}; R020 "
                "renders a display precision only for a csv artifact"
            )
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
                    for path in (f"keys[{index}]", 'output.columns'):
                        errors.append(
                            validation_diagnostic(
                                f"{spec_label}.{path}",
                                'internal_column_in_keys',
                                f"key column {key!r} is not in "
                                'output.columns',
                                context={'column': key},
                            )
                        )

    order_by = output.get('order_by') if isinstance(output, dict) else None
    if isinstance(order_by, list):
        order_variables = []
        for index, term in enumerate(order_by):
            variable = order_term_variable(term)
            if variable is None:
                continue
            order_variables.append(variable)
            if variable not in declared_columns:
                errors.append(
                    validation_diagnostic(
                        f"{spec_label}.output.order_by[{index}]",
                        'undeclared_column',
                        f"undeclared column {variable!r}",
                        context={'column': variable},
                    )
                )
        for variable in sorted(set(order_variables)):
            if order_variables.count(variable) > 1:
                errors.append(
                    validation_diagnostic(
                        f"{spec_label}.output.order_by",
                        'duplicate_order_term',
                        f"duplicate order term {variable!r}",
                        context={'column': variable},
                    )
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
                conflict_path = (
                    f"datasets.{lookup_id}"
                    if lookup_id in dataset_names
                    else 'domain'
                )
                for path in (
                    f"record_lookups[{index}].id", conflict_path
                ):
                    errors.append(
                        validation_diagnostic(
                            f"{spec_label}.{path}",
                            'duplicate_identifier',
                            f"identifier {lookup_id!r} conflicts with a "
                            'dataset or domain',
                            context={'identifier': lookup_id},
                        )
                    )
            lookup_dataset = lookup.get('dataset')
            if isinstance(lookup_dataset, str) and lookup_dataset not in dataset_names:
                errors.append(
                    f"ERROR: {spec_label}.record_lookups[{index}].dataset: "
                    f"undeclared dataset {lookup_dataset!r}"
                )

    return errors


def validate_column_labels(spec, spec_label):
    """Require every surviving declared column to carry a usable label."""
    errors = []
    columns = spec.get('columns')
    if not isinstance(columns, list):
        return errors
    for index, column in enumerate(columns):
        if not isinstance(column, dict):
            continue
        label = column.get('label')
        if isinstance(label, str) and label.strip():
            continue
        errors.append(
            f"ERROR: {spec_label}.columns[{index}].label: declared column "
            "requires a non-empty label"
        )
    return errors


# R021 project resource resolution. A declared project path is confined to the
# approved project root, its written form is fixed before the filesystem is
# consulted, and every accepted physical file is read once as one immutable
# byte snapshot.

RESOURCE_PATH_MESSAGES = {
    'resource_path_not_relative': 'is not a relative project path',
    'resource_path_uri_scheme': 'declares a URI scheme',
    'resource_path_parent_traversal': 'traverses a parent segment',
    'resource_path_not_normalized': 'is not normalized',
    'resource_path_symlink': 'passes through a symbolic link',
    'resource_path_outside_project': 'resolves outside the project root',
    'resource_path_missing': 'does not exist',
    'resource_path_not_regular_file': 'is not a regular file',
    'resource_path_content_changed': 'changed after it was validated',
}

URI_SCHEME_PATTERN = re.compile(r'^[A-Za-z][A-Za-z0-9+.-]*:')
DRIVE_LETTER_PATTERN = re.compile(r'^[A-Za-z]:')


def classify_written_project_path(written):
    """Return the R021 condition a written project path violates, if any."""
    if not isinstance(written, str) or not written:
        return 'resource_path_not_relative'
    if written.startswith('/') or '\\' in written:
        return 'resource_path_not_relative'
    if DRIVE_LETTER_PATTERN.match(written):
        return 'resource_path_not_relative'
    if URI_SCHEME_PATTERN.match(written):
        return 'resource_path_uri_scheme'
    segments = written.split('/')
    if '..' in segments:
        return 'resource_path_parent_traversal'
    if any(segment in ('', '.') for segment in segments):
        return 'resource_path_not_normalized'
    return None


def resolve_project_path(written, base_dir, project_root):
    """Walk a written project path under R021.

    Returns the accepted path and no condition, or no path and the stable
    condition that rejected it. Nothing about the host is returned.
    """
    condition = classify_written_project_path(written)
    if condition is not None:
        return None, condition

    try:
        root = Path(project_root).resolve(strict=True)
    except OSError:
        return None, 'resource_path_outside_project'

    segments = written.split('/')
    current = Path(base_dir)
    for index, segment in enumerate(segments):
        current = current / segment
        if current.is_symlink():
            return None, 'resource_path_symlink'
        if not current.exists():
            return None, 'resource_path_missing'
        is_last = index == len(segments) - 1
        if is_last:
            if not current.is_file():
                return None, 'resource_path_not_regular_file'
        elif not current.is_dir():
            return None, 'resource_path_not_regular_file'

    try:
        current.resolve(strict=True).relative_to(root)
    except (OSError, ValueError):
        return None, 'resource_path_outside_project'
    return current, None


def resource_path_error(path, written, condition):
    """Report one R021 rejection without naming a host location."""
    message = RESOURCE_PATH_MESSAGES[condition]
    rendered = f"ERROR: {path}: {condition}: {written!r} {message}"
    return ValidationDiagnostic(
        path,
        condition,
        f"{written!r} {message}",
        context={'path': written},
        rendered=rendered,
    )


class ProjectSnapshot:
    """One immutable byte snapshot of one accepted physical file."""

    def __init__(self, digest, content):
        self.digest = digest
        self.content = content

    def csv_header(self, delimiter=','):
        """Return the artifact header read from this snapshot's bytes."""
        text = self.content.decode('utf-8')
        reader = csv.reader(
            io.StringIO(text, newline=''), delimiter=delimiter, strict=True
        )
        return next(reader, [])


class ProjectSnapshots:
    """The byte snapshots one validation run has accepted.

    A file is opened once and read through that same handle, so no check is
    made against one path and then used against another. Two declarations
    that reach one physical file share the snapshot, and content that changed
    after the first read fails the run.
    """

    def __init__(self):
        self._by_identity = {}
        self.reads = 0

    def read(self, path):
        """Return (snapshot, condition) for an already accepted path."""
        try:
            with open(path, 'rb') as handle:
                status = os.fstat(handle.fileno())
                if not stat.S_ISREG(status.st_mode):
                    return None, 'resource_path_not_regular_file'
                content = handle.read()
        except OSError:
            return None, 'resource_path_missing'

        identity = (status.st_dev, status.st_ino)
        digest = hashlib.sha256(content).hexdigest()
        accepted = self._by_identity.get(identity)
        if accepted is None:
            self.reads += 1
            snapshot = ProjectSnapshot(digest, content)
            self._by_identity[identity] = snapshot
            return snapshot, None
        if accepted.digest != digest:
            return None, 'resource_path_content_changed'
        return accepted, None


def validate_spec_contracts(
    spec, spec_label, spec_path=None, project_root=None, snapshots=None,
):
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
                declared_fields = ['source'] if has_source else ['key']
                missing = ['key'] if has_source else ['source']
                errors.append(
                    validation_diagnostic(
                        path,
                        'unpaired_fields',
                        'source and key must be declared together',
                        context={
                            'record_lookup': lookup.get('id'),
                            'declared': declared_fields,
                            'missing': missing,
                        },
                    )
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
                has_order = 'order_by' in lookup
                errors.append(
                    validation_diagnostic(
                        path,
                        'unpaired_fields',
                        'order_by and keep must be declared together',
                        context={
                            'record_lookup': lookup.get('id'),
                            'declared': ['order_by'] if has_order else ['keep'],
                            'missing': ['keep'] if has_order else ['order_by'],
                        },
                    )
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
            if keyword in {'all_or_none', 'implies', 'predicate', 'row_count'}:
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
                if 'group_by' in payload:
                    group_by = payload.get('group_by')
                    if not isinstance(payload.get('id'), str):
                        errors.append(
                            validation_diagnostic(
                                path,
                                'missing_verification_id',
                                'a grouped row_count requires a verification '
                                'id',
                            )
                        )
                    if isinstance(group_by, list):
                        if not group_by:
                            errors.append(
                                f"ERROR: {path}.group_by: requires at least "
                                "one column"
                            )
                        for name in sorted(
                            {
                                name for name in group_by
                                if isinstance(name, str)
                                and group_by.count(name) > 1
                            }
                        ):
                            errors.append(
                                f"ERROR: {path}.group_by: duplicate column "
                                f"{name!r}"
                            )
                        for name in group_by:
                            if isinstance(name, str) and name not in declared:
                                errors.append(
                                    f"ERROR: {path}.group_by: unknown column "
                                    f"{name!r}"
                                )

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
        if project_root is None:
            project_root = spec_path.parent
        if snapshots is None:
            snapshots = ProjectSnapshots()
        for dataset_id, source in datasets.items():
            source_path = source if isinstance(source, str) else None
            types = None
            if isinstance(source, dict):
                source_path = source.get('path')
                types = source.get('types')
            if not isinstance(source_path, str):
                continue
            path = f"{spec_label}.datasets.{dataset_id}"
            resolved, condition = resolve_project_path(
                source_path, spec_path.parent, project_root
            )
            if condition is None:
                snapshot, condition = snapshots.read(resolved)
            if condition is not None:
                errors.append(
                    resource_path_error(f"{path}.path", source_path, condition)
                )
                continue
            if resolved.suffix.lower() != '.csv' or not isinstance(types, dict):
                continue
            try:
                header = snapshot.csv_header()
            except (UnicodeError, csv.Error) as exc:
                errors.append(f"ERROR: {path}: cannot read CSV header: {exc}")
                continue
            for field in sorted(set(types) - set(header)):
                errors.append(
                    validation_diagnostic(
                        f"{path}.types.{field}",
                        'unknown_field',
                        f"field is absent from {source_path}",
                        context={
                            'dataset': dataset_id,
                            'field': field,
                        },
                    )
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


def dataset_type_catalog(spec, spec_path, env=None):
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

        if (
            isinstance(producer_path, str)
            and spec_path is not None
            and classify_written_project_path(producer_path) is None
        ):
            resolved = spec_path.parent / producer_path
            try:
                with open(resolved, 'r', encoding='utf-8') as handle:
                    producer = yaml.load(handle, Loader=UniqueKeyLoader)
                if env is not None:
                    producer, resolution_errors, _ = prepare_spec_document(
                        producer, str(resolved), resolved, env
                    )
                    if resolution_errors:
                        producer = None
                if not isinstance(producer, dict):
                    raise ValueError('producer did not resolve to a mapping')
                fields.update(specification_column_types(producer))
            except (OSError, ValueError, yaml.YAMLError):
                pass

        if (
            isinstance(source_path, str)
            and spec_path is not None
            and source_path.lower().endswith(('.csv', '.tsv'))
            and classify_written_project_path(source_path) is None
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
            except (OSError, UnicodeError, csv.Error):
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


def iter_function_calls(value, path):
    """Yield function payloads and stable paths from a derivation tree."""
    if isinstance(value, dict):
        if len(value) == 1 and 'function' in value:
            yield value['function'], f"{path}.function"
            return
        for key, child in value.items():
            yield from iter_function_calls(child, f"{path}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            yield from iter_function_calls(child, f"{path}[{index}]")


def validate_spec_functions(spec, spec_label, spec_path, schema_env):
    """Validate calls against R018 when an implementation is supplied."""
    calls = []
    columns = spec.get('columns')
    column_types = specification_column_types(spec)
    if isinstance(columns, list):
        for index, column in enumerate(columns):
            if not isinstance(column, dict) or 'derivation' not in column:
                continue
            name = column.get('name', index)
            expected = column.get('type')
            for payload, path in iter_function_calls(
                column['derivation'],
                f"{spec_label}.columns.{name}.derivation",
            ):
                calls.append((payload, path, expected))
    rows = spec.get('rows')
    if isinstance(rows, list):
        for index, row in enumerate(rows):
            derivations = row.get('derivations') if isinstance(row, dict) else None
            if not isinstance(derivations, dict):
                continue
            for name, derivation in derivations.items():
                expected = column_types.get(name)
                for payload, path in iter_function_calls(
                    derivation,
                    f"{spec_label}.rows[{index}].derivations.{name}",
                ):
                    calls.append((payload, path, expected))

    environment_path = spec_path.parent / 'environment.yaml'
    if not environment_path.exists():
        # A portable specification can declare logical calls before a project
        # supplies their implementation. R018 requires this environment when
        # project code is validated, activated, or executed.
        return []
    if not environment_path.is_file():
        return [
            f"ERROR: {spec_label}: project environment path is not a file: "
            f"{environment_path}"
        ]
    if environment_path.is_symlink():
        return [
            f"ERROR: {environment_path}: project environment must not be a "
            "symlink"
        ]
    try:
        with open(environment_path, 'r', encoding='utf-8') as handle:
            project_environment = yaml.load(handle, Loader=UniqueKeyLoader)
    except (OSError, UnicodeError, yaml.YAMLError) as exc:
        return [f"ERROR: {environment_path}: cannot read environment: {exc}"]

    schema_root = schema_env.get('root')
    root = (
        Path(schema_root)
        if isinstance(schema_root, str)
        else Path(__file__).resolve().parents[2]
    )
    environment_schema, schema_errors = build_schema_env(
        root, 'schema_environment.yaml'
    )
    if schema_errors:
        return schema_errors
    errors = validate_project_environment(
        project_environment,
        str(environment_path),
        environment_path,
        environment_schema,
    )
    if not isinstance(project_environment, dict):
        return errors
    functions = project_environment.get('functions')
    if not isinstance(functions, dict):
        return errors

    datasets = dataset_type_catalog(spec, spec_path, schema_env)
    lookups = {}
    lookup_entries = spec.get('record_lookups')
    if isinstance(lookup_entries, list):
        for lookup in lookup_entries:
            if not isinstance(lookup, dict):
                continue
            lookup_id = lookup.get('id')
            dataset_id = lookup.get('dataset')
            if isinstance(lookup_id, str) and isinstance(dataset_id, str):
                lookups[lookup_id] = datasets.get(dataset_id, {})

    def resolve_variable(name):
        if not isinstance(name, str):
            return None
        if '.' not in name:
            return column_types.get(name)
        qualifier, field = name.split('.', 1)
        relation = datasets.get(qualifier, lookups.get(qualifier))
        return relation.get(field) if isinstance(relation, dict) else None

    for payload, path, _expected_return in calls:
        if not isinstance(payload, dict):
            continue
        name = payload.get('name')
        if not isinstance(name, str) or name not in functions:
            errors.append(
                f"ERROR: {path}.name: unknown_project_function: {name!r}"
            )
            continue
        contract = functions[name]
        if not isinstance(contract, dict):
            continue
        requested = payload.get('contract_version')
        available = contract.get('contract_version')
        if requested != available:
            errors.append(
                validation_diagnostic(
                    f"{path}.contract_version",
                    'function_contract_mismatch',
                    f"requested {requested!r}, environment provides "
                    f"{available!r}",
                    context={
                        'function': name,
                        'requested': requested,
                        'available': available,
                    },
                )
            )
        errors.extend(
            validate_function_arguments(
                payload.get('args', {}),
                contract.get('params', []),
                f"{path}.args",
                resolve_variable,
            )
        )
    return errors


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
        return [
            validation_diagnostic(
                path,
                'invalid_predicate',
                f"invalid predicate: {exc}",
                context={'predicate': text},
                span=(exc.position, min(len(text), exc.position + 1)),
            )
        ]
    return [
        validation_diagnostic(
            path,
            issue.condition,
            str(issue),
            context=issue.context,
            span=issue.span,
        )
        for issue in dict.fromkeys(validate_predicate_types(ast, resolver))
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


def validate_spec_predicates(spec, spec_label, spec_path=None, env=None):
    """Parse, resolve, and type-check every R004 predicate in a spec."""
    errors = []
    datasets = dataset_type_catalog(spec, spec_path, env)
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
                else ('filter',) if keyword == 'row_count'
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


def numeric_identifier_resolver(unqualified=None, qualified=None):
    unqualified = unqualified or {}
    qualified = qualified or {}

    def resolve(name):
        if '.' not in name:
            if name not in unqualified:
                return None, (
                    'unknown_field',
                    f"unknown identifier {name!r}",
                    {'identifier': name},
                )
            return unqualified[name], None

        qualifier, field = name.split('.', 1)
        relation = qualified.get(qualifier)
        if relation is None:
            return None, (
                'qualified_identifier',
                f"qualified identifier {name!r} is not available here",
                {'identifier': name},
            )
        if field not in relation:
            return None, (
                'unknown_field',
                f"unknown field {field!r} in {qualifier!r}",
                {'identifier': name},
            )
        return relation[field], None

    return resolve


def validate_numeric_expression_at(text, path, resolver):
    try:
        ast = parse_numeric_expression(text)
    except NumericExpressionError as exc:
        return [
            validation_diagnostic(
                path,
                exc.condition,
                exc.message,
                context={'expr': text, **exc.context},
                span=exc.span,
            )
        ]
    _, errors = validate_numeric_expression_ast(ast, path, text, resolver)
    return errors


def validate_expression_numeric(expression, path, resolver):
    errors = []
    if not isinstance(expression, dict) or len(expression) != 1:
        return errors
    keyword, payload = next(iter(expression.items()))

    if keyword == 'compute' and isinstance(payload, dict):
        text = payload.get('expr')
        if isinstance(text, str):
            errors.extend(
                validate_numeric_expression_at(
                    text, f"{path}.compute.expr", resolver
                )
            )
        return errors

    if keyword == 'case' and isinstance(payload, dict):
        branches = payload.get('branches')
        if isinstance(branches, list):
            for index, branch in enumerate(branches):
                if not isinstance(branch, dict):
                    continue
                errors.extend(
                    validate_expression_numeric(
                        branch.get('then'),
                        f"{path}.case.branches[{index}].then",
                        resolver,
                    )
                )
        if 'otherwise' in payload:
            errors.extend(
                validate_expression_numeric(
                    payload['otherwise'],
                    f"{path}.case.otherwise",
                    resolver,
                )
            )
    elif keyword == 'str_concat' and isinstance(payload, dict):
        sources = payload.get('sources')
        if isinstance(sources, list):
            for index, source in enumerate(sources):
                errors.extend(
                    validate_expression_numeric(
                        source,
                        f"{path}.str_concat.sources[{index}]",
                        resolver,
                    )
                )
    return errors


def validate_derivation_numeric(derivation, path, resolver):
    if not isinstance(derivation, dict):
        return []
    if 'value' not in derivation:
        return validate_expression_numeric(derivation, path, resolver)

    errors = validate_expression_numeric(
        derivation.get('value'), f"{path}.value", resolver
    )
    overrides = derivation.get('override')
    if isinstance(overrides, list):
        for index, override in enumerate(overrides):
            if not isinstance(override, dict):
                continue
            errors.extend(
                validate_expression_numeric(
                    override.get('value'),
                    f"{path}.override[{index}].value",
                    resolver,
                )
            )
    return errors


def validate_spec_numeric_expressions(
    spec, spec_label, spec_path=None, env=None
):
    """Parse, resolve, and type-check every R010 expression in a spec."""
    errors = []
    datasets = dataset_type_catalog(spec, spec_path, env)
    output_types = specification_column_types(spec)
    lookups = {}
    lookup_entries = spec.get('record_lookups')
    if isinstance(lookup_entries, list):
        for lookup in lookup_entries:
            if not isinstance(lookup, dict):
                continue
            lookup_id = lookup.get('id')
            dataset_id = lookup.get('dataset')
            if isinstance(lookup_id, str) and isinstance(dataset_id, str):
                lookups[lookup_id] = datasets.get(dataset_id, {})

    column_resolver = numeric_identifier_resolver(
        unqualified=output_types, qualified=lookups
    )
    columns = spec.get('columns')
    if isinstance(columns, list):
        for index, column in enumerate(columns):
            if not isinstance(column, dict) or 'derivation' not in column:
                continue
            name = column.get('name', index)
            errors.extend(
                validate_derivation_numeric(
                    column['derivation'],
                    f"{spec_label}.columns.{name}.derivation",
                    column_resolver,
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
            if isinstance(row.get('group_by'), list):
                grouped_fields = {
                    variable.split('.', 1)[1]: driver_fields.get(
                        variable.split('.', 1)[1]
                    )
                    for variable in row['group_by']
                    if (
                        isinstance(variable, str)
                        and isinstance(driver, str)
                        and variable.startswith(driver + '.')
                        and variable.split('.', 1)[1] in driver_fields
                    )
                }
                qualified = {driver: grouped_fields}
            else:
                qualified = (
                    {driver: driver_fields}
                    if isinstance(driver, str)
                    else {}
                )
            derivations = row.get('derivations')
            row_output = {
                name: output_types[name]
                for name in derivations or {}
                if name in output_types
            } if isinstance(derivations, dict) else {}
            row_resolver = numeric_identifier_resolver(
                unqualified=row_output, qualified=qualified
            )
            if isinstance(derivations, dict):
                for name, derivation in derivations.items():
                    errors.extend(
                        validate_derivation_numeric(
                            derivation,
                            f"{spec_label}.rows[{index}].derivations.{name}",
                            row_resolver,
                        )
                    )
    return errors


def prepare_spec_document(spec, spec_label, spec_path, env):
    if isinstance(spec, dict) and 'parents' in spec:
        return resolve_spec_inheritance(spec, spec_label, spec_path, env)
    return copy.deepcopy(spec), [], {}


def inherited_error_logical_path(path, spec):
    identities = {
        'record_lookups': 'id',
        'columns': 'name',
        'rows': 'id',
    }
    for collection, identity in identities.items():
        match = re.match(rf'^{collection}\[([0-9]+)\]', path)
        values = spec.get(collection) if isinstance(spec, dict) else None
        if match is None or not isinstance(values, list):
            continue
        index = int(match.group(1))
        if index >= len(values) or not isinstance(values[index], dict):
            continue
        member_id = values[index].get(identity)
        if isinstance(member_id, str):
            return (
                f"{collection}.{member_id}" + path[match.end():]
            )
    return path


def inherited_error_provenance(
    error, spec_label, provenance, spec=None, spec_path=None
):
    if not provenance or not error.startswith(f"ERROR: {spec_label}."):
        return error
    remainder = error[len(f"ERROR: {spec_label}."):]
    path = remainder.split(':', 1)[0]
    normalized = inherited_error_logical_path(path, spec)
    candidates = [
        key for key in provenance
        if normalized == key or normalized.startswith(key + '.')
    ]
    if not candidates:
        return error
    source = provenance[max(candidates, key=len)]
    if (
        source == spec_label
        or (
            spec_path is not None
            and source == str(spec_path.resolve())
        )
    ):
        return error
    return f"{error} (contributed by {source})"


def validate_spec_document(
    spec, spec_label, spec_path, env, spec_stack=None, project_root=None,
    snapshots=None,
):
    """Validate one complete specification and its producer dependencies."""
    if not isinstance(spec, dict) or not spec:
        return [
            f"ERROR: {spec_label}: spec is empty or not a mapping"
        ]

    has_inheritance = 'parents' in spec
    if project_root is None:
        project_root = spec_path.parent
    if snapshots is None:
        snapshots = ProjectSnapshots()
    spec, errors, provenance = prepare_spec_document(
        spec, spec_label, spec_path, env
    )
    if errors or not isinstance(spec, dict):
        return errors

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
    if has_inheritance:
        errors.extend(validate_column_labels(spec, spec_label))
    errors.extend(
        validate_spec_contracts(
            spec, spec_label, spec_path, project_root, snapshots
        )
    )
    errors.extend(validate_spec_predicates(spec, spec_label, spec_path, env))
    errors.extend(
        validate_spec_numeric_expressions(spec, spec_label, spec_path, env)
    )
    errors.extend(validate_spec_functions(spec, spec_label, spec_path, env))

    next_stack = set(spec_stack or ())
    next_stack.add(spec_path.resolve())
    errors.extend(
        validate_producing_specs(
            spec, spec_label, spec_path, env, next_stack, project_root,
            snapshots,
        )
    )
    return [
        inherited_error_provenance(
            error, spec_label, provenance, spec, spec_path
        )
        for error in errors
    ]


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


def validate_producing_specs(
    spec, spec_label, spec_path, env, spec_stack, project_root=None,
    snapshots=None,
):
    """Validate producer workflow edges and referenced artifact headers."""
    errors = []
    datasets = spec.get('datasets')
    if not isinstance(datasets, dict):
        return errors
    if project_root is None:
        project_root = spec_path.parent
    if snapshots is None:
        snapshots = ProjectSnapshots()

    for dataset_id, source in datasets.items():
        if not isinstance(source, dict) or 'schema' not in source:
            continue

        path = f"{spec_label}.datasets.{dataset_id}"
        if 'types' in source:
            types = source['types']
            if isinstance(types, dict) and types:
                for field in sorted(types, key=str):
                    errors.append(
                        validation_diagnostic(
                            f"{path}.types.{field}",
                            'redundant_field_type',
                            'field type is already supplied by the producing '
                            'specification',
                            context={
                                'dataset': dataset_id,
                                'field': field,
                                'type': types[field],
                            },
                        )
                    )
            else:
                errors.append(
                    f"ERROR: {path}.types: inline types cannot be combined "
                    "with a producing specification"
                )

        schema_ref = source.get('schema')
        if not isinstance(schema_ref, str):
            continue
        producer_path, condition = resolve_project_path(
            schema_ref, spec_path.parent, project_root
        )
        if condition is not None:
            errors.append(
                resource_path_error(f"{path}.schema", schema_ref, condition)
            )
            continue
        if producer_path.resolve() in spec_stack:
            errors.append(
                f"ERROR: {path}.schema: producer workflow dependency cycle "
                f"through {schema_ref}"
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

        producer_entry = producer
        producer, resolution_errors, _ = prepare_spec_document(
            producer_entry, f"{path}.schema", producer_path, env
        )
        if resolution_errors or not isinstance(producer, dict):
            errors.extend(resolution_errors)
            continue
        producer_errors = validate_spec_document(
            producer_entry,
            f"{path}.schema",
            producer_path,
            env,
            spec_stack,
            project_root,
            snapshots,
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
        source_path, condition = resolve_project_path(
            source_ref, spec_path.parent, project_root
        )
        if condition is not None:
            continue
        if source_path.suffix.lower() != '.csv':
            continue
        snapshot, condition = snapshots.read(source_path)
        if condition is not None:
            continue
        try:
            header = snapshot.csv_header()
        except (UnicodeError, csv.Error) as exc:
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


def expected_resolved_path(example_dir, spec_path):
    suffix = spec_path.stem.removeprefix('spec')
    return example_dir / 'expected' / f"resolved{suffix}.yaml"


def validate_expected_resolved_fixture(
    spec, spec_label, spec_path, example_dir, env
):
    """Compare an inherited positive example with its resolved YAML tree."""
    if not isinstance(spec, dict) or 'parents' not in spec:
        return []
    resolved, resolution_errors, _ = prepare_spec_document(
        spec, spec_label, spec_path, env
    )
    if resolution_errors or not isinstance(resolved, dict):
        return []
    expected_path = expected_resolved_path(example_dir, spec_path)
    if not expected_path.is_file():
        return [
            f"ERROR: {spec_label}: inherited positive example requires "
            f"{expected_path.name}"
        ]
    try:
        with open(expected_path, 'r', encoding='utf-8') as handle:
            expected = yaml.load(handle, Loader=UniqueKeyLoader)
    except (OSError, UnicodeError, yaml.YAMLError) as exc:
        return [
            f"ERROR: {expected_path}: cannot read resolved fixture: {exc}"
        ]
    if resolved != expected:
        return [
            f"ERROR: {spec_label}: resolved specification differs from "
            f"expected/{expected_path.name}"
        ]
    return []


def load_validation_manifest(root: Path):
    path = root / 'yaml' / 'examples' / 'validation-manifest.yaml'
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
    examples_dir = root / 'yaml' / 'examples'
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
    label = 'yaml/examples/validation-manifest.yaml'
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

    manifest_path = f"yaml/examples/validation-manifest.yaml.fixtures.{name}"
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


def validate_examples_structure(root: Path, env, warnings=None, manifest=None):
    errors = []
    if warnings is None:
        warnings = []
    if env is None:
        return errors

    examples_dir = root / 'yaml' / 'examples'
    if not examples_dir.exists():
        return errors

    manifest_fixtures = (
        manifest.get('fixtures', {}) if isinstance(manifest, dict) else {}
    )
    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith('.'):
            continue

        example_errors = []
        spec_labels = []
        for spec_path in example_spec_paths(ex_dir):
            try:
                with open(spec_path, 'r', encoding='utf-8') as f:
                    spec = yaml.load(f, Loader=UniqueKeyLoader)
            except Exception:
                continue

            spec_label = f"{ex_dir.name}/{spec_path.name}"
            spec_labels.append(spec_label)
            is_negative = ex_dir.name.startswith('negative-')
            spec_errors = validate_spec_document(
                spec, spec_label, spec_path, env
            )
            if not is_negative:
                errors.extend(spec_errors)
                errors.extend(
                    validate_expected_resolved_fixture(
                        spec, spec_label, spec_path, ex_dir, env
                    )
                )
                continue

            example_errors.extend(spec_errors)

        if not ex_dir.name.startswith('negative-'):
            continue

        error_yaml_path = ex_dir / 'expected' / 'error.yaml'
        if not error_yaml_path.exists():
            errors.extend(example_errors)
            continue

        try:
            with open(error_yaml_path, 'r', encoding='utf-8') as f:
                err_spec = yaml.load(f, Loader=UniqueKeyLoader)
        except Exception:
            errors.extend(example_errors)
            continue
        if not (
            isinstance(err_spec, dict)
            and err_spec.get('phase') == 'validation'
        ):
            errors.extend(example_errors)
            continue

        entry = manifest_fixtures.get(ex_dir.name)
        if isinstance(entry, dict):
            errors.extend(
                validate_registered_fixture_diagnostics(
                    ex_dir.name, entry, example_errors, spec_labels
                )
            )
            continue

        # Compatibility path for focused unit tests without a repository
        # validation manifest.
        expected_paths = err_spec.get('spec_paths', [])
        if not isinstance(expected_paths, list):
            expected_paths = [expected_paths]
        for err in example_errors:
            parts = err.split(': ', 2)
            if len(parts) < 2:
                errors.append(err)
                continue
            path_part = parts[1]
            norm_path = path_part
            for spec_label in spec_labels:
                prefix = f"{spec_label}."
                if norm_path.startswith(prefix):
                    norm_path = norm_path[len(prefix):]
                    break
            if not any(
                norm_path == expected_path
                or norm_path.startswith(f"{expected_path}.")
                or norm_path.startswith(f"{expected_path}[")
                for expected_path in expected_paths
            ):
                errors.append(err)

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


BOM_UTF8 = '\ufeff'
CANONICAL_INT = re.compile(r'0|-?[1-9][0-9]*')


def parse_csv_profile(data: str):
    """Parse R020's csv profile text into records of (text, quoted) fields.

    A bare empty field is missing and parses to a text of None; a quoted
    empty field is the collected empty string. Raises ValueError for text
    the profile does not admit.
    """
    records = []
    record = []
    index = 0
    size = len(data)
    while index < size:
        if data[index] == '"':
            index += 1
            chunks = []
            while True:
                if index >= size:
                    raise ValueError('unterminated quoted field')
                character = data[index]
                if character == '"':
                    if data[index + 1:index + 2] == '"':
                        chunks.append('"')
                        index += 2
                        continue
                    index += 1
                    break
                chunks.append(character)
                index += 1
            field = (''.join(chunks), True)
        else:
            start = index
            while index < size and data[index] not in ',\n':
                index += 1
            raw = data[start:index]
            if '"' in raw:
                raise ValueError('a bare field carries U+0022')
            if '\r' in raw:
                raise ValueError('U+000D outside a quoted field')
            field = (raw or None, False)
        record.append(field)
        if index >= size:
            raise ValueError('the final record is not terminated by U+000A')
        if data[index] == ',':
            index += 1
            continue
        if data[index] != '\n':
            raise ValueError('a quoted field is followed by ordinary text')
        index += 1
        records.append(record)
        record = []
    if record:
        raise ValueError('the final record is not terminated by U+000A')
    return records


def render_csv_profile(records):
    """Render records back under R020's exact quoting condition."""
    lines = []
    for record in records:
        fields = []
        for text, _quoted in record:
            if text is None:
                fields.append('')
            elif text == '' or any(c in text for c in '",\r\n'):
                fields.append('"' + text.replace('"', '""') + '"')
            else:
                fields.append(text)
        lines.append(','.join(fields) + '\n')
    return ''.join(lines)


def canonical_float_text(value: str, decimals=None):
    """Return why value is not R020's text for its float, or None.

    Static validation reads a golden file rather than running a derivation,
    so it checks the form of the text and not the value behind it. With no
    declared precision that is the whole contract, because R011's shortest
    round-trip text is unique per value. With a declared precision it is the
    written width: proving that the digits are the ones the derivation would
    have produced needs the executable suite.
    """
    try:
        number = float(value)
    except ValueError:
        return 'not a number'
    if math.isnan(number) or math.isinf(number):
        return 'a non-finite float is the missing value'
    if decimals is None:
        # repr supplies the shortest round-tripping digits but places them in
        # exponential notation past its own thresholds; R011 writes the same
        # digits positionally, so one value keeps one spelling.
        canonical = format(decimal.Decimal(repr(number)), 'f')
        if canonical.endswith('.0'):
            canonical = canonical[:-2]
        if value != canonical:
            return f'expected the shortest round-trip text {canonical}'
        return None
    # Rounding reads the exact binary64 value, which for an extreme magnitude
    # or a large declared precision needs more digits than the default context.
    exact = decimal.Decimal(number)
    try:
        with decimal.localcontext() as context:
            context.prec = max(28, exact.adjusted() + decimals + 3)
            rounded = exact.quantize(
                decimal.Decimal(1).scaleb(-decimals),
                rounding=decimal.ROUND_HALF_UP,
            )
    except decimal.InvalidOperation:
        return f'cannot be rendered at decimals {decimals}'
    if not rounded:
        rounded = rounded.copy_abs()
    if value != format(rounded, 'f'):
        return (
            f'expected exactly {decimals} digit(s) after the decimal point '
            'in positional notation'
        )
    return None


CANONICAL_DATE = re.compile(r'(\d{4})-(\d{2})-(\d{2})')
CANONICAL_DATETIME = re.compile(
    r'(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})'
)


def canonical_temporal_text(value: str, declared: str):
    """Return why value is not R016's canonical text for its type, or None.

    R016 fixes exactly one written form per temporal type, so the check is the
    shape and then the calendar: a zone, an offset, a fractional second, a
    truncated form, and an unpadded field are all outside the grammar, and a
    field combination no calendar admits fails even when the shape matches.
    """
    if declared == 'date':
        match = CANONICAL_DATE.fullmatch(value)
        if match is None:
            return 'expected YYYY-MM-DD'
        try:
            dt.date(*(int(part) for part in match.groups()))
        except ValueError as exc:
            return f'not a date on the calendar: {exc}'
        return None

    match = CANONICAL_DATETIME.fullmatch(value)
    if match is None:
        return 'expected YYYY-MM-DDThh:mm:ss'
    try:
        dt.datetime(*(int(part) for part in match.groups()))
    except ValueError as exc:
        return f'not a moment on the calendar: {exc}'
    return None


def validate_csv_artifact(csv_path: Path, label: str, spec):
    """Check one expected artifact against R020's csv profile."""
    errors = []
    try:
        raw = csv_path.read_bytes()
    except OSError as exc:
        return [f"ERROR: {label}: cannot read artifact: {exc}"]
    try:
        data = raw.decode('utf-8')
    except UnicodeError as exc:
        return [f"ERROR: {label}: invalid_text: {exc}"]
    if data.startswith(BOM_UTF8):
        return [f"ERROR: {label}: csv carries a byte-order mark"]
    try:
        records = parse_csv_profile(data)
    except ValueError as exc:
        return [f"ERROR: {label}: csv: {exc}"]
    if not records:
        return [f"ERROR: {label}: csv carries no header record"]
    if render_csv_profile(records) != data:
        errors.append(
            f"ERROR: {label}: csv quoting is not the exact condition R020 "
            "states, or a record is not terminated by U+000A"
        )

    output = spec.get('output') if isinstance(spec, dict) else None
    output = output if isinstance(output, dict) else {}
    decimals = output.get('decimals')
    if isinstance(decimals, bool) or not isinstance(decimals, int):
        decimals = None
    types = {}
    for column in (spec.get('columns') if isinstance(spec, dict) else None) or []:
        if isinstance(column, dict) and isinstance(column.get('name'), str):
            types[column['name']] = column.get('type')

    header = [text for text, _quoted in records[0]]
    for number, record in enumerate(records[1:], 2):
        for name, (text, _quoted) in zip(header, record):
            if text is None:
                continue
            declared = types.get(name)
            if declared == 'int':
                if not CANONICAL_INT.fullmatch(text):
                    errors.append(
                        f"ERROR: {label}: record {number}: {name} is not "
                        f"R020's int text: {text!r}"
                    )
            elif declared == 'float':
                problem = canonical_float_text(text, decimals)
                if problem is not None:
                    errors.append(
                        f"ERROR: {label}: record {number}: {name} is not "
                        f"R020's float text: {text!r}, {problem}"
                    )
            elif declared in ('date', 'datetime'):
                problem = canonical_temporal_text(text, declared)
                if problem is not None:
                    errors.append(
                        f"ERROR: {label}: record {number}: {name} is not "
                        f"R016's {declared} text: {text!r}, {problem}"
                    )
            if len(errors) >= 6:
                errors.append(f"ERROR: {label}: further csv errors elided")
                return errors
    return errors


ARTIFACT_PROFILES = {'.csv': 'csv', '.parquet': 'parquet'}


def artifact_profile(output):
    """Return R020's profile for a declared output.path, or None.

    The mapping is closed: an extension outside it names no profile, which is
    a validation failure rather than a fallback to a default.
    """
    if not isinstance(output, dict):
        return None
    declared = output.get('path')
    if not isinstance(declared, str):
        return None
    return ARTIFACT_PROFILES.get(PurePosixPath(declared).suffix.lower())


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
        except UnicodeError as exc:
            errors.append(f"ERROR: {label}: invalid_text: {exc}")
            continue
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
    r'|R0[0-9][0-9]|output\.columns',
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
            except (OSError, UnicodeError, csv.Error):
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


ASCII_SOURCE_SUFFIXES = {
    '.csv', '.json', '.md', '.py', '.r', '.rb', '.rd', '.sh', '.toml',
    '.txt', '.yaml', '.yml',
}
ASCII_SOURCE_NAMES = {'DESCRIPTION', 'NAMESPACE'}
ASCII_SOURCE_IGNORED_PARTS = {
    '.git', '.pytest_cache', '.venv', '__pycache__', 'venv',
}


def is_unicode_fixture_csv(relative: Path):
    parts = relative.parts
    return (
        relative.suffix == '.csv'
        and len(parts) >= 5
        and parts[0] == 'yaml'
        and parts[1] == 'examples'
        and parts[3] in {'input', 'expected'}
    )


def validate_ascii_sources(root: Path):
    errors = []
    for path in sorted(root.rglob('*')):
        if not path.is_file():
            continue
        relative = path.relative_to(root)
        if any(part in ASCII_SOURCE_IGNORED_PARTS for part in relative.parts):
            continue
        if is_unicode_fixture_csv(relative):
            continue
        if (
            path.suffix.lower() not in ASCII_SOURCE_SUFFIXES
            and path.name not in ASCII_SOURCE_NAMES
        ):
            continue

        try:
            content = path.read_bytes()
        except OSError as exc:
            errors.append(f"ERROR: {relative}: cannot read source: {exc}")
            continue

        for offset, value in enumerate(content):
            if value <= 0x7F:
                continue
            line = content.count(b'\n', 0, offset) + 1
            previous_newline = content.rfind(b'\n', 0, offset)
            column = offset - previous_newline
            errors.append(
                f"ERROR: {relative}:{line}:{column}: non_ascii_source "
                f"byte 0x{value:02X}"
            )
            break
    return errors


def diagnostic_path_key(key):
    return str(key).encode('unicode_escape').decode('ascii')


def validate_unicode_scalars(value, path):
    errors = []
    if isinstance(value, str):
        for index, character in enumerate(value):
            code_point = ord(character)
            if 0xD800 <= code_point <= 0xDFFF:
                errors.append(
                    f"ERROR: {path}: invalid_text surrogate U+{code_point:04X} "
                    f"at string offset {index}"
                )
                break
    elif isinstance(value, list):
        for index, item in enumerate(value):
            errors.extend(validate_unicode_scalars(item, f'{path}[{index}]'))
    elif isinstance(value, dict):
        for key, item in value.items():
            errors.extend(validate_unicode_scalars(key, f'{path}.<key>'))
            errors.extend(
                validate_unicode_scalars(
                    item,
                    f'{path}.{diagnostic_path_key(key)}',
                )
            )
    return errors


def check_yaml_files(root: Path):
    errors = []
    warnings = []
    errors.extend(validate_ascii_sources(root))
    for yaml_file in sorted(root.rglob('*.yaml')):
        if '.github' in yaml_file.parts:
            continue
        try:
            with open(yaml_file, 'r', encoding='utf-8') as f:
                document = yaml.load(f, Loader=UniqueKeyLoader)
            errors.extend(
                validate_unicode_scalars(
                    document,
                    str(yaml_file.relative_to(root)),
                )
            )
        except Exception as e:
            msg = str(e)
            if hasattr(e, 'problem_mark') and e.problem_mark:
                msg += f" at line {e.problem_mark.line + 1}"
            errors.append(f"ERROR: {yaml_file.relative_to(root)}: {msg}")

    # Also validate schemas
    env, schema_errors = build_schema_env(root)
    errors.extend(schema_errors)
    environment_schema = None
    if (root / 'yaml' / 'schema_environment.yaml').exists():
        environment_schema, environment_schema_errors = build_schema_env(
            root, 'schema_environment.yaml'
        )
        errors.extend(environment_schema_errors)
        if environment_schema_errors:
            environment_schema = None
    validation_manifest, manifest_load_errors = load_validation_manifest(root)
    errors.extend(manifest_load_errors)
    if validation_manifest is not None:
        errors.extend(
            validate_validation_manifest(root, validation_manifest)
        )
    errors.extend(
        validate_examples_structure(
            root, env, warnings, validation_manifest
        )
    )
    errors.extend(
        validate_repository_function_fingerprints(
            root, environment_schema
        )
    )
    errors.extend(validate_examples_layout(root))
    errors.extend(validate_examples_index(root))
    errors.extend(validate_expected_error_contracts(root))
    errors.extend(validate_csv_shapes(root))
    errors.extend(validate_example_readmes(root))
    errors.extend(validate_rule_metadata(root))

    csv_errors, csv_warnings = validate_examples_csv(root, env)
    errors.extend(csv_errors)
    warnings.extend(csv_warnings)
    return errors, warnings


def validate_examples_csv(root: Path, env=None):
    errors = []
    warnings = []
    examples_dir = root / 'yaml' / 'examples'
    if not examples_dir.exists():
        return errors, warnings
    if env is None:
        candidate_env, _ = build_schema_env(root)
        env = candidate_env

    profile_checked = set()
    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith('.'):
            continue

        for spec_path in example_spec_paths(ex_dir):
            try:
                with open(spec_path, 'r', encoding='utf-8') as f:
                    spec = yaml.load(f, Loader=UniqueKeyLoader)
            except Exception:
                continue

            if env is not None:
                spec, resolution_errors, _ = prepare_spec_document(
                    spec, f"{ex_dir.name}/{spec_path.name}", spec_path, env
                )
                if resolution_errors:
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
                # The golden file is the artifact the specification says it
                # produces, so its name comes from output.path rather than a
                # convention over the domain.
                declared_path = output.get('path')
                if isinstance(declared_path, str):
                    declared_name = PurePosixPath(declared_path).name
                    if csv_file.name != declared_name:
                        errors.append(
                            f"ERROR: {ex_dir.name}/{csv_file.name}: expected "
                            f"artifact name {declared_name}, which "
                            f"output.path declares"
                        )
                try:
                    with open(csv_file, 'r', encoding='utf-8') as f:
                        reader = csv.reader(f)
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
                except (OSError, UnicodeError, csv.Error):
                    continue

                profile = artifact_profile(output)
                if profile == 'csv' and csv_file not in profile_checked:
                    profile_checked.add(csv_file)
                    errors.extend(
                        validate_csv_artifact(
                            csv_file,
                            f"{ex_dir.name}/{csv_file.name}",
                            spec,
                        )
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
