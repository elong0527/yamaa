#!/usr/bin/env python3
import argparse
import copy
import csv
import datetime as dt
import decimal
import importlib.util
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

# Editorial validators live in editorial.py; re-exported here so existing
# `from validate_repository import ...` call sites keep working.
sys.path.insert(0, str(Path(__file__).resolve().parent))  # noqa: E402
from editorial import (  # noqa: E402
    ASCII_SOURCE_IGNORED_PARTS,
    ASCII_SOURCE_NAMES,
    ASCII_SOURCE_SUFFIXES,
    LIFECYCLE_BADGE_PATTERN,
    README_FOOTER_PATTERN,
    README_FORBIDDEN_PATTERN,
    README_KEY_COLUMNS,
    SPEC_FILE_PATTERN,
    UniqueKeyLoader,
    diagnostic_path_key,
    example_spec_paths,
    is_readme_badge_line,
    is_unicode_fixture_csv,
    validate_ascii_sources,
    validate_example_readmes,
    validate_examples_badges,
    validate_examples_index,
    validate_examples_readme_presence,
    validate_literal_canonical_form,
    validate_rule_metadata,
    validate_source_canonical_form,
    validate_unicode_scalars,
)

__all__ = [
    'ASCII_SOURCE_IGNORED_PARTS',
    'ASCII_SOURCE_NAMES',
    'ASCII_SOURCE_SUFFIXES',
    'LIFECYCLE_BADGE_PATTERN',
    'README_FOOTER_PATTERN',
    'README_FORBIDDEN_PATTERN',
    'README_KEY_COLUMNS',
    'SPEC_FILE_PATTERN',
    'UniqueKeyLoader',
    'diagnostic_path_key',
    'example_spec_paths',
    'is_readme_badge_line',
    'is_unicode_fixture_csv',
    'validate_ascii_sources',
    'validate_example_readmes',
    'validate_examples_badges',
    'validate_examples_index',
    'validate_examples_readme_presence',
    'validate_literal_canonical_form',
    'validate_rule_metadata',
    'validate_source_canonical_form',
    'validate_unicode_scalars',
]


# R022 regular expressions. Every pattern the language admits -- a schema
# `pattern` descriptor, `str_extract.pattern`, and a `matches` verification --
# is read through the portable R022 contract: the rule text plus the shared
# conformance fixtures. This validator replays them through the same Python
# binding the package ships (`yamaa.regex` over the standard library), so
# repository validation is not a second dialect. No other regular-expression
# library reads a pattern of the language.
REGEX_CONTRACT = 'regex'
REGEX_CONTRACT_VERSION = '2.0.0'

# The portable regex binding is imported lazily on first use. Importing the
# yamaa package eagerly pulls in its runtime dependencies (polars), which
# repository validation only needs when it actually compiles a pattern;
# keeping the import lazy drops this module's import cost from ~90MB to
# ~20MB for consumers that never touch regular expressions.
# _portable_binding is None until the first require_regex_binding() call
# attempts the import, then True (loaded) or False (unavailable).
_portable_binding = None
_PortableRegexError = None
_portable_group_count = None
_portable_compile = None
_portable_full_match = None


class RegexBindingUnavailable(Exception):
    """R022 unsupported_regex_engine: the portable binding is not installed."""


class InvalidRegex(Exception):
    """R022 invalid_regex: the portable contract rejected a pattern."""

    def __init__(self, reason):
        super().__init__(reason)
        self.reason = reason


class RegexGroupOutOfRange(Exception):
    """R022 regex_group_out_of_range: a group the pattern does not declare."""


def regex_contract_requirement():
    """Name the contract and binding R022 pins, for a failure to report."""
    return (
        f"portable contract {REGEX_CONTRACT_VERSION} "
        "(the yamaa.regex binding over the standard library)"
    )


def require_regex_binding():
    """Import the portable binding on first use, or fail rather than substitute another."""
    global _portable_binding, _PortableRegexError
    global _portable_group_count, _portable_compile, _portable_full_match
    if _portable_binding is None:
        try:
            from yamaa.regex import (
                RegexError as binding_error,
                capture_group_count as binding_group_count,
                compile_pattern as binding_compile,
                full_match as binding_full_match,
            )
        except ImportError:
            _portable_binding = False
        else:
            _PortableRegexError = binding_error
            _portable_group_count = binding_group_count
            _portable_compile = binding_compile
            _portable_full_match = binding_full_match
            _portable_binding = True
    if not _portable_binding:
        raise RegexBindingUnavailable(
            "unsupported_regex_engine under R022: this validator requires "
            f"{regex_contract_requirement()} and does not fall back to "
            "another regular-expression library"
        )


def compile_regex(pattern):
    """Compile one R022 pattern source through the portable contract."""
    require_regex_binding()
    if not isinstance(pattern, str):
        raise InvalidRegex(
            f"pattern must be a string, got {type(pattern).__name__}"
        )
    try:
        return _portable_compile(pattern)
    except _PortableRegexError as exc:
        raise InvalidRegex(exc.reason) from exc


def anchored_regex_source(pattern):
    """Return the full-match form R022 gives the `pattern` keyword."""
    return '^(?:' + pattern + ')$'


def regex_search(pattern, subject):
    """Return the leftmost match of an R022 pattern, or None."""
    return compile_regex(pattern).search(subject)


def regex_full_match(pattern, subject):
    """Return whether an R022 pattern matches the whole subject."""
    if not isinstance(subject, str):
        return False
    require_regex_binding()
    try:
        return _portable_full_match(pattern, subject)
    except _PortableRegexError as exc:
        raise InvalidRegex(exc.reason) from exc


def regex_capture_group_count(pattern):
    """Count the capturing groups an accepted R022 pattern declares.

    Groups are numbered by the order of their opening parenthesis, counting
    only capturing groups: `(?:`, lookaround, and a parenthesis inside a
    character class contribute no number, while `(?<name>` does. The binding
    answers an out-of-range group index exactly as it answers a group the
    match did not enter, so the count is read from the pattern source the
    contract has already accepted.
    """
    require_regex_binding()
    try:
        return _portable_group_count(pattern)
    except _PortableRegexError as exc:
        raise InvalidRegex(exc.reason) from exc


REGEX_NO_MATCH = object()


def regex_extract(pattern, subject, group=0):
    """Return the R022 `str_extract` result for one pattern and subject.

    REGEX_NO_MATCH when the pattern matched nowhere, None when the match did
    not enter the requested group, and the matched text otherwise.
    """
    declared = regex_capture_group_count(pattern)
    if not isinstance(group, int) or group < 0 or group > declared:
        raise RegexGroupOutOfRange(
            f"group {group!r} is outside the {declared} capturing "
            f"{'group' if declared == 1 else 'groups'} the pattern declares"
        )
    match = regex_search(pattern, subject)
    if match is None:
        return REGEX_NO_MATCH
    return match.group(group)


def regex_pattern_errors(pattern, path, full_match=False):
    """Return the R022 invalid_regex error a pattern produces, if any."""
    try:
        compile_regex(pattern)
        if full_match:
            compile_regex(anchored_regex_source(pattern))
    except InvalidRegex as exc:
        return [validation_diagnostic(
            path,
            'invalid_regex',
            f"the R022 contract rejects {pattern!r}: {exc.reason}",
            context={'pattern': pattern, 'reason': exc.reason},
        )]
    return []


def regex_group_errors(keyword, payload, path):
    """Return the R022 regex_group_out_of_range error for a str_extract."""
    if keyword != 'str_extract' or not isinstance(payload, dict):
        return []
    pattern = payload.get('pattern')
    group = payload.get('group', 0)
    if not isinstance(pattern, str) or type(group) is not int:
        return []
    try:
        declared = regex_capture_group_count(pattern)
    except InvalidRegex:
        # The pattern's own rejection is reported where it is typed `regex`.
        return []
    if group < 0 or group > declared:
        return [validation_diagnostic(
            f"{path}.group",
            'regex_group_out_of_range',
            f"group {group} is outside the {declared} capturing "
            f"{'group' if declared == 1 else 'groups'} {pattern!r} declares",
            context={
                'group': group,
                'group_count': declared,
                'pattern': pattern,
            },
        )]
    return []


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
    ('R001', 'forward_reference'): {'column', 'dependency'},
    ('R001', 'key_dependency'): {'column', 'dependency'},
    ('R002', 'duplicate_identifier'): {'identifier'},
    ('R002', 'unknown_field'): {'identifier'},
    ('R003', 'duplicate_identifier'): {'identifier'},
    ('R003', 'prohibited_construct'): {'identifier'},
    ('R003', 'unknown_field'): {'identifier'},
    ('R004', 'invalid_predicate'): {'predicate'},
    ('R004', 'incompatible_input_type'): {'left_type', 'right_type'},
    ('R004', 'unknown_field'): {'identifier'},
    ('R005', 'duplicate_order_term'): {'column'},
    ('R005', 'internal_column_in_keys'): {'column'},
    ('R005', 'undeclared_column'): {'column'},
    ('R006', 'invalid_field_type'): {'actual', 'expected'},
    ('R007', 'ambiguous_dictionary'): {'entries', 'folded_key'},
    ('R007', 'incompatible_input_type'): {
        'actual', 'expected', 'source',
    },
    ('R007', 'invalid_cut'): {'reason'},
    ('R007', 'incomparable_sources'): {'sources', 'types'},
    ('R007', 'source_key_length_mismatch'): {
        'key', 'key_count', 'key_base', 'key_base_count',
    },
    ('R007', 'zero_offset'): {'offset'},
    ('R007', 'window_on_window_result'): {'column', 'depends_on'},
    ('R007', 'unknown_window'): {'window'},
    ('R007', 'window_order_by_required'): {'operation'},
    ('R007', 'window_order_by_forbidden'): {'operation'},
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
    ('R013', 'invalid_aggregate_context'): {'reason'},
    ('R013', 'invalid_aggregate_expression'): {'expr'},
    ('R013', 'invalid_derive_step'): {'reason'},
    ('R013', 'mixed_relations'): {'relations'},
    ('R013', 'nested_reduction'): {'expr', 'inner', 'outer'},
    ('R013', 'prohibited_construct'): {'construct', 'expr'},
    ('R013', 'prohibited_function'): {'expr', 'function'},
    ('R013', 'unknown_derive_variable'): {'identifier'},
    ('R013', 'unknown_field'): {'identifier'},
    ('R013', 'incompatible_input_type'): {
        'actual', 'expected', 'source',
    },
    ('R014', 'unknown_field'): {'dataset', 'field'},
    ('R003', 'duplicate_identifier'): {'identifier'},
    ('R003', 'incomparable_range_types'): {
        'lower_type', 'intermediate', 'upper_type', 'value_type',
    },
    ('R003', 'no_applicable_keys'): {'dataset', 'hint', 'keys'},
    ('R003', 'redundant_key_base'): {
        'key', 'key_base',
    },
    ('R003', 'source_key_length_mismatch'): {
        'key', 'key_count', 'key_base', 'key_base_count',
    },
    ('R003', 'unpaired_fields'): {
        'declared', 'intermediate', 'missing',
    },
    ('R003', 'rename_only_intermediate'): {
        'intermediate', 'dataset',
    },
    ('R006', 'missing_required_field'): {'class', 'field'},
    ('R016', 'month_out_of_range'): {'month'},
    ('R016', 'month_not_permitted'): {'month'},
    ('R016', 'month_required'): {'minimum_source_precision'},
    ('R016', 'day_out_of_range'): {'day'},
    ('R016', 'incompatible_input_type'): {'actual', 'expected', 'source'},
    ('R016', 'value_not_permitted'): {'permitted', 'value'},
    ('R017', 'inheritance_cycle'): {'reason'},
    ('R017', 'invalid_clear'): {'field'},
    ('R017', 'invalid_parent_path'): {'reason'},
    ('R017', 'redundant_field_type'): {'dataset', 'field', 'type'},
    ('R017', 'schema_version_mismatch'): {
        'entry_version', 'parent_version',
    },
    ('R018', 'function_contract_mismatch'): {
        'available', 'function', 'requested',
    },
    ('R020', 'unknown_artifact_profile'): {'path', 'permitted'},
    ('R021', 'resource_path_missing'): {'path'},
    ('R021', 'resource_path_not_regular_file'): {'path'},
    ('R021', 'resource_path_not_relative'): {'path'},
    ('R021', 'resource_path_outside_project'): {'path'},
    ('R021', 'resource_path_symlink'): {'path'},
    ('R021', 'resource_path_uri_scheme'): {'path'},
    # The binding's wording is additional context an implementation may
    # report; the pattern is the portable fact a fixture must state.
    ('R022', 'invalid_regex'): {'pattern'},
    ('R022', 'regex_group_out_of_range'): {
        'group', 'group_count', 'pattern',
    },
    ('R023', 'source_profile_unknown'): {'path'},
}
VALIDATION_CONDITION_REGISTRY = {
    key: {
        'allowed_phases': {'validation'},
        'required_context': required_context,
    }
    for key, required_context in VALIDATION_CONTEXT_FIELDS.items()
}


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


class PredicateSemanticIssue(str):
    def __new__(cls, message, condition, context, span):
        value = super().__new__(cls, message)
        value.condition = condition
        value.context = dict(context)
        value.span = tuple(span)
        return value


# R004's closed vocabulary. `yaml/grammar/predicate.yaml` is its single
# source, and validate_grammar_contracts fails when the two drift apart.
# The parser itself is the runtime's: `yamaa.expressions.predicates` already
# returns this validator's AST shape, so this file owns no second grammar.
# The import is lazy (see the regex binding note above): importing the yamaa
# package pulls in polars, which this module only needs when it actually
# parses a predicate.
PREDICATE_COMPARISON_OPERATORS = None
PREDICATE_RESERVED_NAMES = None
PredicateError = None
parse_predicate = None


def _ensure_predicate_binding():
    """Import yamaa.expressions.predicates on first use, or exit when unavailable."""
    global PREDICATE_COMPARISON_OPERATORS, PREDICATE_RESERVED_NAMES
    global PredicateError, parse_predicate
    if parse_predicate is None:
        try:
            from yamaa.expressions.predicates import (
                COMPARISON_OPERATORS as comparison_operators,
                RESERVED_NAMES as reserved_names,
                PredicateError as predicate_error,
                parse_predicate as parse,
            )
        except ImportError as error:
            raise SystemExit(
                "validate_repository.py requires the yamaa package "
                "(run: uv sync --project python --locked)"
            ) from error
        PREDICATE_COMPARISON_OPERATORS = comparison_operators
        PREDICATE_RESERVED_NAMES = reserved_names
        PredicateError = predicate_error
        parse_predicate = parse


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
    if left is DERIVED_TYPE_UNKNOWN or right is DERIVED_TYPE_UNKNOWN:
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
                if (
                    actual is not None
                    and actual is not DERIVED_TYPE_UNKNOWN
                    and actual != 'str'
                ):
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


class AggregateExpressionError(NumericExpressionError):
    """An R013 expression cannot be tokenized or parsed."""

    def __init__(
        self,
        message,
        start,
        end=None,
        *,
        condition='invalid_aggregate_expression',
        context=None,
    ):
        super().__init__(
            message,
            start,
            end,
            condition=condition,
            context=context,
        )


AGGREGATE_REDUCERS = {'SUM', 'COUNT', 'MIN', 'MAX', 'MEAN', 'ONLY'}


def tokenize_aggregate_expression(text):
    """Tokenize the closed R013 aggregate language."""
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
            kind = 'NAME'
            if end < length and text[end] == '.':
                if end + 1 < length and text[end + 1] == '*':
                    value += '.*'
                    end += 2
                    kind = 'QUALIFIED_STAR'
                else:
                    suffix = re.match(
                        r'[A-Za-z_][A-Za-z0-9_]*', text[end + 1:]
                    )
                    if suffix is None:
                        raise AggregateExpressionError(
                            'invalid qualified identifier', index, end + 1
                        )
                    value += '.' + suffix.group(0)
                    end += 1 + len(suffix.group(0))
            construct = PROHIBITED_NUMERIC_KEYWORDS.get(value.upper())
            if '.' not in value and construct is not None:
                raise AggregateExpressionError(
                    f'{construct} construct is not permitted in an '
                    'aggregate expression',
                    index,
                    end,
                    condition='prohibited_construct',
                    context={'construct': construct},
                )
            tokens.append((kind, value, index, end))
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
        if char in '<>=!':
            end = index + 1
            if end < length and text[end] == '=':
                end += 1
            raise AggregateExpressionError(
                'comparison is not permitted in an aggregate expression',
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
            raise AggregateExpressionError(
                'string literals are not permitted in an aggregate expression',
                index,
                end,
                condition='prohibited_construct',
                context={'construct': 'string'},
            )
        raise AggregateExpressionError(
            f"unexpected character {char!r}", index, index + 1
        )
    tokens.append(('EOF', '', length, length))
    return tokens


class AggregateExpressionParser(NumericExpressionParser):
    def __init__(self, text):
        self.text = text
        self.tokens = tokenize_aggregate_expression(text)
        self.index = 0

    def require(self, kind, message):
        if self.token[0] != kind:
            raise AggregateExpressionError(
                message, self.token[2], self.token[3]
            )
        return self.advance()

    def parse(self):
        node = self.parse_expression()
        if self.token[0] != 'EOF':
            raise AggregateExpressionError(
                'unexpected trailing token', self.token[2], self.token[3]
            )
        return node

    def parse_primary(self):
        token = self.token
        if token[0] == 'NUMBER':
            self.advance()
            return {
                'kind': 'number',
                'type': (
                    'float'
                    if '.' in token[1] or 'e' in token[1].lower()
                    else 'int'
                ),
                'value': token[1],
                'span': (token[2], token[3]),
            }
        if token[0] == 'NAME':
            self.advance()
            keyword = token[1].upper()
            if self.token[0] != 'LPAREN':
                if keyword == 'NULL':
                    return {'kind': 'null', 'span': (token[2], token[3])}
                return {
                    'kind': 'identifier',
                    'name': token[1],
                    'span': (token[2], token[3]),
                }

            self.advance()
            if keyword in AGGREGATE_REDUCERS:
                if self.token[0] == 'QUALIFIED_STAR':
                    star = self.advance()
                    argument = {
                        'kind': 'qualified_star',
                        'dataset': star[1][:-2],
                        'span': (star[2], star[3]),
                    }
                else:
                    argument = self.parse_expression()
                close = self.require(
                    'RPAREN', "expected ')' to close reducer"
                )
                return {
                    'kind': 'reduction',
                    'name': token[1],
                    'argument': argument,
                    'name_span': (token[2], token[3]),
                    'span': (token[2], close[3]),
                }

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
            node = dict(self.parse_expression())
            close = self.require('RPAREN', "expected ')' to close expression")
            node['span'] = (open_token[2], close[3])
            return node
        if token[0] == 'QUALIFIED_STAR':
            raise AggregateExpressionError(
                'qualified star is valid only as COUNT argument',
                token[2],
                token[3],
            )
        raise AggregateExpressionError(
            'expected a number, identifier, reducer, function, NULL, or '
            'parenthesis',
            token[2],
            token[3],
        )


def parse_aggregate_expression(text):
    if not isinstance(text, str) or not text:
        raise AggregateExpressionError(
            'aggregate expression must be a non-empty string', 0, 0
        )
    return AggregateExpressionParser(text).parse()


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
            if value_type is DERIVED_TYPE_UNKNOWN:
                return None
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
            errors.extend(
                regex_pattern_errors(desc['pattern'], path, full_match=True)
            )

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

def validate_type(data, t_refs, env, path, fragment=False):
    if not isinstance(t_refs, list):
        t_refs = [t_refs]

    # If the union allows null and data is None
    if 'null' in t_refs and data is None:
        return []

    attempted = []
    for t in t_refs:
        errors = _check_single_type(data, t, env, path, fragment)
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
        try:
            satisfied = regex_full_match(pattern, data)
        except InvalidRegex as exc:
            # The pattern is the defect; do not also blame the value.
            errors.append(validation_diagnostic(
                path,
                'invalid_regex',
                f"the R022 contract rejects {pattern!r}: {exc.reason}",
                context={'pattern': pattern, 'reason': exc.reason},
            ))
        else:
            if not satisfied:
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


def validate_descriptor(data, descriptor, env, path, fragment=False):
    errors = validate_type(data, descriptor['type'], env, path, fragment)
    if errors:
        return errors
    return validate_constraints(data, descriptor, path)


def _check_single_type(data, t, env, path, fragment=False):
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
            errors.extend(
                validate_type(item, [inner], env, f"{path}{suffix}", fragment)
            )
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
            errors.extend(
                validate_type(k, [k_type], env, f"{path}.key({k})", fragment)
            )
            errors.extend(
                validate_type(v, [v_type], env, f"{path}.{k}", fragment)
            )
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
                if (
                    fdesc.get('required')
                    and not fragment
                    and fname not in data
                ):
                    errors.append(
                        validation_diagnostic(
                            f"{path}.{fname}",
                            'missing_required_field',
                            f"missing required field '{fname}' for class {t}",
                            context={'field': fname, 'class': t},
                        )
                    )
                if fname in data:
                    errors.extend(
                        validate_descriptor(
                            data[fname], fdesc, env, f"{path}.{fname}",
                            fragment,
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
                        validate_descriptor(
                            val, reg_def, env, f"{path}.{key}", fragment
                        )
                    )
                elif isinstance(reg_def, list): # it's a class inline
                    # Validate against an anonymous class
                    if not isinstance(val, dict):
                        actual = (
                            'mapping' if isinstance(val, dict)
                            else 'sequence' if isinstance(val, list)
                            else 'null' if val is None
                            else type(val).__name__
                        )
                        errors.append(
                            validation_diagnostic(
                                f"{path}.{key}",
                                'invalid_field_type',
                                f"expected {key}, got {actual}",
                                context={
                                    'expected': key,
                                    'actual': actual,
                                },
                            )
                        )
                        continue
                    allowed_keys = set()
                    for field in reg_def:
                        for fname, fdesc in field.items():
                            allowed_keys.add(fname)
                            if (
                                fdesc.get('required')
                                and not fragment
                                and fname not in val
                            ):
                                errors.append(
                                    validation_diagnostic(
                                        f"{path}.{key}.{fname}",
                                        'missing_required_field',
                                        f"missing required field '{fname}'",
                                        context={'field': fname, 'class': key},
                                    )
                                )
                            if fname in val:
                                errors.extend(
                                    validate_descriptor(
                                        val[fname], fdesc, env,
                                        f"{path}.{key}.{fname}",
                                        fragment,
                                    )
                                )
                    for k in val:
                        if k not in allowed_keys:
                            errors.append(f"ERROR: {path}.{key}.{k}: unknown field '{k}'")
                    errors.extend(
                        regex_group_errors(key, val, f"{path}.{key}")
                    )
            return errors

        else:
            # Normal alias
            errors = validate_type(data, alias['type'], env, path, fragment)
            if errors:
                return errors
            errors = validate_constraints(data, alias, path)
            if t == 'regex' and isinstance(data, str):
                errors = errors + regex_pattern_errors(data, path)
            return errors

    return [f"ERROR: {path}: unknown type '{t}'"]


INHERITANCE_KEYED_COLLECTIONS = {
    'input': ('mapping', None, 'dataset_class'),
    'intermediates': ('list', 'id', 'intermediate_class'),
    'columns': ('list', 'name', 'column_class'),
    'rows': ('list', 'id', 'row_class'),
}

# REQ-0630 composes a matching column member by each field's declared kind.
# Every other keyed collection still replaces a present member field whole.
INHERITANCE_COMPOSING_COLLECTIONS = frozenset({'columns'})


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


def _type_matches(data, type_ref, env, fragment=False):
    return not _check_single_type(
        data, type_ref, env, '<normalization>', fragment
    )


def normalize_descriptor_value(data, descriptor, env, fragment=False):
    """Materialize the canonical R006 form of a descriptor value."""
    return normalize_type_value(data, descriptor['type'], env, fragment)


def normalize_type_value(data, type_value, env, fragment=False):
    """Normalize R006 list and single-required-field class shorthands.

    ``fragment`` withholds a schema default, which is what R017 needs while a
    layer's ``columns`` member composes: a default materialized per layer
    would replace what a parent wrote.
    """
    members = _type_members(type_value)

    for member in members:
        if not (member.startswith('list[') and member.endswith(']')):
            continue
        inner = member[5:-1].strip()
        if inner in members and not isinstance(data, list):
            if _type_matches(data, inner, env, fragment):
                return [normalize_type_value(data, inner, env, fragment)]

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
            if not _type_matches(data, member, env, fragment):
                continue
            expanded = {
                field_name: normalize_descriptor_value(
                    data, field_descriptor, env, fragment
                )
            }
            for name, descriptor in fields.items():
                if name in expanded or 'default' not in descriptor or fragment:
                    continue
                expanded[name] = copy.deepcopy(descriptor['default'])
            return expanded

    for member in members:
        if _type_matches(data, member, env, fragment):
            return normalize_single_type_value(data, member, env, fragment)
    return copy.deepcopy(data)


def normalize_inline_class(data, fields, env, fragment=False):
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
                data[name], descriptor, env, fragment
            )
        elif 'default' in descriptor and not fragment:
            normalized[name] = copy.deepcopy(descriptor['default'])
    for name, value in data.items():
        if name not in normalized and name not in descriptors:
            normalized[name] = copy.deepcopy(value)
    return normalized


def normalize_single_type_value(data, type_ref, env, fragment=False):
    if type_ref.startswith('list[') and type_ref.endswith(']'):
        inner = type_ref[5:-1].strip()
        return [
            normalize_type_value(item, inner, env, fragment) for item in data
        ]
    if type_ref.startswith('dict[') and type_ref.endswith(']'):
        key_type, value_type = split_type_arguments(type_ref[5:-1])
        return {
            normalize_type_value(key, key_type, env): normalize_type_value(
                value, value_type, env, fragment
            )
            for key, value in data.items()
        }
    if type_ref in env.get('classes', {}):
        return normalize_inline_class(
            data, env['classes'][type_ref], env, fragment
        )
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
                payload = normalize_inline_class(
                    payload, definition, env, fragment
                )
            elif isinstance(definition, dict) and 'type' in definition:
                payload = normalize_descriptor_value(
                    payload, definition, env, fragment
                )
            return {keyword: payload}
        if type_ref == 'derivation' and isinstance(data, str):
            # REQ-0319: a bare derivation string is the source shorthand.
            # Expand it before the union dispatch so the registry and the
            # handled-expression expansion apply unchanged, mirroring the
            # engine's parse-time normalization.
            data = {'source': data}
        if type_ref == 'case_result' and isinstance(data, str):
            # REQ-0319: a bare then/otherwise variable is a source read.
            data = {'source': data}
        return normalize_type_value(data, alias['type'], env, fragment)
    return copy.deepcopy(data)


def validate_partial_inheritance_member(
    value, class_name, identity, label, env, fragment=False
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
        errors.extend(
            validate_descriptor(field_value, descriptor, env, path, fragment)
        )
        normalized[name] = normalize_descriptor_value(
            field_value, descriptor, env, fragment
        )

    return normalized, errors


def validate_inheritance_layer(layer, label, env):
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
        fragment = name in INHERITANCE_COMPOSING_COLLECTIONS
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
                        member_id, ['identifier'], env,
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
                            member, class_name, identity, member_path, env,
                            fragment,
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
                    member, class_name, identity, member_path, env, fragment
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
    if rooted_project_segments(value) is not None or written.is_absolute():
        # REQ-0780: a rooted path resolves against the approved root it names,
        # and REQ-0781 reads that written form, so rebasing leaves it alone.
        return value
    target = (layer_path.parent / written).resolve()
    try:
        relative = os.path.relpath(target, entry_path.parent.resolve())
    except ValueError:
        return str(target)
    return Path(relative).as_posix()


def rebase_layer_paths(layer, layer_path, entry_path):
    """Rebase layer-owned resource paths to the entry file."""
    rebased = copy.deepcopy(layer)
    datasets = rebased.get('input')
    if isinstance(datasets, dict):
        for source in datasets.values():
            if not isinstance(source, dict):
                continue
            for field in ('path', 'schema'):
                if isinstance(source.get(field), str):
                    source[field] = _rebase_local_path(
                        source[field], layer_path, entry_path
                    )
    output = rebased.get('output')
    if isinstance(output, dict):
        # REQ-0629: an inherited output publishes where its layer names.
        for field in ('path', 'warning_log', 'verification_log'):
            if isinstance(output.get(field), str):
                output[field] = _rebase_local_path(
                    output[field], layer_path, entry_path
                )
    rows = rebased.get('rows')
    if isinstance(rows, list):
        for row in rows:
            if not isinstance(row, dict):
                continue
            catalog = row.get('catalog')
            if isinstance(catalog, dict) and isinstance(catalog.get('path'), str):
                catalog['path'] = _rebase_local_path(
                    catalog['path'], layer_path, entry_path
                )
    return rebased


def _clear_provenance(provenance, prefix):
    for key in list(provenance):
        if key == prefix or key.startswith(prefix + '.'):
            del provenance[key]


def _record_provenance(value, path, provenance_source, provenance):
    """Attribute one written value, and every leaf below it, to its layer."""
    provenance[path] = provenance_source
    if isinstance(value, dict):
        for name, nested in value.items():
            _record_provenance(
                nested, f"{path}.{name}", provenance_source, provenance
            )


def _replace_value(value, path, provenance_source, provenance):
    copied = copy.deepcopy(value)
    _clear_provenance(provenance, path)
    _record_provenance(copied, path, provenance_source, provenance)
    return copied


def _composing_member(accumulated, incoming, type_value, env):
    """Return the union member both values compose under, else None."""
    if not isinstance(accumulated, dict) or not isinstance(incoming, dict):
        return None
    member = _matching_member(incoming, type_value, env)
    if member is None or member != _matching_member(accumulated, type_value, env):
        return None
    return member


def _matching_member(value, type_value, env):
    for member in _type_members(type_value):
        if _type_matches(value, member, env, True):
            return member
    return None


def _compose_class_value(
    accumulated, incoming, fields, env, path, provenance_source, provenance,
):
    descriptors = {
        name: descriptor
        for entry in fields
        for name, descriptor in entry.items()
    }
    composed = dict(accumulated)
    for name, value in incoming.items():
        field_path = f"{path}.{name}"
        descriptor = descriptors.get(name)
        if descriptor is None or name not in composed:
            composed[name] = _replace_value(
                value, field_path, provenance_source, provenance
            )
            continue
        composed[name] = _compose_value(
            composed[name], value, descriptor['type'], env, field_path,
            provenance_source, provenance,
        )
    return composed


def _compose_operation_value(
    accumulated, incoming, registry_name, env, path, provenance_source,
    provenance,
):
    """Compose two registry values, which R007 admits one keyword each."""
    if len(accumulated) != 1 or len(incoming) != 1:
        return _replace_value(incoming, path, provenance_source, provenance)
    keyword, payload = next(iter(incoming.items()))
    inherited_keyword, inherited_payload = next(iter(accumulated.items()))
    definition = env.get('registries', {}).get(registry_name, {}).get(keyword)
    if keyword != inherited_keyword or definition is None:
        return _replace_value(incoming, path, provenance_source, provenance)
    keyword_path = f"{path}.{keyword}"
    if isinstance(definition, list):
        if not isinstance(inherited_payload, dict) or not isinstance(
            payload, dict
        ):
            return _replace_value(
                incoming, path, provenance_source, provenance
            )
        composed = _compose_class_value(
            inherited_payload, payload, definition, env, keyword_path,
            provenance_source, provenance,
        )
    else:
        composed = _compose_value(
            inherited_payload, payload, definition['type'], env, keyword_path,
            provenance_source, provenance,
        )
    return {keyword: composed}


def _compose_value(
    accumulated, incoming, type_value, env, path, provenance_source,
    provenance,
):
    """Compose one written value onto the value it inherits, by kind.

    A class composes field by field, a mapping key by key, and a registry
    value only when both name one keyword.  Every other kind, including every
    list, replaces.  A null here is an R006 value, never a clearing marker:
    REQ-0633 keeps the marker at the two composition boundaries above.
    """
    member = _composing_member(accumulated, incoming, type_value, env)
    if member is None:
        return _replace_value(incoming, path, provenance_source, provenance)
    if member in env.get('classes', {}):
        return _compose_class_value(
            accumulated, incoming, env['classes'][member], env, path,
            provenance_source, provenance,
        )
    if member.startswith('dict[') and member.endswith(']'):
        _, inner = split_type_arguments(member[5:-1])
        composed = dict(accumulated)
        for key, value in incoming.items():
            key_path = f"{path}.{key}"
            if key not in composed:
                composed[key] = _replace_value(
                    value, key_path, provenance_source, provenance
                )
                continue
            composed[key] = _compose_value(
                composed[key], value, inner, env, key_path, provenance_source,
                provenance,
            )
        return composed
    alias = env.get('aliases', {}).get(member)
    if alias is None:
        return _replace_value(incoming, path, provenance_source, provenance)
    registry_name = alias.get('registry')
    if registry_name is not None:
        return _compose_operation_value(
            accumulated, incoming, registry_name, env, path,
            provenance_source, provenance,
        )
    return _compose_value(
        accumulated, incoming, alias['type'], env, path, provenance_source,
        provenance,
    )


def _materialize_inheritance_fragments(resolved, env):
    """Complete every composed member once composition is finished."""
    for name in INHERITANCE_COMPOSING_COLLECTIONS:
        _, _, class_name = INHERITANCE_KEYED_COLLECTIONS[name]
        members = resolved.get(name)
        if not isinstance(members, list):
            continue
        fields = schema_class_fields(env, class_name)
        for member in members:
            if not isinstance(member, dict):
                continue
            for field, descriptor in fields.items():
                if field in member and member[field] is not None:
                    member[field] = normalize_descriptor_value(
                        member[field], descriptor, env
                    )


def _merge_keyed_member(
    accumulated, incoming, class_name, logical_path, error_source,
    provenance_source, env, provenance, errors, compose=False,
):
    fields = schema_class_fields(env, class_name)
    for name, value in incoming.items():
        field_path = f"{logical_path}.{name}"
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
            _clear_provenance(provenance, field_path)
            continue
        if compose and name in accumulated and name in fields:
            accumulated[name] = _compose_value(
                accumulated[name], value, fields[name]['type'], env,
                field_path, provenance_source, provenance,
            )
            continue
        accumulated[name] = _replace_value(
            value, field_path, provenance_source, provenance
        )


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

            if name == 'windows':
                target = resolved.setdefault(name, {})
                for window, definition in value.items():
                    target[window] = _replace_value(
                        definition, f"windows.{window}", provenance_source,
                        provenance,
                    )
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
                        target[member_id] = _replace_value(
                            member, logical_path, provenance_source, provenance
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
                    target.append(
                        _replace_value(
                            member, logical_path, provenance_source, provenance
                        )
                    )
                else:
                    existing = target[positions[member_id]]
                    _merge_keyed_member(
                        existing, member, class_name, logical_path, source,
                        provenance_source, env, provenance, errors,
                        name in INHERITANCE_COMPOSING_COLLECTIONS,
                    )

    _materialize_inheritance_fragments(resolved, env)
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
            raw_layer, layer_label, env
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
        resolved, env, provenance, spec_label
    )
    errors.extend(final_errors)
    if errors:
        return None, errors, provenance
    return resolved, errors, provenance


def predicate_identifier_names(text):
    _ensure_predicate_binding()
    try:
        ast = parse_predicate(text)
    except PredicateError:
        return set()
    return ast_identifier_names(ast)


def ast_identifier_names(ast):
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


def numeric_expression_identifier_names(text):
    if not isinstance(text, str):
        return set()
    try:
        ast = parse_numeric_expression(text)
    except NumericExpressionError:
        return set()
    return ast_identifier_names(ast)


def aggregate_expression_identifier_names(text):
    if not isinstance(text, str):
        return set()
    try:
        ast = parse_aggregate_expression(text)
    except AggregateExpressionError:
        return set()
    return ast_identifier_names(ast)


class StringTemplateError(ValueError):
    def __init__(self, message, start, end, reason, placeholder=None):
        self.message = message
        self.span = (start, end)
        self.reason = reason
        self.placeholder = placeholder
        super().__init__(f"{message} at characters [{start}, {end})")


def parse_string_template(text):
    """Parse R012 and return its literal text and placeholder parts.

    A part is the unit the grammar scans: a `text` part carries the literal
    value a brace pair already unescaped, and a `placeholder` part carries
    the name it binds. Callers that only need the bindings use
    string_template_placeholders.
    """
    if not isinstance(text, str):
        raise StringTemplateError(
            'string template must be a string', 0, 0, 'invalid_template'
        )
    parts = []
    literal = []
    literal_start = 0

    def flush(end):
        if literal:
            parts.append({
                'kind': 'text',
                'value': ''.join(literal),
                'span': (literal_start, end),
            })
            literal.clear()

    index = 0
    while index < len(text):
        if text.startswith('{{', index) or text.startswith('}}', index):
            if not literal:
                literal_start = index
            literal.append(text[index])
            index += 2
            continue
        if text[index] == '}':
            raise StringTemplateError(
                'unmatched closing brace',
                index,
                index + 1,
                'unmatched_brace',
            )
        if text[index] != '{':
            if not literal:
                literal_start = index
            literal.append(text[index])
            index += 1
            continue
        end = text.find('}', index + 1)
        if end < 0:
            raise StringTemplateError(
                'unmatched opening brace',
                index,
                len(text),
                'unmatched_brace',
            )
        placeholder = text[index + 1:end]
        if '{' in placeholder or re.fullmatch(
            r'[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*',
            placeholder,
        ) is None:
            raise StringTemplateError(
                f"invalid placeholder {placeholder!r}",
                index,
                end + 1,
                'invalid_placeholder',
                placeholder,
            )
        flush(index)
        parts.append({
            'kind': 'placeholder',
            'name': placeholder,
            'span': (index + 1, end),
        })
        index = end + 1
    flush(len(text))
    return parts


def string_template_placeholders(text):
    """Return only the placeholder parts of a parsed template."""
    return [
        part
        for part in parse_string_template(text)
        if part['kind'] == 'placeholder'
    ]


def string_template_identifier_names(text):
    try:
        return {
            placeholder['name']
            for placeholder in string_template_placeholders(text)
        }
    except StringTemplateError:
        return set()


def collect_descriptor_references(data, descriptor, env, scope=None):
    return collect_type_references(data, descriptor['type'], env, scope)


# (class or registry keyword, field) positions where an `identifier` string
# names a declared dataset. Replaces the nominal `dataset_id` alias dispatch.
_IDENTIFIER_DATASET_FIELDS = frozenset(
    {
        ('root_class', 'base'),
        ('row_class', 'dataset'),
        ('intermediate_class', 'dataset'),
        ('lookup', 'dataset'),
    }
)

# (class, field) positions where an `identifier` string names a declared
# column. Replaces the nominal `column_name` alias dispatch.
_IDENTIFIER_COLUMN_FIELDS = frozenset(
    {
        ('column_class', 'name'),
        ('root_class', 'keys'),
        ('output_class', 'columns'),
    }
)


def _identifier_reference_kind(scope):
    """Return the reference kind for an `identifier` string at `scope`.

    `scope` is a (class or registry keyword, field) pair, or None. Returns
    'dataset', 'variable', or None when the position names no namespace.
    """
    if scope in _IDENTIFIER_DATASET_FIELDS:
        return 'dataset'
    if scope in _IDENTIFIER_COLUMN_FIELDS:
        return 'variable'
    return None


def collect_inline_class_references(data, fields, env, scope=None):
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
                collect_descriptor_references(
                    value, descriptor, env, (scope, name) if scope else None
                )
            )
    return references


def collect_single_type_references(data, type_ref, env, scope=None):
    if type_ref == 'variable' and isinstance(data, str):
        return {('variable', data)}
    if type_ref == 'identifier' and isinstance(data, str):
        kind = _identifier_reference_kind(scope)
        if kind is None:
            return set()
        return {(kind, data)}
    if type_ref == 'predicate':
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
            for name in aggregate_expression_identifier_names(data)
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
            references.update(collect_type_references(item, inner, env, scope))
        return references

    if type_ref.startswith('dict[') and type_ref.endswith(']'):
        if not isinstance(data, dict):
            return set()
        _, value_type = split_type_arguments(type_ref[5:-1])
        references = set()
        for value in data.values():
            references.update(
                collect_type_references(value, value_type, env, scope)
            )
        return references

    if type_ref in env.get('classes', {}):
        return collect_inline_class_references(
            data, env['classes'][type_ref], env, type_ref
        )

    alias = env.get('aliases', {}).get(type_ref)
    if alias is None:
        return set()
    registry_name = alias.get('registry')
    if registry_name is None:
        return collect_type_references(data, alias['type'], env, scope)
    if not isinstance(data, dict) or len(data) != 1:
        return set()
    keyword, payload = next(iter(data.items()))
    definition = env.get('registries', {}).get(registry_name, {}).get(keyword)
    if isinstance(definition, list):
        return collect_inline_class_references(payload, definition, env, keyword)
    if isinstance(definition, dict) and 'type' in definition:
        return collect_descriptor_references(payload, definition, env, scope)
    return set()


def collect_type_references(data, type_value, env, scope=None):
    members = _type_members(type_value)
    for member in members:
        if _type_matches(data, member, env):
            return collect_single_type_references(data, member, env, scope)
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
                    member[field], descriptors[field], env, (class_name, field)
                )
            )
    return references


def _apply_reference(
    reference, datasets, intermediates, live_columns, live_datasets, live_intermediates
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
    elif qualifier in intermediates and qualifier not in live_intermediates:
        live_intermediates.add(qualifier)
        changed = True
    return changed


def iter_type_reference_paths(data, type_value, env, path, scope=None):
    """Yield (name, path) for each variable reference under a schema type.

    This mirrors collect_type_references and adds the spec path each
    reference is written at, so a diagnostic can name where it appears.
    """
    for member in _type_members(type_value):
        if _type_matches(data, member, env):
            yield from _iter_single_type_reference_paths(
                data, member, env, path, scope
            )
            return


EXPRESSION_IDENTIFIER_READERS = {
    'predicate': lambda data: predicate_identifier_names(data),
    'numeric_expression': lambda data: numeric_expression_identifier_names(
        data
    ),
    'aggregate_expression': (
        lambda data: aggregate_expression_identifier_names(data)
    ),
    'string_template': lambda data: string_template_identifier_names(data),
}


def _iter_single_type_reference_paths(data, type_ref, env, path, scope=None):
    if type_ref == 'variable' and isinstance(data, str):
        yield data, path
        return
    if type_ref == 'identifier' and isinstance(data, str):
        # The old `column_name` alias yielded here; the old `dataset_id`
        # alias never did, so only the column scope yields.
        if _identifier_reference_kind(scope) == 'variable':
            yield data, path
        return

    reader = EXPRESSION_IDENTIFIER_READERS.get(type_ref)
    if reader is not None:
        for name in reader(data):
            yield name, path
        return

    if type_ref.startswith('list[') and type_ref.endswith(']'):
        if isinstance(data, list):
            inner = type_ref[5:-1].strip()
            for index, item in enumerate(data):
                yield from iter_type_reference_paths(
                    item, inner, env, f"{path}[{index}]", scope
                )
        return

    if type_ref.startswith('dict[') and type_ref.endswith(']'):
        if isinstance(data, dict):
            _, value_type = split_type_arguments(type_ref[5:-1])
            for key, value in data.items():
                yield from iter_type_reference_paths(
                    value, value_type, env, f"{path}.{key}", scope
                )
        return

    if type_ref in env.get('classes', {}):
        yield from _iter_inline_class_reference_paths(
            data, env['classes'][type_ref], env, path, type_ref
        )
        return

    alias = env.get('aliases', {}).get(type_ref)
    if alias is None:
        return
    registry_name = alias.get('registry')
    if registry_name is None:
        yield from iter_type_reference_paths(data, alias['type'], env, path, scope)
        return
    if not isinstance(data, dict) or len(data) != 1:
        return
    keyword, payload = next(iter(data.items()))
    definition = env.get('registries', {}).get(registry_name, {}).get(keyword)
    keyword_path = f"{path}.{keyword}"
    if isinstance(definition, list):
        yield from _iter_inline_class_reference_paths(
            payload, definition, env, keyword_path, keyword
        )
    elif isinstance(definition, dict) and 'type' in definition:
        yield from iter_type_reference_paths(
            payload, definition['type'], env, keyword_path, scope
        )


def _iter_inline_class_reference_paths(data, fields, env, path, scope=None):
    if not isinstance(data, dict):
        return
    descriptors = {
        name: descriptor
        for entry in fields
        for name, descriptor in entry.items()
    }
    for name, value in data.items():
        descriptor = descriptors.get(name)
        if descriptor is not None:
            yield from iter_type_reference_paths(
                value,
                descriptor['type'],
                env,
                f"{path}.{name}",
                (scope, name) if scope else None,
            )


def validate_retired_odm_item_references(
    spec, spec_label, spec_path, env, sources=None
):
    """Reject an ODM contextual item reference.

    A long-form ODM relation carries `ItemOID` and `Value`, and REQ-0096
    reads any other suffix on it as a complete ItemOID resolved against the
    row's ODM context. REQ-0131 states the replacement: read `Value` under a
    source `filter` on `ItemOID`, so the record the source reaches is
    written where a reviewer can see it. No specification in the repository
    still uses the retired form, so this check carries no exemption.
    """
    long_form = {
        dataset: fields
        for dataset, fields in dataset_type_catalog(
            spec, spec_path, env, sources
        ).items()
        if 'ItemOID' in fields and 'Value' in fields
    }
    if not long_form:
        return []

    errors = []
    for name, path in iter_type_reference_paths(
        spec, 'root_class', env, spec_label, 'root_class'
    ):
        if not isinstance(name, str) or '.' not in name:
            continue
        dataset, field = name.split('.', 1)
        fields = long_form.get(dataset)
        if fields is None or field in fields:
            continue
        errors.append(
            f"ERROR: {path}: retired_construct: {name!r} addresses an ODM "
            f"item through the variable name (REQ-0096); read {dataset}.Value "
            "under a source filter on ItemOID instead (REQ-0131)"
        )
    return sorted(set(errors))


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
    datasets = pruned.get('input')
    datasets = datasets if isinstance(datasets, dict) else {}
    columns = pruned.get('columns')
    column_entries = columns if isinstance(columns, list) else []
    column_map = {
        column.get('name'): column
        for column in column_entries
        if isinstance(column, dict) and isinstance(column.get('name'), str)
    }
    intermediates = pruned.get('intermediates')
    intermediate_entries = intermediates if isinstance(intermediates, list) else []
    lookup_map = {
        intermediate.get('id'): intermediate
        for intermediate in intermediate_entries
        if isinstance(intermediate, dict) and isinstance(intermediate.get('id'), str)
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
    pruned_datasets = pruned.get('input')
    if isinstance(pruned_datasets, dict):
        sole = [name for name in pruned_datasets if isinstance(name, str)]
        if len(sole) == 1:
            live_datasets.add(sole[0])
    live_intermediates = set()

    references = set()
    root_fields = schema_class_fields(env, 'root_class')
    if 'verifications' in pruned and 'verifications' in root_fields:
        references.update(
            collect_descriptor_references(
                pruned['verifications'],
                root_fields['verifications'],
                env,
                ('root_class', 'verifications'),
            )
        )
    if 'filter' in pruned and 'filter' in root_fields:
        references.update(
            collect_descriptor_references(
                pruned['filter'],
                root_fields['filter'],
                env,
                ('root_class', 'filter'),
            )
        )

    row_fields = schema_class_fields(env, 'row_class')
    for row in row_entries:
        if not isinstance(row, dict):
            continue
        driver = row.get('dataset')
        if isinstance(driver, str):
            live_datasets.add(driver)
        for field in ('group_by', 'filter'):
            if field in row and field in row_fields:
                references.update(
                    collect_descriptor_references(
                        row[field], row_fields[field], env, ('row_class', field)
                    )
                )

    for reference in references:
        _apply_reference(
            reference, datasets, lookup_map, live_columns, live_datasets,
            live_intermediates,
        )

    processed_columns = set()
    processed_intermediates = set()
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
                    live_datasets, live_intermediates,
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
                        live_datasets, live_intermediates,
                    )

        lookup_fields = schema_class_fields(env, 'intermediate_class')
        for intermediate_id in list(live_intermediates - processed_intermediates):
            processed_intermediates.add(intermediate_id)
            intermediate = lookup_map.get(intermediate_id)
            if intermediate is None:
                continue
            dataset_id = intermediate.get('dataset')
            if isinstance(dataset_id, str) and dataset_id not in live_datasets:
                live_datasets.add(dataset_id)
                changed = True
            for field, value in intermediate.items():
                if field in {'id', 'dataset'} or field not in lookup_fields:
                    continue
                refs = collect_descriptor_references(
                    value, lookup_fields[field], env, ('intermediate_class', field)
                )
                for reference in refs:
                    changed |= _apply_reference(
                        reference, datasets, lookup_map, live_columns,
                        live_datasets, live_intermediates,
                    )

        if not changed:
            break

    if isinstance(pruned.get('columns'), list):
        pruned['columns'] = [
            column for column in pruned['columns']
            if isinstance(column, dict) and column.get('name') in live_columns
        ]
    if isinstance(pruned.get('input'), dict):
        pruned['input'] = {
            name: source for name, source in pruned['input'].items()
            if name in live_datasets
        }
    if isinstance(pruned.get('intermediates'), list):
        pruned['intermediates'] = [
            intermediate for intermediate in pruned['intermediates']
            if isinstance(intermediate, dict) and intermediate.get('id') in live_intermediates
        ]
        if not pruned['intermediates']:
            del pruned['intermediates']
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


def column_dependency_names(column, rows, intermediates, env):
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
        intermediate = intermediates.get(qualifier)
        if intermediate is None:
            continue
        lookup_refs = collect_member_field_references(
            intermediate,
            'intermediate_class',
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
    intermediate_entries = spec.get('intermediates')
    intermediate_entries = intermediate_entries if isinstance(intermediate_entries, list) else []
    intermediates = {
        intermediate.get('id'): intermediate
        for intermediate in intermediate_entries
        if isinstance(intermediate, dict) and isinstance(intermediate.get('id'), str)
    }
    dependencies = {}
    errors = []
    for column in columns:
        name = column['name']
        dependencies[name] = column_dependency_names(
            column, rows, intermediates, env
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
        'input': 'dataset_class',
        'intermediates': 'intermediate_class',
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
        if name == 'input' and isinstance(value, dict):
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


def expand_spec_windows(spec, env, strict=True, provenance=None, label=""):
    from yamaa.schema.windows import expand_named_windows
    from yamaa.specification.diagnostics import SpecificationError
    from yamaa.specification.schema import SchemaBundle

    bundle = SchemaBundle(
        version=env.get('version', '1.0'),
        path=Path(env.get('root', '.')) / 'yaml' / 'schema.yaml',
        classes=env.get('classes', {}),
        aliases=env.get('aliases', {}),
        registries=env.get('registries', {}),
    )
    try:
        return expand_named_windows(
            spec, bundle, strict=strict, provenance=provenance
        ), []
    except SpecificationError as error:
        return spec, [
            validation_diagnostic(
                f"{label}.{item.spec_paths[0]}" if label else item.spec_paths[0],
                item.condition, item.condition,
                context=item.context,
            )
            for item in error.diagnostics
        ]


def finalize_resolved_inheritance(spec, env, provenance=None, label=""):
    resolved, errors = expand_spec_windows(spec, env, False, provenance, label)
    if errors:
        return resolved, errors
    resolved = prune_inheritance_collections(resolved, env)
    resolved, errors = expand_spec_windows(resolved, env, label=label)
    if errors:
        return resolved, errors
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


def example_entry_specs(example_dir: Path):
    paths = example_spec_paths(example_dir)
    named = set()
    for path in paths:
        try:
            with open(path, 'r', encoding='utf-8') as handle:
                spec = yaml.load(handle, Loader=UniqueKeyLoader)
        except (OSError, UnicodeError, yaml.YAMLError):
            continue
        if not isinstance(spec, dict):
            continue
        parents = spec.get('parents', [])
        if isinstance(parents, str):
            parents = [parents]
        for parent in parents:
            if isinstance(parent, str) and parent:
                named.add(Path(parent).name)
        inputs = spec.get('input', {})
        if isinstance(inputs, dict):
            for source in inputs.values():
                if (
                    isinstance(source, dict)
                    and isinstance(source.get('schema'), str)
                    and source['schema']
                ):
                    named.add(Path(source['schema']).name)
    return [path for path in paths if path.name not in named]


def example_artifact_owners(example_dir: Path):
    """Map each artifact an example's specs declare to the files declaring it.

    Only an entry and a producer kept beside it own a golden: a spec another
    reads through `input.*.schema` is part of the example, so the artifact it
    produces sits in `expected/` next to the entry's. An inheritance level
    owns nothing of its own; the entry that resolves it does.
    """
    documents = {}
    for path in example_spec_paths(example_dir):
        try:
            with open(path, 'r', encoding='utf-8') as handle:
                documents[path.name] = yaml.load(handle, Loader=UniqueKeyLoader)
        except (OSError, UnicodeError, yaml.YAMLError):
            continue
    owners_of = {path.name for path in example_entry_specs(example_dir)}
    for spec in documents.values():
        inputs = spec.get('input') if isinstance(spec, dict) else None
        if not isinstance(inputs, dict):
            continue
        for source in inputs.values():
            if isinstance(source, dict) and isinstance(source.get('schema'), str):
                owners_of.add(Path(source['schema']).name)
    artifacts = {}
    for name in sorted(owners_of):
        spec = documents.get(name)
        output = spec.get('output') if isinstance(spec, dict) else None
        if not isinstance(output, dict):
            continue
        for field in ('path', 'warning_log', 'verification_log'):
            if isinstance(output.get(field), str):
                artifact = PurePosixPath(output[field]).name
                artifacts.setdefault(artifact, set()).add(name)
    return artifacts


def valid_temporal_literal(kind, text):
    """True when text is a valid R016 temporal literal for kind.

    REQ-0672 permits a tagged temporal form (``{"date": "2020-01-01"}``)
    for a contract default, and REQ-0679 permits it for a call argument.
    The verdict comes from the runtime's strict parsers (``DateValue`` /
    ``DateTimeValue``, already imported for the csv-profile checks), so
    the lexical rule is read from the one implementation: ``YYYY-MM-DD``
    for ``date``, ``YYYY-MM-DDThh:mm[:ss]`` for ``datetime``, and both a
    real date on the calendar.
    """
    _ensure_csv_binding()
    if not isinstance(text, str):
        return False
    if kind == 'date':
        parsed_type = DateValue
    elif kind == 'datetime':
        parsed_type = DateTimeValue
    else:
        return False
    try:
        parsed_type.parse(text)
    except ValueError:
        return False
    return True


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
    return json.dumps(
        logical,
        ensure_ascii=False,
        allow_nan=False,
        sort_keys=True,
        separators=(',', ':'),
    )


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
            elif actual is not DERIVED_TYPE_UNKNOWN and actual != expected:
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


def _resolve_shared_function_contract(
    name, contract, contract_path, env_dir, schema_env, errors
):
    """Resolve a REQ-0669 shared contract reference for repository validation.

    Returns the effective contract dict (shared fields merged under the
    entry's own fields), or None when resolution failed; every failure
    appends an error.
    """
    required_inline = ('contract_version', 'description', 'params', 'returns')
    shareable_fields = required_inline + (
        'comparison_decimals', 'may_return_missing',
    )
    reference = contract.get('contract')
    if reference is None:
        missing = [field for field in required_inline if field not in contract]
        if missing:
            errors.append(
                f"ERROR: {contract_path}: function entry must declare its "
                f"contract inline or name a shared contract; missing "
                f"{', '.join(missing)}"
            )
            return None
        return contract
    if not isinstance(reference, str):
        errors.append(
            f"ERROR: {contract_path}.contract: shared contract reference "
            "must be a path"
        )
        return None
    inline = [field for field in shareable_fields if field in contract]
    if inline:
        errors.append(
            f"ERROR: {contract_path}: function entry must declare its "
            f"contract inline or name a shared contract, not both "
            f"({', '.join(inline)} also present)"
        )
        return None
    written = PurePosixPath(reference)
    if written.is_absolute() or any(part in ('.', '..') for part in written.parts):
        errors.append(
            f"ERROR: {contract_path}.contract: shared contract path must be "
            "local and normalized"
        )
        return None
    candidate = (env_dir / reference).resolve()
    try:
        candidate.relative_to(env_dir.resolve())
    except ValueError:
        errors.append(
            f"ERROR: {contract_path}.contract: shared contract path must "
            "stay inside the project root"
        )
        return None
    if not candidate.is_file():
        errors.append(
            f"ERROR: {contract_path}.contract: shared contract document "
            f"{reference!r} does not exist"
        )
        return None
    try:
        shared_document = yaml.safe_load(candidate.read_text(encoding='utf-8'))
    except yaml.YAMLError as exc:
        errors.append(
            f"ERROR: {contract_path}.contract: cannot parse shared contract "
            f"document {reference!r}: {exc}"
        )
        return None
    if not isinstance(shared_document, dict):
        errors.append(
            f"ERROR: {contract_path}.contract: shared contract document "
            f"{reference!r} must map function names to contracts"
        )
        return None
    for contract_name, shared_contract in shared_document.items():
        if not isinstance(shared_contract, dict):
            errors.append(
                f"ERROR: {contract_path}.contract: shared contract "
                f"{contract_name!r} in {reference!r} must be a mapping"
            )
            return None
        shared_errors = validate_type(
            shared_contract,
            ['shared_function_contract_class'],
            schema_env,
            f"{contract_path}.contract",
        )
        if shared_errors:
            errors.extend(shared_errors)
            return None
    if name not in shared_document:
        errors.append(
            f"ERROR: {contract_path}.contract: shared contract document "
            f"{reference!r} does not define {name!r}"
        )
        return None
    merged = dict(shared_document[name])
    for key, value in contract.items():
        if key != 'contract':
            merged[key] = value
    return merged


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
        contract = _resolve_shared_function_contract(
            name, contract, contract_path, environment_path.parent, schema_env, errors
        )
        if contract is None:
            continue
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
            if binding_args is None:
                # REQ-0683: an omitted mapping names each logical parameter
                # for its host argument.
                binding_args = {name: name for name in names}
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
    examples_dir = root / 'benchmarks'
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
            if not isinstance(contract, dict):
                continue
            # Resolution failures are reported by per-environment
            # validation; this pass only compares identities.
            contract = _resolve_shared_function_contract(
                name,
                contract,
                f"{environment_path}.functions.{name}",
                environment_path.parent,
                schema_env,
                [],
            )
            if contract is None:
                continue
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

        driver = row.get('dataset', default_driver_dataset(spec))
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
    """Validate cross-field names and uniqueness required by R002/R005/R003."""
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

    datasets = spec.get('input')
    dataset_names = set(datasets) if isinstance(datasets, dict) else set()
    domain = spec.get('domain')
    if isinstance(domain, str) and domain in dataset_names:
        for path in (f"input.{domain}", 'domain'):
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
                validation_diagnostic(
                    f"{spec_label}.output.path",
                    'unknown_artifact_profile',
                    f"{declared_path!r} names no profile; R020 maps "
                    + ', '.join(sorted(ARTIFACT_PROFILES))
                    + " and nothing else",
                    context={
                        'path': declared_path,
                        'permitted': sorted(ARTIFACT_PROFILES),
                    },
                )
            )
        # R020 gives each declared path its own profile and requires the
        # three to name three different files.
        for field in ('warning_log', 'verification_log'):
            sidecar_path = output.get(field)
            if (
                isinstance(sidecar_path, str)
                and artifact_profile({'path': sidecar_path}) is None
            ):
                errors.append(
                    validation_diagnostic(
                        f"{spec_label}.output.{field}",
                        'unknown_artifact_profile',
                        f"{sidecar_path!r} names no profile; R020 maps "
                        + ', '.join(sorted(ARTIFACT_PROFILES))
                        + " and nothing else",
                        context={
                            'path': sidecar_path,
                            'permitted': sorted(ARTIFACT_PROFILES),
                        },
                    )
                )
        for left, right, subject in (
            ('path', 'warning_log', 'primary artifact and warning log'),
            (
                'path',
                'verification_log',
                'primary artifact and verification log',
            ),
            (
                'warning_log',
                'verification_log',
                'warning log and verification log',
            ),
        ):
            left_path, right_path = output.get(left), output.get(right)
            if (
                not isinstance(left_path, str)
                or not isinstance(right_path, str)
                or left_path != right_path
            ):
                continue
            for field in (left, right):
                errors.append(
                    validation_diagnostic(
                        f"{spec_label}.output.{field}",
                        'artifact_path_collision',
                        f'{subject} must name different paths',
                        context={'path': left_path},
                    )
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
        if (
            all(isinstance(name, str) for name in output_columns)
            and isinstance(keys, list)
        ):
            output_names = set(output_columns)
            for index, key in enumerate(keys):
                if isinstance(key, str) and key not in output_names:
                    errors.append(
                        validation_diagnostic(
                            f"{spec_label}.keys[{index}]",
                            'internal_column_in_keys',
                            f"key column {key!r} is not in output.columns",
                            context={'column': key},
                        )
                    )

    order_by = output.get('order_by') if isinstance(output, dict) else None
    if isinstance(order_by, list):
        order_variables = set()
        for index, term in enumerate(order_by):
            variable = order_term_variable(term)
            if variable is None:
                continue
            if variable in order_variables:
                errors.append(
                    validation_diagnostic(
                        f"{spec_label}.output.order_by[{index}]",
                        'duplicate_order_term',
                        f"duplicate order term {variable!r}",
                        context={'column': variable},
                    )
                )
            elif variable not in declared_columns:
                errors.append(
                    validation_diagnostic(
                        f"{spec_label}.output.order_by[{index}]",
                        'undeclared_column',
                        f"undeclared column {variable!r}",
                        context={'column': variable},
                    )
                )
            order_variables.add(variable)

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

    intermediates = spec.get('intermediates')
    if isinstance(intermediates, list):
        intermediate_ids = [
            intermediate.get('id') for intermediate in intermediates
            if isinstance(intermediate, dict) and isinstance(intermediate.get('id'), str)
        ]
        duplicate_errors(intermediate_ids, 'intermediates', 'intermediate id')
        reserved_names = dataset_names | ({domain} if isinstance(domain, str) else set())
        for index, intermediate in enumerate(intermediates):
            if not isinstance(intermediate, dict):
                continue
            intermediate_id = intermediate.get('id')
            if isinstance(intermediate_id, str) and intermediate_id in reserved_names:
                conflict_path = (
                    f"input.{intermediate_id}"
                    if intermediate_id in dataset_names
                    else 'domain'
                )
                for path in (
                    f"intermediates[{index}].id", conflict_path
                ):
                    errors.append(
                        validation_diagnostic(
                            f"{spec_label}.{path}",
                            'duplicate_identifier',
                            f"identifier {intermediate_id!r} conflicts with a "
                            'dataset or domain',
                            context={'identifier': intermediate_id},
                        )
                    )
            lookup_dataset = intermediate.get('dataset')
            if isinstance(lookup_dataset, str) and lookup_dataset not in dataset_names:
                errors.append(
                    f"ERROR: {spec_label}.intermediates[{index}].dataset: "
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
    'resource_path_not_relative': 'names no approved location',
    'resource_path_uri_scheme': 'declares a URI scheme',
    'resource_path_not_normalized': 'is not normalized',
    'resource_path_symlink': 'passes through a symbolic link',
    'resource_path_outside_project': 'resolves outside the project root',
    'resource_path_missing': 'does not exist',
    'resource_path_not_regular_file': 'is not a regular file',
    'resource_path_content_changed': 'changed after it was validated',
}

URI_SCHEME_PATTERN = re.compile(r'^[A-Za-z][A-Za-z0-9+.-]*:')
DRIVE_ROOT_PATTERN = re.compile(r'^[A-Za-z]:/')


PROJECT_CONFIGURATION_NAME = 'yamaa-project.yaml'
PROJECT_CONFIGURATION_FIELDS = {'version', 'data_roots'}


def read_project_configuration(project_root, label=None):
    """Return (data_roots, errors) for the configuration at a named root.

    REQ-0768 gives a runner that names the root the configuration sitting at
    that root and no other. REQ-0795 fails a configuration a run cannot start
    from; a root that holds none is a study that declared nothing.
    """
    directory = Path(project_root)
    path = directory / PROJECT_CONFIGURATION_NAME
    if not path.is_file():
        return (), []
    label = label or PROJECT_CONFIGURATION_NAME
    try:
        with open(path, 'r', encoding='utf-8') as handle:
            document = yaml.load(handle, Loader=UniqueKeyLoader)
    except (OSError, UnicodeError, yaml.YAMLError) as exc:
        return (), [f"ERROR: {label}: {exc}"]
    if not isinstance(document, dict):
        return (), [f"ERROR: {label}: expected a mapping"]

    errors = []
    for field in sorted(set(document) - PROJECT_CONFIGURATION_FIELDS):
        errors.append(f"ERROR: {label}.{field}: unknown field")
    if document.get('version') != '1.0':
        errors.append(f"ERROR: {label}.version: expected '1.0'")

    declared = document.get('data_roots')
    if declared is None:
        declared = []
    if not (
        isinstance(declared, list)
        and all(isinstance(item, str) and item for item in declared)
    ):
        errors.append(
            f"ERROR: {label}.data_roots: expected a list of non-empty paths"
        )
        return (), errors

    roots = []
    for item in declared:
        candidate = Path(item)
        if not candidate.is_absolute():
            # A relative entry names a directory beside the study, read from
            # the project root the configuration itself marks.
            candidate = directory / candidate
        try:
            resolved = candidate.resolve(strict=True)
        except OSError:
            resolved = None
        if resolved is None or not resolved.is_dir():
            errors.append(
                f"ERROR: {label}.data_roots: {item!r} is not an existing "
                "directory"
            )
            continue
        # The spelling the study wrote is kept, not its canonical form: a
        # rooted path repeats that spelling, and REQ-0781 matches it there.
        roots.append(candidate)
    return tuple(roots), errors


def project_data_roots(project_root):
    """Data roots the configuration at a named project root approves."""
    roots, _ = read_project_configuration(project_root)
    return roots


def validate_project_configurations(root: Path):
    """Report every project configuration the repository cannot start from."""
    errors = []
    for path in sorted(root.rglob(PROJECT_CONFIGURATION_NAME)):
        if any(
            part in {'.git', '.claude', '.venv', '.venv-docs', 'node_modules'}
            for part in path.parts
        ):
            continue
        _, configuration_errors = read_project_configuration(
            path.parent, label=str(path.relative_to(root))
        )
        errors.extend(configuration_errors)
    return errors


def rooted_project_segments(written):
    """Split a rooted written path into its marker and segments, or None.

    REQ-0773 spells a rooted path with a leading separator or with one ASCII
    letter and ':/'. The marker leads the returned segments, so a path rooted
    one way never repeats a root spelled the other way.
    """
    if not isinstance(written, str):
        return None
    if written.startswith('/'):
        marker, head = '/', ''
    else:
        match = DRIVE_ROOT_PATTERN.match(written)
        if match is None:
            return None
        marker, head = match.group(0), match.group(0)[:-1]
    remainder = written[len(marker):]
    return (head, *(remainder.split('/') if remainder else ()))


def classify_written_project_path(written):
    """Return the R021 condition a written project path violates, if any.

    REQ-0791 fixes the order: a scheme (REQ-0775), then a backslash (REQ-0776),
    then an empty segment (REQ-0777), then a dot segment in a rooted path
    (REQ-0778). Nothing here consults the filesystem.
    """
    if not isinstance(written, str):
        return 'resource_path_not_normalized'
    segments = rooted_project_segments(written)
    if segments is None and URI_SCHEME_PATTERN.match(written):
        return 'resource_path_uri_scheme'
    if '\\' in written:
        return 'resource_path_not_normalized'
    parts = written.split('/') if segments is None else list(segments[1:])
    if not parts or any(part == '' for part in parts):
        return 'resource_path_not_normalized'
    if segments is not None and any(part in ('.', '..') for part in parts):
        return 'resource_path_not_normalized'
    return None


def root_spellings(written, resolved):
    """Return the rooted segment spellings that name one approved root."""
    spellings = []
    for candidate in (resolved, written):
        segments = rooted_project_segments(Path(candidate).as_posix())
        if segments is not None and segments not in spellings:
            spellings.append(segments)
    return spellings


def walk_below_anchor(written, anchor, segments, containment):
    """Walk the components below an anchor, rejecting a link at each one."""
    current = anchor
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
        current.resolve(strict=True).relative_to(containment)
    except (OSError, ValueError):
        return None, 'resource_path_outside_project'
    return current, None


def resolve_project_path(written, base_dir, project_root, data_roots=()):
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

    segments = rooted_project_segments(written)
    if segments is not None:
        # REQ-0781: a rooted path is anchored at the approved root whose
        # leading segments it repeats, and the anchor is canonical, so the
        # link a platform puts in front of a system directory is resolved
        # once here rather than rejected below.
        anchor = None
        depth = -1
        for candidate in (project_root, *data_roots):
            try:
                resolved = Path(candidate).resolve(strict=True)
            except OSError:
                continue
            if not resolved.is_dir():
                continue
            for spelling in root_spellings(candidate, resolved):
                size = len(spelling)
                if size > depth and segments[:size] == spelling:
                    anchor, depth = resolved, size
        if anchor is None:
            return None, 'resource_path_not_relative'
        remainder = segments[depth:]
        if not remainder:
            return None, 'resource_path_not_regular_file'
        return walk_below_anchor(written, anchor, remainder, anchor)

    approved = [root]
    for candidate in data_roots:
        try:
            resolved = Path(candidate).resolve(strict=True)
        except OSError:
            continue
        if resolved.is_dir() and resolved not in approved:
            approved.append(resolved)

    # REQ-0781: the writing layer's directory is the first anchor. From a
    # directory inside an approved root, a traversal that leaves every root is
    # terminal; a layer stored outside every root is an anchor only where its
    # path climbs into one.
    base = Path(base_dir).resolve()
    anchors = []
    first = textual_location(written, base)
    first_root = containing_root(first, approved)
    if containing_root(base, approved) is not None and first_root is None:
        return None, 'resource_path_outside_project'
    if first_root is not None:
        anchors.append((first_root, first))
    # REQ-0780/REQ-1246: then the project root and each data root in order,
    # each reading the spelling as the layer wrote it. A root the spelling
    # climbs out of does not participate.
    for candidate in approved:
        location = textual_location(written, candidate)
        owner = containing_root(location, approved)
        if owner is not None and (owner, location) not in anchors:
            anchors.append((owner, location))
    if not anchors:
        return None, 'resource_path_outside_project'
    for owner, location in anchors:
        segments = location.relative_to(owner).parts
        if not segments:
            return None, 'resource_path_not_regular_file'
        accepted, condition = walk_below_anchor(written, owner, segments, owner)
        if condition != 'resource_path_missing':
            return accepted, condition
    return None, 'resource_path_missing'


def textual_location(written, start):
    """Resolve a relative written path against a directory, text only."""
    parts = list(start.parts)
    for segment in written.split('/'):
        if segment == '.':
            continue
        if segment == '..':
            if len(parts) > 1:
                parts.pop()
        else:
            parts.append(segment)
    return Path(*parts)


def containing_root(location, roots):
    """Return the innermost approved root a location sits under, if any."""
    matches = [
        root for root in roots if location == root or root in location.parents
    ]
    return max(matches, key=lambda root: len(root.parts)) if matches else None


def resolved_source_files(spec, spec_path, project_root, provenance):
    """Map (dataset, field) to the file each relative input path reaches."""
    files = {}
    datasets = spec.get('input') if isinstance(spec, dict) else None
    if spec_path is None or not isinstance(datasets, dict):
        return files
    data_roots = project_data_roots(project_root)
    for dataset_id, source in datasets.items():
        fields = {'path': source} if isinstance(source, str) else source
        if not isinstance(fields, dict):
            continue
        for field in ('path', 'schema'):
            written = fields.get(field)
            if not isinstance(written, str):
                continue
            base_dir, spelling = layer_written_path(
                written, f"input.{dataset_id}.{field}", provenance, spec_path
            )
            accepted, condition = resolve_project_path(
                spelling, base_dir, project_root, data_roots
            )
            if condition is None:
                files[(dataset_id, field)] = accepted
    return files


def layer_written_path(written, logical, provenance, spec_path):
    """Return the directory and spelling the layer that wrote a path used.

    REQ-0780 retries a relative path in its layer's own spelling, which the
    rebased form of REQ-0636 no longer shows. Provenance names the layer, and
    the rebased form still names the same location from the entry file.
    """
    source = (provenance or {}).get(logical)
    entry_directory = spec_path.parent
    if source is None or rooted_project_segments(written) is not None:
        return entry_directory, written
    layer_directory = Path(source).resolve().parent
    entry_resolved = entry_directory.resolve()
    if layer_directory == entry_resolved:
        return entry_directory, written
    target = os.path.normpath(entry_resolved / written)
    return layer_directory, Path(os.path.relpath(target, layer_directory)).as_posix()


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

    def __init__(self, content):
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
        accepted = self._by_identity.get(identity)
        if accepted is None:
            self.reads += 1
            snapshot = ProjectSnapshot(content)
            self._by_identity[identity] = snapshot
            return snapshot, None
        if accepted.content != content:
            return None, 'resource_path_content_changed'
        return accepted, None


SOURCE_PROFILES = {'.csv': 'csv', '.parquet': 'parquet'}


def validate_spec_contracts(
    spec, spec_label, spec_path=None, project_root=None, snapshots=None,
    provenance=None, sources=None,
):
    """Validate static cross-field contracts from normative rules."""
    errors = []
    output = spec.get('output')

    rows = spec.get('rows')
    row_entries = rows if isinstance(rows, list) else []
    default_driver = default_driver_dataset(spec)
    datasets = spec.get('input')
    dataset_names = (
        [name for name in datasets if isinstance(name, str)]
        if isinstance(datasets, dict)
        else []
    )
    full_spec = all(
        field in spec
        for field in ('domain', 'input', 'keys', 'output', 'columns')
    )
    if full_spec and not row_entries and not isinstance(default_driver, str):
        errors.append(
            f"ERROR: {spec_label}.base: base is required when rows is absent "
            "or empty and more than one dataset is declared"
        )
    for index, row in enumerate(row_entries):
        if (
            full_spec
            and isinstance(row, dict)
            and 'dataset' not in row
            and len(dataset_names) > 1
        ):
            errors.append(
                f"ERROR: {spec_label}.rows[{index}].dataset: row requires an "
                "explicit dataset when more than one dataset is declared"
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

    intermediates = spec.get('intermediates')
    if isinstance(intermediates, list):
        catalog = dataset_type_catalog(spec, spec_path, sources=sources)
        root_keys = spec.get('keys')
        root_keys = root_keys if isinstance(root_keys, list) else []
        for index, intermediate in enumerate(intermediates):
            if not isinstance(intermediate, dict):
                continue
            path = f"{spec_label}.intermediates[{index}]"
            # `order_by` and `keep` pair with each other; `source`/`key`
            # pairing is checked below now that both are optional (REQ-0153).
            if ('order_by' in intermediate) != ('keep' in intermediate):
                has_order = 'order_by' in intermediate
                errors.append(
                    validation_diagnostic(
                        path,
                        'unpaired_fields',
                        'order_by and keep must be declared together',
                        context={
                            'intermediate': intermediate.get('id'),
                            'declared': ['order_by'] if has_order else ['keep'],
                            'missing': ['keep'] if has_order else ['order_by'],
                        },
                    )
                )
            sources = intermediate.get('key_base')
            keys = intermediate.get('key')
            if (
                isinstance(sources, list)
                and isinstance(keys, list)
            ):
                if sources == keys:
                    # REQ-0155: key_base must not repeat the key names.
                    errors.append(
                        validation_diagnostic(
                            path,
                            'redundant_key_base',
                            'key_base repeats the key names; omit it',
                            context={
                                'key_base': sources,
                                'key': keys,
                            },
                        )
                    )
                elif len(sources) != len(keys):
                    errors.append(
                        validation_diagnostic(
                            path,
                            'source_key_length_mismatch',
                            f"key_base has {len(sources)} value(s), key has "
                            f"{len(keys)}",
                            context={
                                'key_base': sources,
                                'key': keys,
                                'key_base_count': len(sources),
                                'key_count': len(keys),
                            },
                        )
                    )
            dataset = intermediate.get('dataset')
            if keys is None and isinstance(dataset, str):
                # REQ-0153: an omitted key is inferred from the output keys
                # that name a column of the intermediate dataset.
                fields = catalog.get(dataset, {})
                applicable = [key for key in root_keys if key in fields]
                if not applicable:
                    errors.append(
                        validation_diagnostic(
                            path,
                            'no_applicable_keys',
                            f"no output key names a column of {dataset!r}",
                            context={
                                'dataset': dataset,
                                'keys': list(root_keys),
                                'hint': 'declare the `key` explicitly',
                            },
                        )
                    )

    verification_ids = []
    warning_paths = []
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
            if payload.get('severity', 'error') == 'warning':
                warning_paths.append(f"{path}.severity")
            if keyword in {'all_or_none', 'implies', 'assert', 'row_count'}:
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
                min_fraction = payload.get('min_fraction')
                max_fraction = payload.get('max_fraction')
                for field, bound in (
                    ('min_fraction', min_fraction),
                    ('max_fraction', max_fraction),
                ):
                    if bound is not None and (
                        type(bound) not in (int, float) or not 0 <= bound <= 1
                    ):
                        errors.append(
                            f"ERROR: {path}.{field}: must be between 0 and 1"
                        )
                if all(bound is None for bound in (
                    minimum, maximum, min_fraction, max_fraction
                )):
                    errors.append(
                        f"ERROR: {path}: requires at least one bound"
                    )
                if (
                    type(minimum) is int
                    and type(maximum) is int
                    and minimum > maximum
                ):
                    errors.append(f"ERROR: {path}: min must not exceed max")
                if (
                    type(min_fraction) in (int, float)
                    and type(max_fraction) in (int, float)
                    and min_fraction > max_fraction
                ):
                    errors.append(
                        f"ERROR: {path}: min_fraction must not exceed max_fraction"
                    )
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
            if payload.get('severity', 'error') == 'warning':
                warning_paths.append(f"{path}.severity")
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

    if warning_paths and (
        not isinstance(output, dict)
        or not isinstance(output.get('warning_log'), str)
    ):
        errors.append(
            validation_diagnostic(
                f"{spec_label}.output.warning_log",
                'missing_warning_log',
                'warning verifications require a governed warning log',
                context={'warnings': warning_paths},
            )
        )

    datasets = spec.get('input')
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
            path = f"{spec_label}.input.{dataset_id}"
            base_dir, written = layer_written_path(
                source_path, f"input.{dataset_id}.path", provenance, spec_path
            )
            resolved, condition = resolve_project_path(
                written, base_dir, project_root,
                project_data_roots(project_root),
            )
            if (
                condition == 'resource_path_missing'
                and isinstance(source, dict)
                and isinstance(source.get('schema'), str)
            ):
                # REQ-0521: the producing specification writes this artifact
                # before the consumer reads it, so it need not exist yet;
                # validate_producing_specs checks its header when it does.
                continue
            if condition is not None:
                errors.append(
                    resource_path_error(f"{path}.path", source_path, condition)
                )
                continue
            # R023 selects the profile from the written path, so an extension
            # it does not map is rejected before the source is read.
            profile = SOURCE_PROFILES.get(resolved.suffix.lower())
            if profile is None:
                errors.append(validation_diagnostic(
                    f"{path}.path",
                    'source_profile_unknown',
                    f"{source_path!r} names no source profile",
                    context={'path': source_path},
                ))
                continue
            snapshot, condition = snapshots.read(resolved)
            if condition is not None:
                errors.append(
                    resource_path_error(f"{path}.path", source_path, condition)
                )
                continue
            if not isinstance(types, dict):
                continue
            if profile == 'parquet':
                for field in sorted(types, key=str):
                    errors.append(validation_diagnostic(
                        f"{path}.types.{field}",
                        'redundant_field_type',
                        'field type is already supplied by the Parquet schema',
                        context={
                            'dataset': dataset_id,
                            'field': field,
                            'type': types[field],
                        },
                    ))
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


def dataset_type_catalog(spec, spec_path, env=None, sources=None):
    """Return statically discoverable field types for each dataset.

    ``sources`` maps a dataset to the files its relative ``path`` and
    ``schema`` reach under REQ-0780, which an inherited path absent beside
    its layer finds under the project root rather than beside the entry.
    """
    catalog = {}
    datasets = spec.get('input')
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
            # Only a relative path is read here; R021 decides a rooted one
            # against the approved roots before anything opens it.
            and rooted_project_segments(producer_path) is None
        ):
            resolved = (sources or {}).get((dataset_id, 'schema')) or (
                spec_path.parent / producer_path
            )
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
            and rooted_project_segments(source_path) is None
        ):
            resolved = (sources or {}).get((dataset_id, 'path')) or (
                spec_path.parent / source_path
            )
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


def validate_spec_functions(spec, spec_label, spec_path, schema_env, sources=None):
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
                calls,
                column_types,
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
    beside = spec_path.parent / 'environment.yaml'
    found = [beside] if beside.exists() else []
    for language in ('python', 'r'):
        candidate = spec_path.parent / language / 'environment.yaml'
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
    """Validate one specification's calls against one project root."""
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
        else Path(__file__).resolve().parents[3]
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
    # Resolve REQ-0669 shared contracts for the checks below; resolution
    # failures were already reported by validate_project_environment above.
    resolved_functions = {}
    for function_name, function_contract in functions.items():
        if not isinstance(function_contract, dict):
            continue
        resolved = _resolve_shared_function_contract(
            function_name,
            function_contract,
            f"{environment_path}.functions.{function_name}",
            environment_path.parent,
            environment_schema,
            [],
        )
        if resolved is not None:
            resolved_functions[function_name] = resolved
    functions = resolved_functions

    datasets = dataset_type_catalog(spec, spec_path, schema_env, sources)
    intermediates = intermediate_type_catalog(spec, datasets, spec_label)

    def resolve_variable(name):
        if not isinstance(name, str):
            return None
        if '.' not in name:
            return column_types.get(name)
        qualifier, field = name.split('.', 1)
        relation = datasets.get(qualifier, intermediates.get(qualifier))
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


#: Predicate operand type for a derived name the repository validator
#: cannot type statically. The name is known (no `unknown_field`), and it
#: compares as an untyped value (no `incompatible_input_type`).
DERIVED_TYPE_UNKNOWN = object()


def _compute_expression_type(derivation, donor_fields, path):
    """Infer the R010 type of a `compute: {expr: ...}` derivation."""
    if not isinstance(derivation, dict) or len(derivation) != 1:
        return None
    keyword, payload = next(iter(derivation.items()))
    if keyword != 'compute' or not isinstance(payload, dict):
        return None
    text = payload.get('expr')
    if not isinstance(text, str):
        return None
    try:
        ast = parse_numeric_expression(text)
    except NumericExpressionError:
        return None

    def resolve(identifier):
        _qualifier, separator, field = identifier.partition('.')
        field_type = donor_fields.get(field if separator else identifier)
        if field_type is None:
            return None, (
                'unknown_field',
                f"unknown identifier {identifier!r}",
                {'identifier': identifier},
            )
        return field_type, None

    inferred, _ = validate_numeric_expression_ast(ast, path, text, resolve)
    return inferred if inferred not in (None, '<invalid>') else None


def _intermediate_derived_field_types(intermediate, donor_fields, operation_path):
    """Map an intermediate's derivation names to predicate operand types.

    Derived values augment each donor record before `filter`, `order_by`,
    and `columns` (REQ-1185), so a dataset-qualified derived name must
    resolve in the intermediate's filter. Numeric `compute` expressions
    keep their inferred type; anything else is known-but-untyped.
    """
    derivations = intermediate.get('derivations')
    if not isinstance(derivations, dict):
        return {}
    typed = {}
    for name, derivation in derivations.items():
        if not isinstance(name, str):
            continue
        inferred = _compute_expression_type(
            derivation, donor_fields, f"{operation_path}.derivations.{name}"
        )
        typed[name] = inferred if inferred is not None else DERIVED_TYPE_UNKNOWN
    return typed


def intermediate_type_catalog(spec, datasets, spec_label):
    """Return stored and derived fields exposed by each named intermediate."""
    intermediates = {}
    for index, intermediate in enumerate(spec.get('intermediates') or []):
        if not isinstance(intermediate, dict):
            continue
        intermediate_id = intermediate.get('id')
        dataset_id = intermediate.get('dataset')
        if not isinstance(intermediate_id, str) or not isinstance(dataset_id, str):
            continue
        fields = dict(datasets.get(dataset_id, {}))
        fields.update(_intermediate_derived_field_types(
            intermediate, fields, f"{spec_label}.intermediates[{index}]"
        ))
        intermediates[intermediate_id] = fields
    return intermediates


def predicate_resolver(unqualified=None, qualified=None):
    unqualified = unqualified or {}
    qualified = qualified or {}

    def resolve(name):
        if '.' not in name:
            return unqualified.get(name)
        qualifier, field = name.split('.', 1)
        relation = qualified.get(qualifier)
        if not isinstance(relation, dict):
            return None
        if field in relation:
            return relation[field]
        if (
            '.' in field
            and 'ItemOID' in relation
            and 'Value' in relation
        ):
            return relation['Value']
        return None

    resolve.unqualified = unqualified
    resolve.qualified = qualified
    return resolve


def validate_predicate_at(text, path, resolver):
    _ensure_predicate_binding()
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


def source_filter_errors(payload, path, datasets):
    """Check a source `filter` against the right-side records it selects.

    REQ-0132 keeps the predicate inside the dataset the source reads, so it
    resolves against that dataset's fields alone and never against the
    output columns the reading derivation may name.
    """
    if not isinstance(payload, dict) or not isinstance(payload.get('filter'), str):
        return []
    variable = payload.get('variable')
    qualifier = (
        variable.split('.', 1)[0]
        if isinstance(variable, str) and '.' in variable
        else None
    )
    if qualifier is None:
        # REQ-0149: an unqualified source reads one completed output column,
        # so the predicate has no records to select among.
        return [
            validation_diagnostic(
                f"{path}.filter",
                'prohibited_construct',
                'a source reading one completed value selects among no records',
                context={'identifier': variable},
            )
        ]
    right_resolver = predicate_resolver(qualified={qualifier: datasets.get(qualifier, {})})
    return validate_predicate_at(payload['filter'], f"{path}.filter", right_resolver)


def validate_expression_predicates(
    expression, path, resolver, datasets
):
    errors = []
    if not isinstance(expression, dict) or len(expression) != 1:
        return errors
    keyword, payload = next(iter(expression.items()))

    if keyword == 'case' and isinstance(payload, list):
        for index, item in enumerate(payload):
            if not isinstance(item, dict):
                continue
            item_path = f"{path}.case[{index}]"
            if 'otherwise' in item:
                errors.extend(
                    validate_expression_predicates(
                        item['otherwise'],
                        f"{item_path}.otherwise",
                        resolver,
                        datasets,
                    )
                )
                continue
            if isinstance(item.get('when'), str):
                errors.extend(
                    validate_predicate_at(
                        item['when'], f"{item_path}.when", resolver
                    )
                )
            errors.extend(
                validate_expression_predicates(
                    item.get('then'),
                    f"{item_path}.then",
                    resolver,
                    datasets,
                )
            )

    elif keyword == 'flag' and isinstance(payload, (dict, str)):
        # REQ-1256: a bare predicate string is the condition. R006 expands
        # it to the mapping form when the engine loads the spec, so both
        # spellings report at the canonical condition path.
        condition = (
            payload if isinstance(payload, str) else payload.get('condition')
        )
        condition_path = f"{path}.flag.condition"
        if isinstance(condition, str):
            errors.extend(
                validate_predicate_at(
                    condition, condition_path, resolver
                )
            )

    elif keyword == 'source' and isinstance(payload, dict):
        errors.extend(
            source_filter_errors(payload, f"{path}.source", datasets)
        )

    elif keyword == 'mapping' and isinstance(payload, dict):
        errors.extend(
            source_filter_errors(
                payload.get('source'), f"{path}.mapping.source", datasets
            )
        )

    elif keyword == 'first_available' and isinstance(payload, dict):
        sources = payload.get('sources')
        if isinstance(sources, list):
            for index, source in enumerate(sources):
                errors.extend(
                    source_filter_errors(
                        source, f"{path}.first_available.sources[{index}]", datasets
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

    elif keyword == 'lookup' and isinstance(payload, dict):
        if isinstance(payload.get('filter'), str):
            errors.extend(validate_predicate_at(
                payload['filter'], f"{path}.lookup.filter",
                predicate_resolver(qualified=datasets),
            ))

    elif keyword in {
        'row_number', 'rank', 'row_value', 'previous_non_missing', 'locf',
        'baseline_flag',
    } and isinstance(payload, dict):
        window = payload.get('window')
        window_filter = window.get('filter') if isinstance(window, dict) else None
        if isinstance(window_filter, str):
            errors.extend(
                validate_predicate_at(
                    window_filter, f"{path}.{keyword}.window.filter", resolver
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
    return errors


def validate_lookup_filter_scopes(spec, spec_label, env):
    """Bind correlated lookup predicates to the driver at each use site."""
    errors = []
    entries = {
        item['id']: (item, f"{spec_label}.intermediates[{index}].filter")
        for index, item in enumerate(spec.get('intermediates') or [])
        if isinstance(item, dict) and isinstance(item.get('id'), str)
    }
    rows = spec.get('rows') or []
    column_scopes = [
        (row.get('dataset', default_driver_dataset(spec)), row.get('group_by'))
        for row in rows
    ] or [(default_driver_dataset(spec), None)]

    def check(payload, path, scopes):
        dataset = payload.get('dataset')
        for name in predicate_identifier_names(payload.get('filter')):
            if '.' not in name or name.split('.', 1)[0] == dataset:
                continue  # Existence and mandatory qualification checked separately.
            for driver, group_by in scopes:
                if name.split('.', 1)[0] != driver:
                    errors.append(validation_diagnostic(
                        path, 'unknown_field',
                        'a correlated filter must name the current driver',
                        context={'identifier': name},
                    ))
                elif group_by is not None and name not in group_by:
                    errors.append(validation_diagnostic(
                        path, 'ungrouped_driver_field',
                        'a correlated filter must read a driver group key',
                        context={'identifier': name},
                    ))

    def visit(node, path, scopes):
        if isinstance(node, dict):
            for key, value in node.items():
                if key == 'lookup' and isinstance(value, dict):
                    check(value, f"{path}.lookup.filter", scopes)
                visit(value, f"{path}.{key}", scopes)
        elif isinstance(node, list):
            for index, value in enumerate(node):
                visit(value, f"{path}[{index}]", scopes)

    def derivation(value, path, scopes):
        references = (
            [name for kind, name in collect_type_references(value, 'derivation', env)
             if kind == 'variable']
            if env is not None else derive_binding_reference_names(value)
        )
        for name in references:
            qualifier = name.split('.', 1)[0]
            if qualifier in entries:
                payload, filter_path = entries[qualifier]
                check(payload, filter_path, scopes)
        visit(value, path, scopes)

    for column in spec.get('columns') or []:
        if 'derivation' in column:
            derivation(column['derivation'],
                       f"{spec_label}.columns.{column['name']}.derivation",
                       column_scopes)
    for index, row in enumerate(rows):
        scopes = [(row.get('dataset', default_driver_dataset(spec)),
                   row.get('group_by'))]
        for name, value in (row.get('derivations') or {}).items():
            derivation(value, f"{spec_label}.rows[{index}].derivations.{name}", scopes)
    return errors


def validate_spec_predicates(
    spec, spec_label, spec_path=None, env=None, sources=None
):
    """Parse, resolve, and type-check every R004 predicate in a spec."""
    errors = validate_lookup_filter_scopes(spec, spec_label, env)
    datasets = dataset_type_catalog(spec, spec_path, env, sources)
    output_types = specification_column_types(spec)
    intermediates = {}
    intermediate_entries = spec.get('intermediates')
    if isinstance(intermediate_entries, list):
        for index, intermediate in enumerate(intermediate_entries):
            if not isinstance(intermediate, dict):
                continue
            intermediate_id = intermediate.get('id')
            dataset_id = intermediate.get('dataset')
            # REQ-1185: derived values are available both in the donor filter
            # and through a downstream intermediate-qualified read.
            donor_fields = dict(datasets.get(dataset_id, {}))
            donor_fields.update(
                _intermediate_derived_field_types(
                    intermediate,
                    donor_fields,
                    f"{spec_label}.intermediates[{index}]",
                )
            )
            if isinstance(intermediate_id, str) and isinstance(dataset_id, str):
                intermediates[intermediate_id] = donor_fields
            if isinstance(intermediate.get('filter'), str):
                qualified = (
                    {**datasets, dataset_id: donor_fields}
                    if isinstance(dataset_id, str)
                    else datasets
                )
                resolver = predicate_resolver(qualified=qualified)
                errors.extend(
                    validate_predicate_at(
                        intermediate['filter'],
                        f"{spec_label}.intermediates[{index}].filter",
                        resolver,
                    )
                )

    column_resolver = predicate_resolver(
        unqualified=output_types, qualified={**datasets, **intermediates}
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
            driver = row.get('dataset', default_driver_dataset(spec))
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
            unqualified=output_types, qualified=intermediates
        )
        for index, verification in enumerate(verifications):
            if not isinstance(verification, dict) or len(verification) != 1:
                continue
            keyword, payload = next(iter(verification.items()))
            if not isinstance(payload, dict):
                continue
            fields = (
                ('when', 'then') if keyword == 'implies'
                else ('expr',) if keyword == 'assert'
                else ('filter', 'when') if keyword == 'row_count'
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

    if keyword == 'case' and isinstance(payload, list):
        for index, item in enumerate(payload):
            if not isinstance(item, dict):
                continue
            item_path = f"{path}.case[{index}]"
            if 'otherwise' in item:
                errors.extend(
                    validate_expression_numeric(
                        item['otherwise'],
                        f"{item_path}.otherwise",
                        resolver,
                    )
                )
            else:
                errors.extend(
                    validate_expression_numeric(
                        item.get('then'),
                        f"{item_path}.then",
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
    return errors


def validate_spec_numeric_expressions(
    spec, spec_label, spec_path=None, env=None, sources=None
):
    """Parse, resolve, and type-check every R010 expression in a spec."""
    errors = []
    datasets = dataset_type_catalog(spec, spec_path, env, sources)
    output_types = specification_column_types(spec)
    intermediates = intermediate_type_catalog(spec, datasets, spec_label)

    column_resolver = numeric_identifier_resolver(
        unqualified=output_types, qualified=intermediates
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
            driver = row.get('dataset', default_driver_dataset(spec))
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


def iter_expression_ast(node):
    if not isinstance(node, dict):
        return
    yield node
    for value in node.values():
        if isinstance(value, dict):
            yield from iter_expression_ast(value)
        elif isinstance(value, list):
            for item in value:
                yield from iter_expression_ast(item)


def aggregate_relation_names(ast):
    qualified = set()
    has_unqualified = False
    for node in iter_expression_ast(ast):
        if node.get('kind') == 'identifier':
            name = node['name']
            if '.' in name:
                qualified.add(name.split('.', 1)[0])
            else:
                has_unqualified = True
        elif node.get('kind') == 'qualified_star':
            qualified.add(node['dataset'])
    return qualified, has_unqualified


def first_identifier_with_type(ast, resolver, value_type):
    for node in iter_expression_ast(ast):
        if node.get('kind') != 'identifier':
            continue
        resolved, _ = resolver(node['name'])
        if resolved == value_type:
            return node['name'], node['span']
    return '<expression>', ast['span']


def validate_aggregate_expression_ast(
    ast, path, expression, resolver, grouped, relation
):
    """Validate the R013 key rule, reducer nesting, and static operand types."""
    errors = []

    def validate_grain(node, inside_reduction=False):
        if node.get('kind') == 'identifier':
            if not inside_reduction and node['name'] not in grouped:
                errors.append(
                    validation_diagnostic(
                        path,
                        'aggregate_identifier_not_grouped',
                        f"identifier {node['name']!r} is outside every "
                        'reducer and is not grouped',
                        context={
                            'dataset': relation,
                            'identifier': node['name'],
                        },
                        span=node['span'],
                    )
                )
            return
        nested = inside_reduction or node.get('kind') == 'reduction'
        for value in node.values():
            if isinstance(value, dict):
                validate_grain(value, nested)
            elif isinstance(value, list):
                for item in value:
                    if isinstance(item, dict):
                        validate_grain(item, nested)

    validate_grain(ast)

    def incompatible(node, actual):
        source, span = first_identifier_with_type(node, resolver, actual)
        errors.append(
            validation_diagnostic(
                path,
                'incompatible_input_type',
                f"{source!r} has non-numeric type {actual!r}",
                context={
                    'source': source,
                    'expected': 'numeric',
                    'actual': actual,
                },
                span=span,
            )
        )

    def infer(node, enclosing_reducer=None):
        kind = node['kind']
        if kind == 'number':
            return node['type']
        if kind == 'null':
            return None
        if kind == 'qualified_star':
            return '<star>'
        if kind == 'identifier':
            value_type, issue = resolver(node['name'])
            if issue is not None:
                _, message, context = issue
                errors.append(
                    validation_diagnostic(
                        path,
                        'unknown_field',
                        message,
                        context=context,
                        span=node['span'],
                    )
                )
                return '<invalid>'
            return value_type
        if kind == 'unary':
            value_type = infer(node['value'], enclosing_reducer)
            if value_type not in {None, 'int', 'float', '<invalid>'}:
                incompatible(node['value'], value_type)
                return '<invalid>'
            return value_type
        if kind == 'binary':
            value_types = [
                infer(node['left'], enclosing_reducer),
                infer(node['right'], enclosing_reducer),
            ]
            for child, value_type in zip(
                (node['left'], node['right']), value_types
            ):
                if value_type not in {None, 'int', 'float', '<invalid>'}:
                    incompatible(child, value_type)
                    return '<invalid>'
            if '<invalid>' in value_types:
                return '<invalid>'
            if node['operator'] == '/':
                return 'float'
            return promote_numeric_types(value_types)
        if kind == 'call':
            argument_types = [
                infer(argument, enclosing_reducer)
                for argument in node['arguments']
            ]
            name = node['name'].upper()
            arity = NUMERIC_FUNCTION_ARITIES.get(name)
            if arity is None:
                errors.append(
                    validation_diagnostic(
                        path,
                        'prohibited_function',
                        f"function {node['name']!r} is not permitted by R013",
                        context={
                            'expr': expression,
                            'function': node['name'],
                        },
                        span=node['name_span'],
                    )
                )
                return '<invalid>'
            minimum, maximum = arity
            actual = len(argument_types)
            if actual < minimum or (
                maximum is not None and actual > maximum
            ):
                errors.append(
                    validation_diagnostic(
                        path,
                        'prohibited_function',
                        f"function {node['name']!r} has {actual} arguments",
                        context={
                            'expr': expression,
                            'function': node['name'],
                            'argument_count': actual,
                        },
                        span=node['span'],
                    )
                )
                return '<invalid>'
            for argument, value_type in zip(
                node['arguments'], argument_types
            ):
                if value_type not in {None, 'int', 'float', '<invalid>'}:
                    incompatible(argument, value_type)
                    return '<invalid>'
            if '<invalid>' in argument_types:
                return '<invalid>'
            if name in {'SQRT', 'POWER', 'EXP', 'LN', 'CEIL', 'FLOOR', 'TRUNC'}:
                return 'float'
            return promote_numeric_types(argument_types)
        if kind == 'reduction':
            name = node['name'].upper()
            if enclosing_reducer is not None:
                errors.append(
                    validation_diagnostic(
                        path,
                        'nested_reduction',
                        f"{node['name']} is nested inside {enclosing_reducer}",
                        context={
                            'expr': expression,
                            'outer': enclosing_reducer,
                            'inner': node['name'],
                        },
                        span=node['name_span'],
                    )
                )
            argument = node['argument']
            if argument['kind'] == 'qualified_star':
                if name != 'COUNT':
                    errors.append(
                        validation_diagnostic(
                            path,
                            'invalid_aggregate_expression',
                            'qualified star is valid only with COUNT',
                            context={'expr': expression},
                            span=argument['span'],
                        )
                    )
                    return '<invalid>'
                if relation is not None and argument['dataset'] != relation:
                    errors.append(
                        validation_diagnostic(
                            path,
                            'mixed_relations',
                            'COUNT star names a different relation',
                            context={
                                'relations': sorted(
                                    {relation, argument['dataset']}
                                )
                            },
                            span=argument['span'],
                        )
                    )
                return 'int'
            argument_type = infer(argument, node['name'])
            if name in {'SUM', 'MEAN'} and argument_type not in {
                None, 'int', 'float', '<invalid>'
            }:
                incompatible(argument, argument_type)
                return '<invalid>'
            if name == 'COUNT':
                return 'int'
            if name == 'MEAN':
                return 'float'
            return argument_type
        raise AssertionError(f"unknown aggregate AST node {kind!r}")

    result_type = infer(ast)
    return result_type, list(dict.fromkeys(errors))


def normalize_scalar_list(value):
    return value if isinstance(value, list) else [value]


def validate_aggregate_between(
    between, path, relation, fields, current_resolver
):
    """Validate the row-relative range owned by a qualified aggregate."""
    if not isinstance(between, dict):
        return []
    errors = []
    bounds = [
        (name, between.get(name))
        for name in ('lower', 'upper')
        if between.get(name) is not None
    ]
    if not bounds:
        return [
            validation_diagnostic(
                path,
                'invalid_aggregate_context',
                'aggregate between requires at least one bound',
                context={'reason': 'missing_between_bound'},
            )
        ]

    operand_types = []
    value = between.get('value')
    if isinstance(value, str):
        value_type = current_resolver(value)
        if value_type is None:
            errors.append(
                validation_diagnostic(
                    f"{path}.value",
                    'unknown_field',
                    f"unknown aggregate between value {value!r}",
                    context={'identifier': value},
                )
            )
        else:
            operand_types.append((value, value_type))

    for name, bound in bounds:
        if not isinstance(bound, str) or not bound.startswith(relation + '.'):
            errors.append(
                validation_diagnostic(
                    f"{path}.{name}",
                    'invalid_aggregate_context',
                    f"aggregate {name} must be qualified by {relation!r}",
                    context={'reason': 'wrong_between_bound_relation'},
                )
            )
            continue
        field = bound.split('.', 1)[1]
        bound_type = fields.get(field)
        if bound_type is None:
            if not fields:
                continue
            errors.append(
                validation_diagnostic(
                    f"{path}.{name}",
                    'unknown_field',
                    f"unknown aggregate bound {bound!r}",
                    context={'identifier': bound},
                )
            )
            continue
        operand_types.append((bound, bound_type))

    if operand_types:
        reference, reference_type = operand_types[0]
        for operand, operand_type in operand_types[1:]:
            if runtime_types_comparable(reference_type, operand_type):
                continue
            errors.append(
                incompatible_variable_diagnostic(
                    path,
                    operand,
                    f"a type comparable with {reference!r} "
                    f"({reference_type})",
                    operand_type,
                )
            )
            break
    return errors


# REQ-1189: payload argument names that name record fields for each
# derivation operation, mirroring the schema declarations
# (yaml/schema_expression_*.yaml). A plain string in any other argument
# position is a literal, an enum value, or an identifier -- never a
# variable reference. Window operations additionally read their
# window_spec (see _DERIVE_WINDOW_OPERATIONS below).
_DERIVE_VARIABLE_FIELDS = {
    'baseline_flag': ('date', 'reference_date'),
    'cut': ('source',),
    'date_diff': ('start', 'end'),
    'date_impute': ('source', 'not_before'),
    'date_precision': ('source',),
    'datetime_impute': ('source',),
    'datetime_precision': ('source',),
    'greatest': ('sources',),
    'least': ('sources',),
    'mapping': ('source',),
    'previous_non_missing': ('source',),
    'locf': ('source',),
    'round_half_away_from_zero': ('source',),
    'row_value': ('source',),
    'str_extract': ('source',),
    'str_lower': ('source',),
    'str_upper': ('source',),
    'str_sentence': ('source',),
    'str_title': ('source',),
    'study_day': ('date', 'reference'),
    'to_date': ('source',),
    'to_epoch_day': ('source',),
}

# Window operations name their window_spec's fields in a derive binding
# derivation, alongside the operation's own variable fields above.
_DERIVE_WINDOW_OPERATIONS = (
    'row_number',
    'rank',
    'row_value',
    'previous_non_missing',
    'locf',
    'baseline_flag',
)


def derive_binding_reference_names(derivation):
    """Collect the variable references a derive binding derivation names.

    Only argument positions the schema declares as variable references
    contribute names; literals, enum values, and identifiers never do.
    Nested derivations, expressions, and predicates recurse (REQ-1189).
    """
    names = []

    def add_variable_field(value):
        # A variable field is a bare variable or a filtered source naming
        # one variable with an optional record filter.
        variable = value
        selector = None
        if isinstance(value, dict):
            variable = value.get('variable')
            selector = value.get('filter')
        if isinstance(variable, str):
            names.append(variable)
        names.extend(predicate_identifier_names(selector))

    def add_order_by_names(order_by):
        if isinstance(order_by, list):
            for term in order_by:
                variable = term
                if isinstance(term, dict):
                    variable = term.get('variable')
                if isinstance(variable, str):
                    names.append(variable)

    def add_window_names(window):
        if not isinstance(window, dict):
            return
        group_by = window.get('group_by')
        if isinstance(group_by, list):
            names.extend(entry for entry in group_by if isinstance(entry, str))
        add_order_by_names(window.get('order_by'))
        names.extend(predicate_identifier_names(window.get('filter')))

    def visit(node):
        if isinstance(node, str):
            # A bare-string derivation is one source read.
            names.append(node)
        elif isinstance(node, dict):
            if len(node) == 1:
                operation, payload = next(iter(node.items()))
                if operation == 'source':
                    variable = payload
                    if isinstance(payload, dict):
                        variable = payload.get('variable')
                    if isinstance(variable, str):
                        names.append(variable)
                    return
                if operation == 'compute':
                    expression = payload.get('expr') if isinstance(payload, dict) else None
                    names.extend(numeric_expression_identifier_names(expression))
                    return
                if operation in ('literal', 'aggregate'):
                    # A literal is a fixed value; a nested reduction names
                    # its own relation, checked with the aggregate itself.
                    return
                if operation == 'function' and isinstance(payload, dict):
                    # REQ-0679 writes a named variable as a plain string;
                    # every other argument leaf is a literal class mapping
                    # and depends on nothing.
                    args = payload.get('args')
                    if isinstance(args, dict):
                        names.extend(
                            value for value in args.values() if isinstance(value, str)
                        )
                    return
                if operation == 'str_template':
                    template = payload
                    if isinstance(payload, dict):
                        template = payload.get('template')
                    if isinstance(template, str):
                        names.extend(string_template_identifier_names(template))
                    return
                if operation == 'case' and isinstance(payload, list):
                    for item in payload:
                        if not isinstance(item, dict):
                            continue
                        if 'otherwise' in item:
                            visit(item['otherwise'])
                        else:
                            names.extend(
                                predicate_identifier_names(item.get('when'))
                            )
                            visit(item.get('then'))
                    return
                if operation == 'flag' and isinstance(payload, (dict, str)):
                    # The values are literals and name nothing; only the
                    # predicate names variables. A bare string is the
                    # condition (REQ-1256).
                    condition = (
                        payload
                        if isinstance(payload, str)
                        else payload.get('condition')
                    )
                    names.extend(predicate_identifier_names(condition))
                    return
                if operation == 'lookup' and isinstance(payload, dict):
                    key_base = payload.get('key_base')
                    entries = key_base if isinstance(key_base, list) else [key_base]
                    names.extend(entry for entry in entries if isinstance(entry, str))
                    names.extend(predicate_identifier_names(payload.get('filter')))
                    add_order_by_names(payload.get('order_by'))
                    between = payload.get('between')
                    if isinstance(between, dict):
                        add_variable_field(between.get('value'))
                    return
                if operation == 'first_available' and isinstance(payload, dict):
                    sources = payload.get('sources')
                    if isinstance(sources, list):
                        for entry in sources:
                            add_variable_field(entry)
                    return
                if operation == 'str_concat' and isinstance(payload, dict):
                    sources = payload.get('sources')
                    if isinstance(sources, list):
                        for entry in sources:
                            visit(entry)
                    return
                if operation in ('greatest', 'least') and isinstance(payload, dict):
                    sources = payload.get('sources')
                    if isinstance(sources, list):
                        names.extend(entry for entry in sources if isinstance(entry, str))
                    return
                if operation in _DERIVE_VARIABLE_FIELDS and isinstance(payload, dict):
                    for field in _DERIVE_VARIABLE_FIELDS[operation]:
                        value = payload.get(field)
                        values = value if isinstance(value, list) else [value]
                        for entry in values:
                            add_variable_field(entry)
                    if operation in _DERIVE_WINDOW_OPERATIONS:
                        add_window_names(payload.get('window'))
                    return
                if operation in ('row_number', 'rank') and isinstance(payload, dict):
                    # No variable fields of their own; only the window_spec
                    # names record fields (the rank method is an enum).
                    add_window_names(payload.get('window'))
                    return
            elif set(node) <= {'value', 'missing', 'strict'} and 'value' in node:
                # A handled expression wraps one derivation; missing and
                # strict are literals and name nothing.
                visit(node['value'])
                return
            for value in node.values():
                names.extend(derive_binding_reference_names(value))
        elif isinstance(node, list):
            for item in node:
                names.extend(derive_binding_reference_names(item))
    visit(derivation)
    return names


def validate_derive_step(derive, path, context, filter_text=None):
    '''Validate an aggregate derive step.

    Returns (bindings, relation, errors): bindings maps each bound name to
    its declared type, relation is the single dataset the step names, and
    errors holds any diagnostics. REQ-1189 through REQ-1192.
    '''
    errors = []
    bindings = {}

    def invalid(reason, detail_path, extra=None):
        errors.append(
            validation_diagnostic(
                detail_path,
                'invalid_derive_step',
                f'invalid derive step: {reason}',
                context={'reason': reason, **(extra or {})},
            )
        )

    if not isinstance(derive, list) or not derive:
        invalid('a derive step is a non-empty binding list', f'{path}.derive')
        return bindings, None, errors
    datasets = context.get('input', {})
    qualified_datasets = {
        key: value for key, value in datasets.items() if isinstance(value, dict)
    }
    qualifiers = set()
    for index, binding in enumerate(derive):
        binding_path = f'{path}.derive[{index}]'
        if not isinstance(binding, dict):
            invalid('a derive binding is a mapping', binding_path)
            continue
        name = binding.get('name')
        if not isinstance(name, str) or not name or name in bindings:
            invalid(
                'derive binding names are unique identifiers',
                binding_path,
                {'identifier': name},
            )
            continue
        declared = binding.get('type')
        bindings[name] = declared if isinstance(declared, str) else None
        # REQ-1189: a binding derivation reads the relation's fields and
        # earlier bindings, per record.
        earlier = {
            earlier_name: bindings[earlier_name] for earlier_name in list(bindings)[:-1]
        }
        binding_context = {
            'resolver': predicate_resolver(
                unqualified=earlier, qualified=qualified_datasets
            ),
            'input': datasets,
            'env': context.get('env'),
            'aggregate': context.get('aggregate'),
        }
        errors.extend(
            validate_derivation_static_semantics(
                binding.get('derivation'),
                f'{binding_path}.derivation',
                binding_context,
            )
        )
        for reference in derive_binding_reference_names(binding.get('derivation')):
            head, dot, _ = reference.partition('.')
            if dot and head:
                qualifiers.add(head)
            elif reference not in earlier:
                errors.append(
                    validation_diagnostic(
                        binding_path,
                        'unknown_derive_variable',
                        f'derive binding {name!r} names unknown variable {reference!r}',
                        context={'identifier': reference, 'binding': name},
                    )
                )
    # REQ-1191: the aggregate filter reads the same relation as the
    # bindings, so its qualifiers join the single-relation check.
    if isinstance(filter_text, str):
        for reference in predicate_identifier_names(filter_text):
            head, dot, _ = reference.partition('.')
            if dot and head:
                qualifiers.add(head)
    # REQ-1242: bindings may read keep-declared named intermediates: with
    # `keep`, the intermediate selects exactly one record per row, so it
    # is a row-scoped value, not another reduced relation. Every other
    # qualifier must be the step's one driving relation.
    intermediate_ids = context.get('intermediate_ids', frozenset())
    keep_intermediate_ids = context.get('keep_intermediate_ids', frozenset())
    relation_qualifiers = {
        qualifier for qualifier in qualifiers if qualifier not in intermediate_ids
    }
    relation = next(iter(relation_qualifiers)) if len(relation_qualifiers) == 1 else None
    if relation is None:
        invalid(
            'a derive step names exactly one relation',
            f'{path}.derive',
            {'relations': sorted(relation_qualifiers)},
        )
        return bindings, relation, errors
    for qualifier in sorted(qualifiers - relation_qualifiers):
        if qualifier not in keep_intermediate_ids:
            invalid(
                'a derive step reads only keep-declared named intermediates',
                f'{path}.derive',
                {'intermediate': qualifier},
            )
    return bindings, relation, errors


def validate_aggregate_at(payload, path, context):
    if isinstance(payload, str):
        expression = payload
        group_by = None
        between = None
        expression_path = path
    elif isinstance(payload, dict):
        expression = payload.get('expr')
        group_by = payload.get('group_by')
        between = payload.get('between')
        expression_path = (
            path if set(payload) == {'expr'} else f"{path}.expr"
        )
    else:
        return []
    if not isinstance(expression, str):
        return []
    try:
        ast = parse_aggregate_expression(expression)
    except AggregateExpressionError as exc:
        return [
            validation_diagnostic(
                expression_path,
                exc.condition,
                exc.message,
                context={'expr': expression, **exc.context},
                span=exc.span,
            )
        ]

    errors = []
    derive_bindings = {}
    derive_relation = None
    if isinstance(payload, dict) and payload.get('derive') is not None:
        filter_text = payload.get('filter')
        derive_bindings, derive_relation, derive_errors = validate_derive_step(
            payload['derive'],
            path,
            context,
            filter_text if isinstance(filter_text, str) else None,
        )
        errors.extend(derive_errors)
        if derive_errors:
            return list(dict.fromkeys(errors))
        # REQ-1189: unqualified names in the reduction name derive
        # bindings; the step names the reduced relation.
        unqualified_names = {
            node['name']
            for node in iter_expression_ast(ast)
            if node.get('kind') == 'identifier' and '.' not in node['name']
        }
        unknown = sorted(unqualified_names - set(derive_bindings))
        if unknown:
            return [
                validation_diagnostic(
                    expression_path,
                    'unknown_derive_variable',
                    'aggregate expression names unknown derive '
                    f"variable(s): {', '.join(unknown)}",
                    context={
                        'identifier': unknown[0],
                        'variables': unknown,
                        'expr': expression,
                    },
                )
            ]
        # The planner rejects a qualified reference alongside unqualified
        # names before the derive step is examined (REQ-0504).
        expr_qualifiers, _ = aggregate_relation_names(ast)
        if len(expr_qualifiers) > 1 or (expr_qualifiers and unqualified_names):
            relation_names = sorted(expr_qualifiers) + ['<output>']
            return [
                validation_diagnostic(
                    expression_path,
                    'mixed_relations',
                    f'aggregate expression mixes relations {relation_names!r}',
                    context={'relations': relation_names},
                    span=ast['span'],
                )
            ]
        qualifiers = {derive_relation}
        has_unqualified = False
        relations = sorted(qualifiers)
    else:
        qualifiers, has_unqualified = aggregate_relation_names(ast)
        relations = sorted(qualifiers)
    if len(qualifiers) > 1 or (qualifiers and has_unqualified):
        relation_names = relations + (['<output>'] if has_unqualified else [])
        return [
            validation_diagnostic(
                expression_path,
                'mixed_relations',
                f"aggregate expression mixes relations {relation_names!r}",
                context={'relations': relation_names},
                span=ast['span'],
            )
        ]

    kind = context['kind']
    relation = relations[0] if relations else None
    datasets = context['input']
    output_types = context['output_types']
    if kind == 'ungrouped_row':
        return [
            validation_diagnostic(
                path,
                'invalid_aggregate_context',
                'aggregate is not valid in an ungrouped row template',
                context={'reason': 'ungrouped_row'},
            )
        ]
    if kind == 'grouped_row':
        driver = context.get('driver')
        if relation != driver or has_unqualified:
            errors.append(
                validation_diagnostic(
                    path,
                    'invalid_aggregate_context',
                    'grouped-row aggregate must name only its row driver',
                    context={'reason': 'wrong_grouped_row_relation'},
                )
            )
        if group_by is not None:
            errors.append(
                validation_diagnostic(
                    path,
                    'invalid_aggregate_context',
                    'grouped-row aggregate must omit local group_by',
                    context={'reason': 'grouped_row_local_group_by'},
                )
            )
        # REQ-0142: the group is the match, so a key pair could only be
        # ignored; it never widens the read to the scope it names.
        for field in ('key', 'key_base'):
            if isinstance(payload, dict) and payload.get(field) is not None:
                errors.append(
                    validation_diagnostic(
                        f"{path}.{field}",
                        'invalid_aggregate_context',
                        'grouped-row aggregate reads its own group and '
                        'declares no key pairs',
                        context={'reason': 'grouped_row_key_pairs'},
                    )
                )
        grouped = set(context.get('row_group_by') or [])
        fields = datasets.get(driver, {})
        resolver = numeric_identifier_resolver(
            qualified={driver: fields} if isinstance(driver, str) else {}
        )
    elif relation is not None:
        if relation not in datasets:
            errors.append(
                validation_diagnostic(
                    expression_path,
                    'unknown_field',
                    f"unknown aggregate relation {relation!r}",
                    context={'identifier': relation},
                    span=ast['span'],
                )
            )
        grouped = set(group_by or []) if isinstance(group_by, list) else set()
        # REQ-1190: the reduction names derive bindings unqualified with
        # their declared types; the relation's fields stay qualified.
        resolver = numeric_identifier_resolver(
            unqualified=derive_bindings,
            qualified={relation: datasets.get(relation, {})}
        )
        for name in grouped:
            if not isinstance(name, str) or not name.startswith(relation + '.'):
                errors.append(
                    validation_diagnostic(
                        f"{path}.group_by",
                        'invalid_aggregate_context',
                        'qualified aggregate group_by must use its relation',
                        context={'reason': 'wrong_group_by_relation'},
                    )
                )
                break
            field = name.split('.', 1)[1]
            if datasets.get(relation) and field not in datasets[relation]:
                errors.append(
                    validation_diagnostic(
                        f"{path}.group_by",
                        'unknown_field',
                        f"unknown aggregate group field {name!r}",
                        context={'identifier': name},
                    )
                )
                break
            if field not in context.get('keys', []):
                errors.append(
                    validation_diagnostic(
                        f"{path}.group_by",
                        'invalid_aggregate_context',
                        f"{name!r} is not an output key",
                        context={'reason': 'group_by_not_output_key'},
                    )
                )
                break
    else:
        grouped = set(group_by or []) if isinstance(group_by, list) else set()
        resolver = numeric_identifier_resolver(unqualified=output_types)
        if not grouped:
            errors.append(
                validation_diagnostic(
                    path,
                    'invalid_aggregate_context',
                    'unqualified aggregate requires non-empty group_by',
                    context={'reason': 'missing_output_group_by'},
                )
            )
        for name in grouped:
            if not isinstance(name, str) or '.' in name:
                errors.append(
                    validation_diagnostic(
                        f"{path}.group_by",
                        'invalid_aggregate_context',
                        'unqualified aggregate group_by must use output '
                        'columns',
                        context={'reason': 'qualified_output_group_by'},
                    )
                )
                break
            if name not in output_types:
                errors.append(
                    validation_diagnostic(
                        f"{path}.group_by",
                        'unknown_field',
                        f"unknown aggregate group field {name!r}",
                        context={'identifier': name},
                    )
                )
                break

    if between is not None:
        if kind != 'column' or relation is None or has_unqualified:
            errors.append(
                validation_diagnostic(
                    f"{path}.between",
                    'invalid_aggregate_context',
                    'between requires a qualified column aggregate',
                    context={'reason': 'between_without_right_relation'},
                )
            )
        else:
            current_resolver = context.get('resolver')
            if current_resolver is None:
                current_resolver = predicate_resolver(
                    unqualified=output_types
                )
            errors.extend(
                validate_aggregate_between(
                    between,
                    f"{path}.between",
                    relation,
                    datasets.get(relation, {}),
                    current_resolver,
                )
            )

    _, expression_errors = validate_aggregate_expression_ast(
        ast, expression_path, expression, resolver, grouped, relation
    )
    errors.extend(expression_errors)
    return list(dict.fromkeys(errors))


def validate_string_template_at(text, path, resolver):
    try:
        placeholders = string_template_placeholders(text)
    except StringTemplateError as exc:
        return [
            validation_diagnostic(
                path,
                'invalid_string_template',
                exc.message,
                context={
                    'reason': exc.reason,
                    'placeholder': exc.placeholder or '',
                },
                span=exc.span,
            )
        ]
    errors = []
    for placeholder in placeholders:
        name = placeholder['name']
        value_type = resolver(name)
        if value_type is None:
            errors.append(
                validation_diagnostic(
                    path,
                    'unknown_field',
                    f"unknown template placeholder {name!r}",
                    context={'identifier': name},
                    span=placeholder['span'],
                )
            )
        elif value_type is not DERIVED_TYPE_UNKNOWN and value_type != 'str':
            errors.append(
                validation_diagnostic(
                    path,
                    'incompatible_input_type',
                    f"template placeholder {name!r} has type {value_type!r}",
                    context={
                        'source': name,
                        'expected': 'str',
                        'actual': value_type,
                    },
                    span=placeholder['span'],
                )
            )
    return errors


def runtime_types_comparable(left, right):
    if left is None or right is None:
        return True
    if left is DERIVED_TYPE_UNKNOWN or right is DERIVED_TYPE_UNKNOWN:
        return True
    if left in {'int', 'float'} and right in {'int', 'float'}:
        return True
    return left == right


def ascii_case_fold(value):
    return ''.join(
        chr(ord(character) - 32)
        if 'a' <= character <= 'z'
        else character
        for character in value
    )


def incompatible_variable_diagnostic(path, source, expected, actual):
    return validation_diagnostic(
        path,
        'incompatible_input_type',
        f"{source!r} has type {actual!r}; expected {expected}",
        context={
            'source': source,
            'expected': expected,
            'actual': actual,
        },
    )


def validate_named_input_type(
    payload, field, accepted, expected, path, resolver
):
    if not isinstance(payload, dict):
        return []
    source = payload.get(field)
    if not isinstance(source, str):
        return []
    actual = resolver(source)
    if actual is None or actual is DERIVED_TYPE_UNKNOWN or actual in accepted:
        return []
    return [
        incompatible_variable_diagnostic(
            f"{path}.{field}", source, expected, actual
        )
    ]


def unresolved_variable_diagnostic(name, path, resolver):
    if resolver(name) is not None:
        return None
    if '.' in name:
        qualifier, _ = name.split('.', 1)
        relations = getattr(resolver, 'qualified', {})
        # If a declared relation could not be inspected (for example because
        # R021 already rejected its path), do not invent a second failure.
        if qualifier in relations and not relations[qualifier]:
            return None
    return validation_diagnostic(
        path,
        'unknown_field',
        f"unknown variable {name!r}",
        context={'identifier': name},
    )


def validate_expression_reference_bindings(expression, path, context):
    """Resolve variable leaves not owned by a language-specific parser."""
    if not isinstance(expression, dict) or len(expression) != 1:
        return []
    keyword, payload = next(iter(expression.items()))
    if keyword in {'aggregate', 'compute', 'str_template'}:
        return []
    if keyword == 'case' and isinstance(payload, list):
        errors = []
        for index, item in enumerate(payload):
            if not isinstance(item, dict):
                continue
            item_path = f"{path}.case[{index}]"
            if 'otherwise' in item:
                errors.extend(
                    validate_expression_reference_bindings(
                        item['otherwise'], f"{item_path}.otherwise", context
                    )
                )
            else:
                errors.extend(
                    validate_expression_reference_bindings(
                        item.get('then'),
                        f"{item_path}.then",
                        context,
                    )
                )
        return errors
    if keyword == 'flag' and isinstance(payload, (dict, str)):
        # The condition, bare or under condition, is a predicate owned by
        # the predicate parser; the values are literals and name nothing
        # (REQ-1256).
        return []
    if keyword == 'str_concat' and isinstance(payload, dict):
        errors = []
        sources = payload.get('sources')
        if isinstance(sources, list):
            for index, source in enumerate(sources):
                errors.extend(
                    validate_expression_reference_bindings(
                        source,
                        f"{path}.str_concat.sources[{index}]",
                        context,
                    )
                )
        return errors

    env = context.get('env')
    if not isinstance(env, dict):
        return []
    references = collect_type_references(expression, 'expression', env)
    errors = []
    for kind, name in sorted(references):
        if kind != 'variable':
            continue
        diagnostic = unresolved_variable_diagnostic(
            name, f"{path}.{keyword}", context['resolver']
        )
        if diagnostic is not None:
            errors.append(diagnostic)
    return errors


def validate_expression_static_semantics(expression, path, context):
    errors = validate_expression_reference_bindings(
        expression, path, context
    )
    if not isinstance(expression, dict) or len(expression) != 1:
        return errors
    keyword, payload = next(iter(expression.items()))
    resolver = context['resolver']

    window_order_required = {
        'row_number', 'rank', 'row_value', 'previous_non_missing', 'locf',
    }
    window_order_forbidden = {'baseline_flag'}
    if (
        keyword in window_order_required | window_order_forbidden
        and isinstance(payload, dict)
    ):
        window = payload.get('window')
        order_by = window.get('order_by') if isinstance(window, dict) else None
        operation_path = f"{path}.{keyword}"
        if keyword in window_order_required and not order_by:
            # REQ-0339: without a declared order the window has no positions
            # to number or to move along.
            errors.append(
                validation_diagnostic(
                    f"{operation_path}.window",
                    'window_order_by_required',
                    f'{keyword} requires window.order_by',
                    context={'operation': keyword},
                )
            )
        elif keyword in window_order_forbidden and order_by:
            # REQ-0340: the baseline row is located by date and flag, not by
            # a declared order, so a declared order would be silently ignored.
            errors.append(
                validation_diagnostic(
                    f"{operation_path}.window.order_by",
                    'window_order_by_forbidden',
                    f'{keyword} does not take window.order_by',
                    context={'operation': keyword},
                )
            )

    if keyword == 'aggregate':
        return validate_aggregate_at(
            payload, f"{path}.aggregate", context['aggregate']
        )

    if keyword == 'str_template':
        template = (
            payload.get('template') if isinstance(payload, dict) else payload
        )
        if isinstance(template, str):
            errors.extend(
                validate_string_template_at(
                    template, f"{path}.str_template", resolver
                )
            )
        return errors

    if keyword == 'mapping' and isinstance(payload, dict):
        errors.extend(
            validate_named_input_type(
                payload,
                'source',
                {'str'},
                'str',
                f"{path}.mapping",
                resolver,
            )
        )
        dictionary = payload.get('dict')
        if payload.get('case_sensitive', True) is False and isinstance(
            dictionary, dict
        ):
            folded = {}
            for key in dictionary:
                if not isinstance(key, str):
                    continue
                folded.setdefault(ascii_case_fold(key), []).append(key)
            for folded_key, entries in folded.items():
                if len(entries) < 2:
                    continue
                errors.append(
                    validation_diagnostic(
                        f"{path}.mapping.dict",
                        'ambiguous_dictionary',
                        f"dictionary entries {entries!r} fold to "
                        f"{folded_key!r}",
                        context={
                            'folded_key': folded_key,
                            'entries': entries,
                        },
                    )
                )
        return errors

    if keyword == 'lookup' and isinstance(payload, dict):
        sources = normalize_scalar_list(payload.get('key_base'))
        keys = normalize_scalar_list(payload.get('key'))
        operation_path = f"{path}.lookup"
        # REQ-0155: only flag if BOTH were explicitly written (not inferred).
        # If either was omitted, the inference/defaulting is not redundant.
        if (
            payload.get('key_base') is not None
            and payload.get('key') is not None
            and sources == keys
        ):
            # REQ-0155: key_base must not repeat the key names.
            return [
                validation_diagnostic(
                    operation_path,
                    'redundant_key_base',
                    'key_base repeats the key names; omit it',
                    context={
                        'key_base': sources,
                        'key': keys,
                    },
                )
            ]
        if len(sources) != len(keys):
            return [
                validation_diagnostic(
                    operation_path,
                    'source_key_length_mismatch',
                    f"key_base has {len(sources)} value(s), key has "
                    f"{len(keys)}",
                    context={
                        'key_base': sources,
                        'key': keys,
                        'key_base_count': len(sources),
                        'key_count': len(keys),
                    },
                )
            ]
        dataset = payload.get('dataset')
        fields = context['input'].get(dataset, {})
        if not fields:
            return errors
        for source, key in zip(sources, keys):
            if not isinstance(source, str) or not isinstance(key, str):
                continue
            source_type = resolver(source)
            key_type = fields.get(key)
            if key_type is None:
                errors.append(
                    validation_diagnostic(
                        f"{operation_path}.key",
                        'unknown_field',
                        f"unknown mapping key {key!r}",
                        context={'identifier': key},
                    )
                )
                continue
            if (
                source_type is not None
                and not runtime_types_comparable(source_type, key_type)
            ):
                errors.append(
                    incompatible_variable_diagnostic(
                        operation_path,
                        source,
                        f"a type comparable with {key!r} ({key_type})",
                        source_type,
                    )
                )
        value = payload.get('value')
        if isinstance(value, str) and value not in fields:
            errors.append(
                validation_diagnostic(
                    f"{operation_path}.value",
                    'unknown_field',
                    f"unknown mapping value field {value!r}",
                    context={'identifier': value},
                )
            )
        return errors

    if keyword in {'greatest', 'least'} and isinstance(payload, dict):
        sources = payload.get('sources')
        if not isinstance(sources, list):
            return errors
        types = [resolver(source) for source in sources]
        concrete = [value_type for value_type in types if value_type is not None]
        if concrete and any(
            not runtime_types_comparable(concrete[0], value_type)
            for value_type in concrete[1:]
        ):
            errors.append(
                validation_diagnostic(
                    f"{path}.{keyword}",
                    'incomparable_sources',
                    f"sources {sources!r} have incomparable types {types!r}",
                    context={'sources': sources, 'types': types},
                )
            )
        return errors

    if keyword == 'row_value' and isinstance(payload, dict):
        if payload.get('offset') == 0:
            errors.append(
                validation_diagnostic(
                    f"{path}.row_value.offset",
                    'zero_offset',
                    'row_value offset must be nonzero',
                    context={'offset': 0},
                )
            )
        return errors

    if keyword == 'cut' and isinstance(payload, dict):
        breaks = payload.get('breaks')
        labels = payload.get('labels')
        operation_path = f"{path}.cut"
        if isinstance(breaks, list) and isinstance(labels, list) and (
            len(labels) != len(breaks) + 1
        ):
            errors.append(
                validation_diagnostic(
                    operation_path,
                    'invalid_cut',
                    'cut requires exactly one more label than break',
                    context={'reason': 'label_count'},
                )
            )
        if isinstance(breaks, list) and any(
            left >= right for left, right in zip(breaks, breaks[1:])
            if type(left) in {int, float} and type(right) in {int, float}
        ):
            errors.append(
                validation_diagnostic(
                    operation_path,
                    'invalid_cut',
                    'cut breaks must be strictly ascending',
                    context={'reason': 'break_order'},
                )
            )
        errors.extend(
            validate_named_input_type(
                payload,
                'source',
                {'int', 'float'},
                'numeric',
                operation_path,
                resolver,
            )
        )
        return errors

    if keyword == 'round_half_away_from_zero' and isinstance(payload, dict):
        errors.extend(
            validate_named_input_type(
                payload,
                'source',
                {'int', 'float'},
                'numeric',
                f"{path}.round_half_away_from_zero",
                resolver,
            )
        )
        return errors

    # (to_number validation removed: REQ-1186 withdrawn per #715 direction)

    temporal_inputs = {
        'date_diff': {
            'start': ({'date'}, 'date'),
            'end': ({'date'}, 'date'),
        },
        'study_day': {
            'date': ({'date'}, 'date'),
            'reference': ({'date'}, 'date'),
        },
        'date_precision': {
            'source': ({'str', 'date'}, 'str or date'),
        },
        'datetime_impute': {
            'source': ({'str'}, 'str'),
        },
        'datetime_precision': {
            'source': ({'str', 'datetime'}, 'str or datetime'),
        },
        'to_date': {
            'source': ({'datetime', 'str'}, 'datetime or ISO date text'),
        },
        'to_epoch_day': {
            'source': ({'date'}, 'date'),
        },
    }
    if keyword in temporal_inputs and isinstance(payload, dict):
        operation_path = f"{path}.{keyword}"
        for field, (accepted, expected) in temporal_inputs[keyword].items():
            errors.extend(
                validate_named_input_type(
                    payload,
                    field,
                    accepted,
                    expected,
                    operation_path,
                    resolver,
                )
            )
        if keyword == 'date_diff':
            unit = payload.get('unit')
            bounds = payload.get('bounds', 'exclusive')
            if unit in ('week', 'month', 'year') and bounds != 'exclusive':
                errors.append(
                    validation_diagnostic(
                        f"{operation_path}.bounds",
                        'value_not_permitted',
                        f"bounds {bounds!r} is defined only with unit 'day'",
                        context={
                            'value': bounds,
                            'permitted': ['exclusive'],
                        },
                    )
                )
        return errors

    if keyword == 'date_impute' and isinstance(payload, dict):
        operation_path = f"{path}.date_impute"
        minimum = payload.get('minimum_source_precision', 'year')
        month = payload.get('month')
        day = payload.get('day')
        if minimum == 'month' and 'month' in payload:
            # REQ-0592: no specification carries a value the policy leaves
            # unreachable; the value itself is never read.
            errors.append(
                validation_diagnostic(
                    f"{operation_path}.month",
                    'month_not_permitted',
                    f"month {month!r} is unreachable with "
                    "minimum_source_precision 'month'",
                    context={
                        'month': month if type(month) is int else str(month)
                    },
                )
            )
        elif minimum == 'year' and 'month' not in payload:
            # REQ-0592: the month is required where the policy can use it.
            errors.append(
                validation_diagnostic(
                    f"{operation_path}.month",
                    'month_required',
                    "month is required with minimum_source_precision 'year'",
                    context={'minimum_source_precision': minimum},
                )
            )
        elif type(month) is int and not 1 <= month <= 12:
            errors.append(
                validation_diagnostic(
                    f"{operation_path}.month",
                    'month_out_of_range',
                    f"month {month} is outside 1 through 12",
                    context={'month': month},
                )
            )
        if type(day) is int and not 1 <= day <= 31:
            errors.append(
                validation_diagnostic(
                    f"{operation_path}.day",
                    'day_out_of_range',
                    f"day {day} is outside 1 through 31",
                    context={'day': day},
                )
            )
        maximum_days = {2: 29, 4: 30, 6: 30, 9: 30, 11: 30}
        if (
            type(month) is int
            and 1 <= month <= 12
            and type(day) is int
            and 1 <= day <= 31
            and day > maximum_days.get(month, 31)
        ):
            errors.append(
                validation_diagnostic(
                    f"{operation_path}.day",
                    'day_out_of_range',
                    f"day {day} cannot occur in month {month}",
                    context={'day': day},
                )
            )
        errors.extend(
            validate_named_input_type(
                payload,
                'source',
                {'str'},
                'str',
                operation_path,
                resolver,
            )
        )
        errors.extend(
            validate_named_input_type(
                payload,
                'not_before',
                {'date'},
                'date',
                operation_path,
                resolver,
            )
        )
        return errors

    if keyword in {'str_extract', 'str_upper', 'str_lower', 'str_sentence', 'str_title'}:
        errors.extend(
            validate_named_input_type(
                payload,
                'source',
                {'str'},
                'str',
                f"{path}.{keyword}",
                resolver,
            )
        )
        return errors

    if keyword == 'case' and isinstance(payload, list):
        for index, item in enumerate(payload):
            if not isinstance(item, dict):
                continue
            item_path = f"{path}.case[{index}]"
            if 'otherwise' in item:
                errors.extend(
                    validate_expression_static_semantics(
                        item['otherwise'], f"{item_path}.otherwise", context
                    )
                )
            else:
                errors.extend(
                    validate_expression_static_semantics(
                        item.get('then'),
                        f"{item_path}.then",
                        context,
                    )
                )
    elif keyword == 'str_concat' and isinstance(payload, dict):
        sources = payload.get('sources')
        if isinstance(sources, list):
            for index, source in enumerate(sources):
                errors.extend(
                    validate_expression_static_semantics(
                        source,
                        f"{path}.str_concat.sources[{index}]",
                        context,
                    )
                )
    return errors


def validate_derivation_static_semantics(derivation, path, context):
    if not isinstance(derivation, dict):
        return []
    if 'value' not in derivation:
        return validate_expression_static_semantics(
            derivation, path, context
        )
    errors = validate_expression_static_semantics(
        derivation.get('value'), f"{path}.value", context
    )
    return errors


ROW_WINDOW_OPERATIONS = (
    'row_number',
    'rank',
    'row_value',
    'previous_non_missing',
    'locf',
    'baseline_flag',
)


def validate_row_window_dependencies(derivations, index, spec_label, env):
    """Reject a row-construction window that reads another window's result.

    REQ-0326 evaluates a template's windows in one pass over its
    constructed rows with no declared order between the window
    expressions, so a window must not depend on another window's result,
    directly or through a scalar derived from one. A window reading its
    own column is a cycle, left to the dependency-cycle check.
    """
    errors = []
    window_columns = {}
    for name, derivation in derivations.items():
        if isinstance(derivation, dict) and len(derivation) == 1:
            operation = next(iter(derivation))
            if operation in ROW_WINDOW_OPERATIONS:
                window_columns[name] = operation
    if not window_columns:
        return errors
    dependencies = {}
    for name, derivation in derivations.items():
        references = collect_type_references(derivation, 'expression', env)
        dependencies[name] = {
            ref_name
            for kind, ref_name in references
            if kind == 'variable'
            and '.' not in ref_name
            and ref_name in derivations
        }
    deferred = set(window_columns)
    changed = True
    while changed:
        changed = False
        for name, deps in dependencies.items():
            if name not in deferred and any(dep in deferred for dep in deps):
                deferred.add(name)
                changed = True
    for name in sorted(window_columns):
        blocked = sorted(
            dep
            for dep in dependencies[name]
            if dep in deferred and dep != name
        )
        if not blocked:
            continue
        errors.append(
            validation_diagnostic(
                f"{spec_label}.rows[{index}].derivations.{name}."
                f"{window_columns[name]}",
                'window_on_window_result',
                f"window {name!r} depends on window-derived {blocked[0]!r}",
                context={'column': name, 'depends_on': blocked},
            )
        )
    return errors


def validate_intermediate_static_semantics(
    spec, spec_label, datasets, output_types
):
    errors = []
    intermediates = spec.get('intermediates')
    if not isinstance(intermediates, list):
        return errors
    output_resolver = predicate_resolver(
        unqualified=output_types, qualified=datasets
    )
    for index, intermediate in enumerate(intermediates):
        if not isinstance(intermediate, dict):
            continue
        operation_path = f"{spec_label}.intermediates[{index}]"
        dataset = intermediate.get('dataset')
        if (
            intermediate.get('key') is None
            and intermediate.get('key_base') is None
            and intermediate.get('between') is None
            and intermediate.get('filter') is None
            and intermediate.get('order_by') is None
            and intermediate.get('keep') is None
            and intermediate.get('columns') is None
            and intermediate.get('derivations') is None
            and intermediate.get('verification') is None
            and intermediate.get('missing') is None
            and not intermediate.get('strict', False)
        ):
            errors.append(
                validation_diagnostic(
                    operation_path,
                    'rename_only_intermediate',
                    'named intermediate only renames its dataset',
                    context={
                        'intermediate': intermediate.get('id'),
                        'dataset': dataset,
                    },
                )
            )
        fields = datasets.get(dataset, {})
        sources = intermediate.get('source')
        keys = intermediate.get('key')
        if sources is not None and keys is not None:
            source_list = normalize_scalar_list(sources)
            key_list = normalize_scalar_list(keys)
            if len(source_list) != len(key_list):
                errors.append(
                    validation_diagnostic(
                        operation_path,
                        'source_key_length_mismatch',
                        'record intermediate source and key lengths differ',
                        context={
                            'source': source_list,
                            'key': key_list,
                            'source_count': len(source_list),
                            'key_count': len(key_list),
                        },
                    )
                )
            elif fields:
                for source, key in zip(source_list, key_list):
                    if not isinstance(source, str) or not isinstance(key, str):
                        continue
                    unresolved = unresolved_variable_diagnostic(
                        source, operation_path, output_resolver
                    )
                    if unresolved is not None:
                        errors.append(unresolved)
                    if key not in fields:
                        errors.append(
                            validation_diagnostic(
                                f"{operation_path}.key",
                                'unknown_field',
                                f"unknown record intermediate key {key!r}",
                                context={'identifier': key},
                            )
                        )
                        continue
                    source_type = output_resolver(source)
                    key_type = fields.get(key)
                    if (
                        source_type is None
                        or key_type is None
                        or runtime_types_comparable(source_type, key_type)
                    ):
                        continue
                    errors.append(
                        incompatible_variable_diagnostic(
                            operation_path,
                            source,
                            f"a type comparable with {key!r} ({key_type})",
                            source_type,
                        )
                    )
        between = intermediate.get('between')
        if not isinstance(between, dict) or not fields:
            continue
        value = between.get('value')
        lower = between.get('lower')
        upper = between.get('upper')
        value_type = output_resolver(value) if isinstance(value, str) else None
        lower_type = fields.get(lower) if isinstance(lower, str) else None
        upper_type = fields.get(upper) if isinstance(upper, str) else None
        if isinstance(value, str):
            unresolved = unresolved_variable_diagnostic(
                value, f"{operation_path}.between.value", output_resolver
            )
            if unresolved is not None:
                errors.append(unresolved)
        for name, bound in (('lower', lower), ('upper', upper)):
            if isinstance(bound, str) and bound not in fields:
                errors.append(
                    validation_diagnostic(
                        f"{operation_path}.between.{name}",
                        'unknown_field',
                        f"unknown record intermediate bound {bound!r}",
                        context={'identifier': bound},
                    )
                )
        concrete = [
            value_type,
            *(
                [lower_type] if lower is not None else []
            ),
            *(
                [upper_type] if upper is not None else []
            ),
        ]
        known = [item_type for item_type in concrete if item_type is not None]
        if known and any(
            not runtime_types_comparable(known[0], item_type)
            for item_type in known[1:]
        ):
            errors.append(
                validation_diagnostic(
                    f"{operation_path}.between",
                    'incomparable_range_types',
                    'record intermediate range operands are not comparable',
                    context={
                        'intermediate': intermediate.get('id'),
                        'value_type': value_type,
                        'lower_type': lower_type,
                        'upper_type': upper_type,
                    },
                )
            )
    return errors


def default_driver_dataset(spec):
    base = spec.get('base')
    if isinstance(base, str):
        return base
    datasets = spec.get('input')
    if isinstance(datasets, dict):
        names = [name for name in datasets if isinstance(name, str)]
        if len(names) == 1:
            return names[0]
    return None


def column_dependency_graph(spec, env):
    columns = spec.get('columns')
    if not isinstance(columns, list):
        return [], {}
    names = [
        column.get('name')
        for column in columns
        if isinstance(column, dict) and isinstance(column.get('name'), str)
    ]
    rows = spec.get('rows') if isinstance(spec.get('rows'), list) else []
    intermediate_entries = (
        spec.get('intermediates')
        if isinstance(spec.get('intermediates'), list)
        else []
    )
    intermediates = {
        intermediate.get('id'): intermediate
        for intermediate in intermediate_entries
        if isinstance(intermediate, dict) and isinstance(intermediate.get('id'), str)
    }
    by_name = {
        column.get('name'): column
        for column in columns
        if isinstance(column, dict) and isinstance(column.get('name'), str)
    }
    dependencies = {
        name: {
            dependency
            for dependency in column_dependency_names(
                by_name[name], rows, intermediates, env
            )
            if dependency in by_name
        }
        for name in names
    }
    return names, dependencies


def dependency_components(names, dependencies):
    index_of = {}
    lowlink = {}
    stack = []
    on_stack = set()
    counter = [0]
    component_of = {}

    def connect(name):
        index_of[name] = lowlink[name] = counter[0]
        counter[0] += 1
        stack.append(name)
        on_stack.add(name)
        for dependency in sorted(dependencies[name]):
            if dependency not in index_of:
                connect(dependency)
                lowlink[name] = min(lowlink[name], lowlink[dependency])
            elif dependency in on_stack:
                lowlink[name] = min(lowlink[name], index_of[dependency])
        if lowlink[name] == index_of[name]:
            while True:
                member = stack.pop()
                on_stack.discard(member)
                component_of[member] = name
                if member == name:
                    break

    for name in names:
        if name not in index_of:
            connect(name)
    return component_of


def find_forward_reference(spec, env):
    names, dependencies = column_dependency_graph(spec, env)
    positions = {name: index for index, name in enumerate(names)}
    component_of = dependency_components(names, dependencies)
    for name in names:
        for dependency in sorted(dependencies[name]):
            if positions[dependency] > positions[name]:
                if component_of.get(name) == component_of.get(dependency):
                    continue
                return name, dependency
    return None


def find_key_dependency(spec, env):
    rows = spec.get('rows')
    if isinstance(rows, list) and rows:
        return None
    keys = spec.get('keys')
    if not isinstance(keys, list):
        return None
    key_names = {key for key in keys if isinstance(key, str)}
    names, dependencies = column_dependency_graph(spec, env)
    for name in names:
        if name in key_names:
            for dependency in sorted(dependencies[name]):
                if dependency not in key_names:
                    return name, dependency
    return None


def find_column_dependency_cycle(spec, env):
    names, dependencies = column_dependency_graph(spec, env)
    state = {}
    stack = []

    def visit(name):
        state[name] = 'active'
        stack.append(name)
        for dependency in sorted(dependencies[name]):
            if state.get(dependency) == 'active':
                start = stack.index(dependency)
                return stack[start:] + [dependency]
            if state.get(dependency) is None:
                cycle = visit(dependency)
                if cycle is not None:
                    return cycle
        stack.pop()
        state[name] = 'complete'
        return None

    for name in reversed(names):
        if state.get(name) is None:
            cycle = visit(name)
            if cycle is not None:
                return cycle
    return None


def derivation_primary_path(spec, spec_label, name):
    columns = spec.get('columns')
    if isinstance(columns, list):
        for column in columns:
            if not isinstance(column, dict) or column.get('name') != name:
                continue
            derivation = column.get('derivation')
            path = f"{spec_label}.columns.{name}.derivation"
            if isinstance(derivation, str):
                # REQ-0319: a bare string desugars to {source: string} before
                # any path is computed, so diagnostics name the source.
                derivation = {'source': derivation}
            if isinstance(derivation, dict) and 'value' in derivation:
                derivation = derivation.get('value')
                path += '.value'
            if isinstance(derivation, dict) and len(derivation) == 1:
                path += '.' + next(iter(derivation))
            return path
    rows = spec.get('rows')
    if isinstance(rows, list):
        for index, row in enumerate(rows):
            derivations = row.get('derivations') if isinstance(row, dict) else None
            if not isinstance(derivations, dict) or name not in derivations:
                continue
            derivation = derivations[name]
            path = f"{spec_label}.rows[{index}].derivations.{name}"
            if isinstance(derivation, str):
                derivation = {'source': derivation}
            if isinstance(derivation, dict) and len(derivation) == 1:
                path += '.' + next(iter(derivation))
            return path
    return f"{spec_label}.columns.{name}.derivation"


def validate_spec_static_semantics(spec, spec_label, spec_path, env, sources=None):
    """Validate static operation, aggregate, template, and graph contracts."""
    errors = []
    datasets = dataset_type_catalog(spec, spec_path, env, sources)
    output_types = specification_column_types(spec)
    intermediate_entries = spec.get('intermediates')
    intermediates = intermediate_type_catalog(spec, datasets, spec_label)

    # REQ-1242: a derive binding may read a keep-declared named intermediate.
    # The planned selection only honors `keep` with `order_by` (REQ-0119),
    # so the static single-record-per-row promise mirrors it.
    keep_intermediate_ids = frozenset(
        intermediate.get('id')
        for intermediate in intermediate_entries or ()
        if isinstance(intermediate, dict)
        and isinstance(intermediate.get('id'), str)
        and intermediate.get('keep') is not None
        and intermediate.get('order_by') is not None
    )

    keys = spec.get('keys') if isinstance(spec.get('keys'), list) else []
    column_context = {
        'resolver': predicate_resolver(
            unqualified=output_types, qualified={**datasets, **intermediates}
        ),
        'input': datasets,
        'env': env,
        'aggregate': {
            'kind': 'column',
            'input': datasets,
            'output_types': output_types,
            'keys': keys,
            'intermediate_ids': frozenset(intermediates),
            'keep_intermediate_ids': keep_intermediate_ids,
            'resolver': predicate_resolver(
                unqualified=output_types, qualified={**datasets, **intermediates}
            ),
        },
    }
    columns = spec.get('columns')
    if isinstance(columns, list):
        for index, column in enumerate(columns):
            if not isinstance(column, dict) or 'derivation' not in column:
                continue
            name = column.get('name', index)
            errors.extend(
                validate_derivation_static_semantics(
                    column['derivation'],
                    f"{spec_label}.columns.{name}.derivation",
                    column_context,
                )
            )

    rows = spec.get('rows')
    if isinstance(rows, list):
        for index, row in enumerate(rows):
            if not isinstance(row, dict):
                continue
            driver = row.get('dataset', default_driver_dataset(spec))
            derivations = row.get('derivations')
            if not isinstance(derivations, dict):
                continue
            row_output = {
                name: output_types[name]
                for name in derivations
                if name in output_types
            }
            driver_fields = datasets.get(driver, {})
            grouped = isinstance(row.get('group_by'), list)
            row_context = {
                'resolver': predicate_resolver(
                    unqualified=row_output,
                    qualified={
                        # REQ-0156/REQ-0157: a row derivation reads another
                        # dataset through the row-phase join, so binding
                        # accepts every input dataset here. Formula and
                        # predicate scopes stay driver-only in their own
                        # validators (REQ-0410, REQ-0058).
                        **datasets,
                        **intermediates,
                        **(
                            {driver: driver_fields}
                            if isinstance(driver, str)
                            else {}
                        ),
                    },
                ),
                'input': datasets,
                'env': env,
                'aggregate': {
                    'kind': 'grouped_row' if grouped else 'ungrouped_row',
                    'input': datasets,
                    'output_types': row_output,
                    'keys': keys,
                    'driver': driver,
                    'row_group_by': row.get('group_by'),
                    'intermediate_ids': frozenset(intermediates),
                    'keep_intermediate_ids': keep_intermediate_ids,
                    'resolver': predicate_resolver(
                        unqualified=row_output,
                        qualified={
                            **intermediates,
                            **(
                                {driver: driver_fields}
                                if isinstance(driver, str)
                                else {}
                            ),
                        },
                    ),
                },
            }
            for name, derivation in derivations.items():
                errors.extend(
                    validate_derivation_static_semantics(
                        derivation,
                        f"{spec_label}.rows[{index}].derivations.{name}",
                        row_context,
                    )
                )
            errors.extend(
                validate_row_window_dependencies(
                    derivations, index, spec_label, env
                )
            )

    errors.extend(
        validate_intermediate_static_semantics(
            spec, spec_label, datasets, output_types
        )
    )
    cycle = find_column_dependency_cycle(spec, env)
    if cycle is not None:
        for name in dict.fromkeys(cycle[:-1]):
            errors.append(
                validation_diagnostic(
                    derivation_primary_path(spec, spec_label, name),
                    'dependency_cycle',
                    'column dependency cycle: ' + ' -> '.join(cycle),
                    context={'cycle': cycle},
                )
            )
    forward = find_forward_reference(spec, env)
    if forward is not None:
        name, dependency = forward
        errors.append(
            validation_diagnostic(
                derivation_primary_path(spec, spec_label, name),
                'forward_reference',
                f'column {name!r} references later declared column '
                f'{dependency!r}',
                context={'column': name, 'dependency': dependency},
            )
        )
    key_dependency = find_key_dependency(spec, env)
    if key_dependency is not None:
        name, dependency = key_dependency
        errors.append(
            validation_diagnostic(
                derivation_primary_path(spec, spec_label, name),
                'key_dependency',
                f'key column {name!r} depends on non-key output column '
                f'{dependency!r}',
                context={'column': name, 'dependency': dependency},
            )
        )
    return list(dict.fromkeys(errors))


def prepare_spec_document(spec, spec_label, spec_path, env):
    if isinstance(spec, dict) and 'parents' in spec:
        resolved, errors, provenance = resolve_spec_inheritance(
            spec, spec_label, spec_path, env
        )
    else:
        resolved, errors, provenance = copy.deepcopy(spec), [], {}
    if isinstance(resolved, dict):
        from yamaa.schema.row_catalog import expand_row_catalogs
        from yamaa.specification.diagnostics import SpecificationError

        try:
            resolved = expand_row_catalogs(resolved, spec_path)
        except SpecificationError as error:
            errors.extend(
                f"ERROR: {spec_label}.{diagnostic.spec_paths[0]}: "
                f"{diagnostic.condition}"
                for diagnostic in error.diagnostics
            )
            return resolved, errors, provenance
        # REQ-0319: the engine desugars a bare-string derivation to
        # {source: string} before anything else runs. The repository
        # validator works on the same normalized shape so its paths and
        # reference walkers agree with engine diagnostics.
        resolved = _desugar_bare_derivations(resolved)
        resolved, window_errors = expand_spec_windows(resolved, env, label=spec_label)
        errors.extend(window_errors)
    return resolved, errors, provenance


def inherited_error_logical_path(path, spec):
    identities = {
        'intermediates': 'id',
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
    sources = resolved_source_files(spec, spec_path, project_root, provenance)
    errors.extend(
        validate_retired_odm_item_references(
            spec, spec_label, spec_path, env, sources
        )
    )
    errors.extend(validate_column_labels(spec, spec_label))
    errors.extend(
        validate_spec_contracts(
            spec, spec_label, spec_path, project_root, snapshots, provenance,
            sources,
        )
    )
    errors.extend(
        validate_spec_predicates(spec, spec_label, spec_path, env, sources)
    )
    errors.extend(
        validate_spec_numeric_expressions(
            spec, spec_label, spec_path, env, sources
        )
    )
    errors.extend(
        validate_spec_static_semantics(
            spec, spec_label, spec_path, env, sources
        )
    )
    errors.extend(
        validate_spec_functions(spec, spec_label, spec_path, env, sources)
    )

    next_stack = set(spec_stack or ())
    next_stack.add(spec_path.resolve())
    errors.extend(
        validate_producing_specs(
            spec, spec_label, spec_path, env, next_stack, project_root,
            snapshots, provenance,
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
    snapshots=None, provenance=None,
):
    """Validate producer workflow edges and referenced artifact headers."""
    errors = []
    datasets = spec.get('input')
    if not isinstance(datasets, dict):
        return errors
    if project_root is None:
        project_root = spec_path.parent
    if snapshots is None:
        snapshots = ProjectSnapshots()

    for dataset_id, source in datasets.items():
        if not isinstance(source, dict) or 'schema' not in source:
            continue

        path = f"{spec_label}.input.{dataset_id}"
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
        base_dir, written = layer_written_path(
            schema_ref, f"input.{dataset_id}.schema", provenance, spec_path
        )
        producer_path, condition = resolve_project_path(
            written, base_dir, project_root,
            project_data_roots(project_root),
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
        base_dir, written = layer_written_path(
            source_ref, f"input.{dataset_id}.path", provenance, spec_path
        )
        source_path, condition = resolve_project_path(
            written, base_dir, project_root,
            project_data_roots(project_root),
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
    if not (example_dir / 'spec.yaml').is_file():
        return example_dir / 'expected' / 'spec_resolved.yaml'
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


def validate_examples_structure(root: Path, env, warnings=None, manifest=None):
    errors = []
    if warnings is None:
        warnings = []
    if env is None:
        return errors

    examples_dir = root / 'benchmarks'
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
        for spec_path in example_entry_specs(ex_dir):
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
        for field in sorted(set(registration) - {'rules', 'phases'}):
            errors.append(f"ERROR: {path}.{field}: unknown field")
        rules = registration.get('rules')
        if not (
            isinstance(rules, list)
            and rules
            and all(
                isinstance(rule, str)
                and re.fullmatch(r'R[0-9]{3}', rule)
                for rule in rules
            )
            and rules == sorted(set(rules))
        ):
            errors.append(
                f"ERROR: {path}.rules: expected unique sorted rule ids"
            )
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


def _desugar_bare_derivations(node):
    """Apply the REQ-0319 bare-string derivation shorthand to a raw spec dict.

    Contracts record diagnostic paths against the normalized form, where a
    bare `derivation:` string has already become `{'source': value}` and a
    bare `input:` dataset string has already become `{'path': value}`. The
    raw file keeps the shorthand, so path checks must see the desugared
    shape or they would reject valid contract paths.
    """
    if isinstance(node, dict):
        out = {}
        for key, value in node.items():
            if key == 'derivation' and isinstance(value, str):
                out[key] = {'source': value}
            elif key == 'input' and isinstance(value, dict):
                out[key] = {
                    alias: (
                        {'path': source}
                        if isinstance(source, str)
                        else _desugar_bare_derivations(source)
                    )
                    for alias, source in value.items()
                }
            else:
                out[key] = _desugar_bare_derivations(value)
        return out
    if isinstance(node, list):
        return [_desugar_bare_derivations(item) for item in node]
    return node


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
            spec = _desugar_bare_derivations(spec)
            for path in paths:
                if condition in {'missing_required_field', 'month_required'}:
                    # The diagnostic points at the field that should exist.
                    continue
                if not spec_path_exists(spec, path):
                    errors.append(
                        f"ERROR: {label}.spec_paths: {path!r} does not exist "
                        f"in {spec_path.name}"
                    )
    return errors


BOM_UTF8 = '\ufeff'
CANONICAL_INT = re.compile(r'0|-?[1-9][0-9]*')


# R020's csv profile text. The runtime's `yamaa.io.csv` owns the profile:
# `scan_records` reads records of text-or-missing under the unified
# missing rule, and `render_records` writes the exact quoting condition.
# This file owns no second dialect.
# The import is lazy (see the regex binding note above): importing the yamaa
# package pulls in polars, which this module only needs for csv-profile checks.
CsvProfileFailure = None
fixed_point = None
render_records = None
scan_records = None
DateTimeValue = None
DateValue = None
convert_value = None


def _ensure_csv_binding():
    """Import yamaa.io.csv and yamaa.models on first use, or exit when unavailable."""
    global CsvProfileFailure, fixed_point, render_records, scan_records
    global DateTimeValue, DateValue, convert_value
    if scan_records is None:
        try:
            from yamaa.io.csv import (
                CsvProfileFailure as csv_failure,
                fixed_point as profile_fixed_point,
                render_records as profile_render_records,
                scan_records as profile_scan_records,
            )
            from yamaa.models import (
                DateTimeValue as model_datetime_value,
                DateValue as model_date_value,
                convert_value as model_convert_value,
            )
        except ImportError as error:
            raise SystemExit(
                "validate_repository.py requires the yamaa package "
                "(run: uv sync --project python --locked)"
            ) from error
        CsvProfileFailure = csv_failure
        fixed_point = profile_fixed_point
        render_records = profile_render_records
        scan_records = profile_scan_records
        DateTimeValue = model_datetime_value
        DateValue = model_date_value
        convert_value = model_convert_value


def canonical_float_text(value: str, decimals=None):
    """Return why value is not R020's text for its float, or None.

    Static validation reads a golden file rather than running a derivation,
    so it checks the form of the text and not the value behind it. The form
    itself comes from the runtime: R011's shortest text with no declared
    precision, R020's exact display rounding with one.
    """
    _ensure_csv_binding()
    try:
        number = float(value)
    except ValueError:
        return 'not a number'
    if math.isnan(number) or math.isinf(number):
        return 'a non-finite float is the missing value'
    if decimals is None:
        converted = convert_value(number, 'str')
        canonical = converted.value
        if value != canonical:
            return f'expected the shortest round-trip text {canonical}'
        return None
    canonical = fixed_point(number, decimals)
    if value != canonical:
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

    R016 fixes exactly one written form per temporal type. The verdict comes
    from the runtime's strict parsers; the shape patterns below only route
    the message, telling a misspelled field from a date no calendar admits.
    """
    _ensure_csv_binding()
    if declared == 'date':
        parsed_type = DateValue
        pattern = CANONICAL_DATE
        yours = 'expected YYYY-MM-DD'
        calendar = 'not a date on the calendar'
    else:
        parsed_type = DateTimeValue
        pattern = CANONICAL_DATETIME
        yours = 'expected YYYY-MM-DDThh:mm:ss'
        calendar = 'not a moment on the calendar'
    try:
        if parsed_type.parse(value).to_text() != value:
            return yours
    except ValueError as exc:
        if pattern.fullmatch(value) is None:
            return yours
        return f'{calendar}: {exc}'
    return None


def validate_csv_artifact(csv_path: Path, label: str, spec):
    """Check one expected artifact against R020's csv profile."""
    _ensure_csv_binding()
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
        records = scan_records(data)
    except ValueError as exc:
        return [f"ERROR: {label}: csv: {exc}"]
    if not records:
        return [f"ERROR: {label}: csv carries no header record"]
    if render_records(records[0], records[1:]).decode('utf-8') != data:
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

    header = records[0]
    for number, record in enumerate(records[1:], 2):
        for name, text in zip(header, record):
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


# REQ-1182 admits a container only when it carries every value the
# language does; REQ-1183 keeps SAS Transport out on that ground.
ARTIFACT_PROFILES = {'.csv': 'csv', '.parquet': 'parquet'}
WARNING_LOG_TYPES = {
    'LOG_VERSION': 'str',
    'ARTIFACT': 'str',
    'SEVERITY': 'str',
    'CONDITION': 'str',
    'REQUIREMENT': 'str',
    'SPEC_PATH': 'str',
    'VERIFICATION_ID': 'str',
    'FAILURE_COUNT': 'int',
    'OFFENDING_KEYS': 'str',
    'DETAILS': 'str',
}
VERIFICATION_LOG_TYPES = {
    'REPORT_VERSION': 'str',
    'ARTIFACT': 'str',
    'SPEC_PATH': 'str',
    'VERIFICATION_ID': 'str',
    'CHECK': 'str',
    'TARGET': 'str',
    'REQUIREMENT': 'str',
    'SEVERITY': 'str',
    'OUTCOME': 'str',
    'CONDITION': 'str',
    'EVALUATED_COUNT': 'int',
    'FAILURE_COUNT': 'int',
    'DETAILS': 'str',
}


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


# R023's syntax has one implementation. The package module loaded here is
# the reader a runtime uses, and it imports the standard library alone so
# this script can load it by path without installing anything. What a
# repository check adds stays below: fixture wording, and reporting every
# departure in one pass where a runtime stops at the first.
_CSV_PROFILE_PATH = (
    Path(__file__).resolve().parents[3]
    / 'python' / 'src' / 'yamaa' / 'io' / 'csv.py'
)
_CSV_PROFILE_SPEC = importlib.util.spec_from_file_location(
    'yamaa_io_csv', _CSV_PROFILE_PATH
)
CSV_PROFILE = importlib.util.module_from_spec(_CSV_PROFILE_SPEC)
_CSV_PROFILE_SPEC.loader.exec_module(CSV_PROFILE)

SourceProfileError = CSV_PROFILE.CsvProfileFailure
parse_source_profile = CSV_PROFILE.scan_records
source_coordinates = CSV_PROFILE.record_coordinates


SOURCE_PROFILE_CONDITIONS = {
    'source_profile_unknown': 'an extension R023 does not map',
    'source_byte_order_mark': 'source carries a byte-order mark',
    'source_header_absent': 'source has no header record',
    'source_field_name_empty': 'header contains an empty name',
    'source_field_name_duplicate': 'duplicate header(s)',
    'source_record_width': 'record width is not the header width',
    'source_quote_unterminated': 'a quoted field has no closing quote',
    'source_quote_in_bare_field': 'a bare field carries U+0022',
    'source_text_after_quote': 'a closing quote is followed by ordinary text',
    'source_carriage_return': 'U+000D does not begin a record terminator',
}
# What reading one fixture can decide. `source_profile_unknown` is settled
# from the written path while a specification is validated, so no fixture
# provokes it and a negative example that declares it is answered there.
SOURCE_READ_CONDITIONS = (
    set(SOURCE_PROFILE_CONDITIONS) - {'source_profile_unknown'}
) | {'invalid_text'}


def check_source_file(csv_path: Path):
    """Report every way one fixture departs from R023's source profile.

    Each finding is a condition, the record it was decided at, and the
    detail a reader needs to find it.
    """
    raw = csv_path.read_bytes()
    try:
        data = raw.decode('utf-8')
    except UnicodeDecodeError as exc:
        # Everything before the offending byte decoded, so the reader's
        # position in the source is exactly what that prefix spells.
        record, field = source_coordinates(raw[:exc.start].decode('utf-8'))
        return [(
            'invalid_text',
            record,
            f"ill-formed encoded text at field {field}: {exc.reason}",
        )]
    if data.startswith(BOM_UTF8):
        return [('source_byte_order_mark', 1, 'source carries a byte-order mark')]
    try:
        records = parse_source_profile(data)
    except SourceProfileError as exc:
        return [(
            exc.condition,
            exc.record,
            f"{SOURCE_PROFILE_CONDITIONS[exc.condition]} at field "
            f"{exc.field}",
        )]
    if not records:
        return [('source_header_absent', 1, 'source has no header record')]

    findings = []
    header = records[0]
    if any(not name for name in header):
        findings.append(
            ('source_field_name_empty', 1, 'header contains an empty name')
        )
    duplicates = sorted(
        {name for name in header if name and header.count(name) > 1}
    )
    if duplicates:
        findings.append((
            'source_field_name_duplicate',
            1,
            'duplicate header(s): ' + ', '.join(duplicates),
        ))
    for number, record in enumerate(records[1:], 2):
        if len(record) != len(header):
            findings.append((
                'source_record_width',
                number,
                f"expected {len(header)} fields, got {len(record)}",
            ))
    return findings


def declared_source_condition(example_dir: Path):
    """The source condition a negative example declares, if any."""
    if not example_dir.name.startswith('negative-'):
        return None
    error_path = example_dir / 'expected' / 'error.yaml'
    if not error_path.is_file():
        return None
    try:
        contract = yaml.safe_load(error_path.read_text(encoding='utf-8'))
    except (OSError, UnicodeError, yaml.YAMLError):
        return None
    if not isinstance(contract, dict):
        return None
    condition = contract.get('condition')
    return condition if condition in SOURCE_READ_CONDITIONS else None


def validate_csv_shapes(root: Path):
    """Check every fixture against R023's csv source profile.

    The reader preserves quoting, so a bare empty field stays distinct from
    a quoted empty one rather than being normalized to the same text. A
    negative example that declares a source condition may carry the fixture
    that provokes it, and must actually provoke it: a malformed fixture no
    example asked for is still an error, and a declared condition no fixture
    reports means the example has stopped failing the way it claims to.
    """
    errors = []
    examples_dir = root / 'benchmarks'
    if not examples_dir.exists():
        return errors
    for example_dir in sorted(
        path for path in examples_dir.iterdir() if path.is_dir()
    ):
        declared = declared_source_condition(example_dir)
        reported = set()
        csv_paths = []
        for directory in ('input', 'expected'):
            csv_paths.extend(sorted(
                path for path in (example_dir / directory).rglob('*')
                if path.is_file() and path.suffix.lower() == '.csv'
            ))
        for csv_path in csv_paths:
            label = csv_path.relative_to(root)
            try:
                findings = check_source_file(csv_path)
            except OSError as exc:
                errors.append(f"ERROR: {label}: cannot read source: {exc}")
                continue
            for condition, number, detail in findings:
                reported.add(condition)
                if condition == declared:
                    continue
                errors.append(
                    f"ERROR: {label}:{number}: {condition}: {detail}"
                )
        if declared is not None and declared not in reported:
            errors.append(
                f"ERROR: {example_dir.relative_to(root)}: {declared} is "
                "declared but no source fixture reports it"
            )
    return errors


KEY_COLUMN_ORDER = ('DOMAIN', 'STUDYID', 'USUBJID')


def _key_column_order_errors(names, label):
    """Check populated key columns keep DOMAIN, STUDYID, USUBJID in order.

    Only key columns present in `names` constrain the order; an absent key
    column imposes nothing.
    """
    present = [column for column in KEY_COLUMN_ORDER if column in names]
    actual = [column for column in names if column in present]
    if actual != present:
        return [
            f"ERROR: {label}: key columns out of order: "
            f"{', '.join(actual)}; populated key columns keep the order "
            f"{', '.join(present)}"
        ]
    return []


def validate_key_column_order(root: Path):
    """Keep DOMAIN, STUDYID, USUBJID in that order across benchmark artifacts.

    Checks each entry spec's declared `columns`, its `output.columns`
    artifact order, and the golden CSV header, so the column order the
    GitHub Action verifies stays consistent everywhere a benchmark's
    columns are populated.
    """
    errors = []
    examples_dir = root / 'benchmarks'
    if not examples_dir.exists():
        return errors
    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith('.'):
            continue
        for spec_path in example_entry_specs(ex_dir):
            try:
                with open(spec_path, 'r', encoding='utf-8') as handle:
                    spec = yaml.load(handle, Loader=UniqueKeyLoader)
            except Exception:
                continue
            if not isinstance(spec, dict):
                continue
            declared = spec.get('columns')
            if isinstance(declared, list):
                names = [
                    column.get('name') if isinstance(column, dict) else column
                    for column in declared
                ]
                names = [name for name in names if isinstance(name, str)]
                errors.extend(_key_column_order_errors(
                    names, f"{ex_dir.name}/{spec_path.name}: declared columns",
                ))
            artifact_name = None
            output = spec.get('output')
            if isinstance(output, dict):
                artifact_columns = output.get('columns')
                if (
                    isinstance(artifact_columns, list)
                    and all(isinstance(c, str) for c in artifact_columns)
                ):
                    errors.extend(_key_column_order_errors(
                        artifact_columns,
                        f"{ex_dir.name}/{spec_path.name}: output columns",
                    ))
                artifact_path = output.get('path')
                if isinstance(artifact_path, str) and artifact_path:
                    artifact_name = PurePosixPath(artifact_path).name
            if artifact_name:
                golden = ex_dir / 'expected' / artifact_name
                # Parquet goldens carry no header contract; only CSV
                # goldens pin the column order.
                if golden.suffix.lower() != '.csv' or not golden.is_file():
                    continue
                try:
                    with open(golden, 'r', encoding='utf-8', newline='') as f:
                        header = next(csv.reader(f))
                except (StopIteration, UnicodeDecodeError, OSError):
                    # Empty, non-UTF-8, or unreadable goldens are
                    # reported by the CSV shape checks; skip them here.
                    continue
                errors.extend(_key_column_order_errors(
                    header,
                    f"{ex_dir.name}/expected/{artifact_name}: header",
                ))
    return errors


# One machine-readable grammar per closed language. Each file is the single
# source for its rule's grammar block, for this validator's parser, and for
# the R parser, so a copy that drifts from it fails validation instead of
# quietly disagreeing at run time.
GRAMMAR_DIR = PurePosixPath('yaml/grammar')
GRAMMAR_CONTRACTS = {
    'predicate': 'operations/predicates',
    'numeric': 'operations/computation',
    'string-template': 'operations/text',
    'aggregate': 'operations/aggregation',
}
GRAMMAR_DOCUMENT_KEYS = {
    'schema_version', 'contract', 'contract_version', 'rule', 'start',
    'productions', 'imports', 'vocabulary', 'prohibited', 'reserved',
    'cases',
}
GRAMMAR_REQUIRED_KEYS = (
    'schema_version', 'contract', 'contract_version', 'rule', 'start',
    'productions', 'imports', 'vocabulary', 'cases',
)
GRAMMAR_PRODUCTION_KEYS = {'name', 'definition', 'prose'}
GRAMMAR_CASE_KEYS = {
    'id', 'covers', 'text', 'parse', 'condition', 'identifiers', 'shape',
}
# The behavior each contract's vectors must exercise. A vector set that
# stops covering one of these stops being evidence for it, and a vector that
# invents a category outside them hides what it is evidence for.
GRAMMAR_COVERS = {
    'predicate': {
        'between', 'call', 'comparison', 'escape', 'grouping', 'identifier',
        'in', 'keyword-case', 'like', 'literal', 'logic', 'null-test',
        'precedence', 'rejection', 'reserved', 'temporal',
    },
    'numeric': {
        'arithmetic', 'call', 'grouping', 'identifier', 'keyword-case',
        'literal', 'null-literal', 'precedence', 'prohibited',
        'rejection', 'unary', 'vocabulary',
    },
    'string-template': {
        'escape', 'identifier', 'placeholder', 'rejection', 'text',
    },
    'aggregate': {
        'arithmetic', 'call', 'grouping', 'identifier', 'keyword-case',
        'literal', 'null-literal', 'precedence', 'prohibited',
        'reduction', 'rejection', 'star', 'unary', 'vocabulary',
    },
}
# The failures a rejected vector may record. Every one is a condition its
# owning rule registers, so a vector cannot pin a failure the language does
# not name.
GRAMMAR_CONDITIONS = {
    'predicate': {'invalid_predicate'},
    'numeric': {
        'invalid_numeric_expression', 'prohibited_construct',
        'prohibited_function',
    },
    'string-template': {'invalid_string_template'},
    'aggregate': {
        'invalid_aggregate_expression', 'nested_reduction',
        'prohibited_construct', 'prohibited_function',
    },
}


def grammar_numeric_resolver(_name):
    """Type every identifier in a replayed vector as numeric.

    A vector pins what the grammar and its closed vocabulary decide. Which
    names are visible, and what they are typed, belongs to R001, R002, and
    R007, so a replay resolves every identifier rather than importing a
    binding context the vector does not declare.
    """
    return 'float', None


def quote_grammar_scalar(value):
    """Quote a literal for a shape the way R004 quotes a string."""
    doubled = value.replace("'", "''")
    return f"'{doubled}'"


def predicate_operand_shape(node):
    if node['kind'] == 'identifier':
        return f"(id {node['name']})"
    value_type = node['type']
    if value_type is None:
        return 'null'
    if value_type in {'str', 'date', 'datetime'}:
        return f"({value_type} {quote_grammar_scalar(node['value'])})"
    return f"({value_type} {node['value']})"


def predicate_shape(node):
    """Render a parsed predicate as the prefix form a vector records."""
    kind = node['kind']
    if kind in {'and', 'or'}:
        return (
            f"({kind} {predicate_shape(node['left'])} "
            f"{predicate_shape(node['right'])})"
        )
    if kind == 'not':
        return f"(not {predicate_shape(node['value'])})"
    if kind == 'boolean':
        return 'true' if node['value'] else 'false'
    if kind == 'comparison':
        return (
            f"({node['operator']} {predicate_operand_shape(node['left'])} "
            f"{predicate_operand_shape(node['right'])})"
        )
    if kind == 'null_test':
        name = 'is-not-null' if node['negated'] else 'is-null'
        return f"({name} {predicate_operand_shape(node['value'])})"
    if kind == 'in':
        name = 'not-in' if node['negated'] else 'in'
        operands = ' '.join(
            predicate_operand_shape(operand) for operand in node['values']
        )
        return f"({name} {predicate_operand_shape(node['value'])} {operands})"
    if kind == 'between':
        name = 'not-between' if node['negated'] else 'between'
        return (
            f"({name} {predicate_operand_shape(node['value'])} "
            f"{predicate_operand_shape(node['lower'])} "
            f"{predicate_operand_shape(node['upper'])})"
        )
    if kind == 'like':
        name = 'not-like' if node['negated'] else 'like'
        rendered = (
            f"({name} {predicate_operand_shape(node['value'])} "
            f"{predicate_operand_shape(node['pattern'])}"
        )
        if node['escape'] is not None:
            rendered += f" (escape {quote_grammar_scalar(node['escape'])})"
        return rendered + ')'
    if kind == 'call':
        return (
            f"(str-contains {predicate_operand_shape(node['source'])} "
            f"(str {quote_grammar_scalar(node['pattern'])})"
            f")"
        )
    raise AssertionError(f"unknown predicate AST node {kind!r}")


def expression_shape(node):
    """Render a parsed R010 or R013 expression as a vector's prefix form."""
    kind = node['kind']
    if kind == 'number':
        return f"({node['type']} {node['value']})"
    if kind == 'null':
        return 'null'
    if kind == 'identifier':
        return f"(id {node['name']})"
    if kind == 'qualified_star':
        return f"(star {node['dataset']})"
    if kind == 'unary':
        name = 'neg' if node['operator'] == '-' else 'pos'
        return f"({name} {expression_shape(node['value'])})"
    if kind == 'binary':
        return (
            f"({node['operator']} {expression_shape(node['left'])} "
            f"{expression_shape(node['right'])})"
        )
    if kind == 'reduction':
        return (
            f"(reduce {node['name'].upper()} "
            f"{expression_shape(node['argument'])})"
        )
    if kind == 'call':
        rendered = f"(call {node['name'].upper()}"
        for argument in node['arguments']:
            rendered += f" {expression_shape(argument)}"
        return rendered + ')'
    raise AssertionError(f"unknown expression AST node {kind!r}")


def string_template_shape(parts):
    """Render parsed template parts as the prefix form a vector records."""
    rendered = '(template'
    for part in parts:
        if part['kind'] == 'text':
            rendered += f" (text {quote_grammar_scalar(part['value'])})"
        else:
            rendered += f" (placeholder {part['name']})"
    return rendered + ')'


def decide_predicate(text):
    _ensure_predicate_binding()
    try:
        ast = parse_predicate(text)
    except PredicateError:
        return 'invalid_predicate', None, set()
    return None, predicate_shape(ast), ast_identifier_names(ast)


def decide_numeric(text):
    try:
        ast = parse_numeric_expression(text)
    except NumericExpressionError as exc:
        return exc.condition, None, set()
    _, errors = validate_numeric_expression_ast(
        ast, 'grammar', text, grammar_numeric_resolver
    )
    if errors:
        return errors[0].condition, None, set()
    return (
        None,
        expression_shape(ast),
        numeric_expression_identifier_names(text),
    )


def decide_aggregate(text):
    try:
        ast = parse_aggregate_expression(text)
    except AggregateExpressionError as exc:
        return exc.condition, None, set()
    identifiers = aggregate_expression_identifier_names(text)
    _, errors = validate_aggregate_expression_ast(
        ast, 'grammar', text, grammar_numeric_resolver, identifiers, None
    )
    if errors:
        return errors[0].condition, None, set()
    return None, expression_shape(ast), identifiers


def decide_string_template(text):
    try:
        parts = parse_string_template(text)
    except StringTemplateError:
        return 'invalid_string_template', None, set()
    return (
        None,
        string_template_shape(parts),
        string_template_identifier_names(text),
    )


GRAMMAR_DECISIONS = {
    'predicate': decide_predicate,
    'numeric': decide_numeric,
    'string-template': decide_string_template,
    'aggregate': decide_aggregate,
}


def render_grammar_block(productions):
    """Render productions as the grammar block a rule carries.

    The name column is as wide as the longest production name. A
    continuation line that opens an alternative aligns its bar under the
    definition operator; every other continuation aligns under the
    definition itself.
    """
    width = max(len(production['name']) for production in productions)
    lines = []
    for production in productions:
        definition = production['definition'].split('\n')
        lines.append(f"{production['name'].ljust(width)} := {definition[0]}")
        for continuation in definition[1:]:
            indent = width + (2 if continuation.startswith('|') else 4)
            lines.append(' ' * indent + continuation)
    return '\n'.join(lines)


def rule_grammar_block(text):
    """Return the grammar block written in a rule's Grammar section."""
    section = re.search(r'\n#{2,3} (?:Templates: )?Grammar\n(.*?)(?=\n#{2,3} |\Z)', text, re.DOTALL)
    if section is None:
        return None
    block = re.search(r'```text\n(.*?)\n```', section.group(1), re.DOTALL)
    return None if block is None else block.group(1)


def grammar_symbol_references(definition):
    """Return the non-terminals an EBNF definition refers to."""
    without_terminals = re.sub(r'"[^"]*"', ' ', definition)
    return set(re.findall(r'[A-Za-z_][A-Za-z0-9_]*', without_terminals))


def grammar_defined_symbols(document):
    """Return every symbol a grammar document defines or closes."""
    defined = set()
    productions = document.get('productions')
    if isinstance(productions, list):
        for production in productions:
            if isinstance(production, dict) and isinstance(
                production.get('name'), str
            ):
                defined.add(production['name'])
    vocabulary = document.get('vocabulary')
    if isinstance(vocabulary, dict):
        defined.update(
            name for name in vocabulary if isinstance(name, str)
        )
    for closed in ('prohibited', 'reserved'):
        if closed in document:
            defined.add(closed)
    return defined


def grammar_vocabulary_errors(contract, document, label):
    """Compare a contract's closed vocabulary with this validator's."""
    _ensure_predicate_binding()
    errors = []
    vocabulary = document.get('vocabulary')
    if not isinstance(vocabulary, dict):
        return [f"ERROR: {label}: vocabulary must be a mapping"]

    if contract == 'predicate':
        reserved = document.get('reserved')
        if not isinstance(reserved, list) or not all(
            isinstance(name, str) for name in reserved
        ):
            errors.append(f"ERROR: {label}: reserved must be a list of names")
        elif set(reserved) != set(PREDICATE_RESERVED_NAMES):
            errors.append(
                f"ERROR: {label}: reserved {sorted(reserved)} does not name "
                f"the names this validator reserves, "
                f"{sorted(PREDICATE_RESERVED_NAMES)}"
            )
        compare = next(
            (
                production
                for production in document['productions']
                if isinstance(production, dict)
                and production.get('name') == 'compare'
            ),
            None,
        )
        declared = (
            re.findall(r'"([^"]*)"', compare.get('definition', ''))
            if isinstance(compare, dict)
            else []
        )
        if declared != list(PREDICATE_COMPARISON_OPERATORS):
            errors.append(
                f"ERROR: {label}: compare declares {declared}, not the "
                f"operators this validator tokenizes, "
                f"{list(PREDICATE_COMPARISON_OPERATORS)}"
            )
        return errors

    if contract == 'numeric':
        functions = vocabulary.get('function')
        if not isinstance(functions, dict):
            errors.append(f"ERROR: {label}: vocabulary.function is missing")
        else:
            declared = {}
            for name, arity in sorted(functions.items()):
                if not isinstance(arity, dict) or set(arity) != {
                    'min_arguments', 'max_arguments'
                }:
                    errors.append(
                        f"ERROR: {label}: function {name} must declare "
                        "min_arguments and max_arguments"
                    )
                    continue
                declared[name] = (
                    arity['min_arguments'], arity['max_arguments']
                )
            if declared != NUMERIC_FUNCTION_ARITIES:
                errors.append(
                    f"ERROR: {label}: vocabulary.function does not name the "
                    "functions and arities this validator permits"
                )
        prohibited = document.get('prohibited')
        if prohibited != PROHIBITED_NUMERIC_KEYWORDS:
            errors.append(
                f"ERROR: {label}: prohibited does not name the constructs "
                "this validator refuses"
            )
        return errors

    if contract == 'aggregate':
        reducers = vocabulary.get('reducer')
        if not isinstance(reducers, dict):
            errors.append(f"ERROR: {label}: vocabulary.reducer is missing")
        else:
            if set(reducers) != AGGREGATE_REDUCERS:
                errors.append(
                    f"ERROR: {label}: vocabulary.reducer {sorted(reducers)} "
                    f"does not name the reducers this validator permits, "
                    f"{sorted(AGGREGATE_REDUCERS)}"
                )
            for name, entry in sorted(reducers.items()):
                arguments = (
                    entry.get('argument') if isinstance(entry, dict) else None
                )
                expected = ['expr', 'star'] if name == 'COUNT' else ['expr']
                if arguments != expected:
                    errors.append(
                        f"ERROR: {label}: reducer {name} must declare "
                        f"argument {expected}"
                    )
        imports = document.get('imports')
        if not isinstance(imports, dict):
            return errors
        for symbol in ('function', 'prohibited'):
            if imports.get(symbol) != 'numeric':
                errors.append(
                    f"ERROR: {label}: {symbol} must be imported from the "
                    "numeric contract rather than restated"
                )
        return errors

    if vocabulary:
        errors.append(f"ERROR: {label}: vocabulary must be empty")
    return errors


def grammar_case_errors(contract, case, label):
    """Replay one vector against this validator's parser."""
    errors = []
    unknown = sorted(set(case) - GRAMMAR_CASE_KEYS)
    if unknown:
        errors.append(f"ERROR: {label}: unknown keys {unknown}")

    covers = case.get('covers')
    if not isinstance(covers, list) or not covers:
        errors.append(f"ERROR: {label}: covers must be a non-empty list")
    else:
        outside = sorted(set(covers) - GRAMMAR_COVERS[contract])
        if outside:
            errors.append(f"ERROR: {label}: unknown covers {outside}")

    text = case.get('text')
    if not isinstance(text, str):
        errors.append(f"ERROR: {label}: text must be a string")
        return errors

    outcome = case.get('parse')
    if outcome not in {'accept', 'reject'}:
        errors.append(f"ERROR: {label}: parse must be 'accept' or 'reject'")
        return errors

    condition, shape, identifiers = GRAMMAR_DECISIONS[contract](text)

    if outcome == 'reject':
        for key in ('identifiers', 'shape'):
            if key in case:
                errors.append(
                    f"ERROR: {label}: a rejected text records no {key}"
                )
        declared = case.get('condition')
        if declared not in GRAMMAR_CONDITIONS[contract]:
            errors.append(
                f"ERROR: {label}: condition must be one of "
                f"{sorted(GRAMMAR_CONDITIONS[contract])}"
            )
        elif condition is None:
            errors.append(
                f"ERROR: {label}: this validator accepts text {text!r} the "
                "vector records as rejected"
            )
        elif condition != declared:
            errors.append(
                f"ERROR: {label}: this validator fails with {condition!r}, "
                f"not the {declared!r} the vector records"
            )
        return errors

    if 'condition' in case:
        errors.append(f"ERROR: {label}: an accepted text records no condition")
    if condition is not None:
        errors.append(
            f"ERROR: {label}: this validator rejects text {text!r} with "
            f"{condition!r}, and the vector records it as accepted"
        )
        return errors

    declared_identifiers = case.get('identifiers')
    if not isinstance(declared_identifiers, list) or not all(
        isinstance(name, str) for name in declared_identifiers
    ):
        errors.append(f"ERROR: {label}: identifiers must be a list of names")
    elif declared_identifiers != sorted(declared_identifiers):
        errors.append(f"ERROR: {label}: identifiers must be sorted")
    elif declared_identifiers != sorted(identifiers):
        errors.append(
            f"ERROR: {label}: this validator collects "
            f"{sorted(identifiers)}, not the {declared_identifiers} the "
            "vector records"
        )

    declared_shape = case.get('shape')
    if not isinstance(declared_shape, str):
        errors.append(f"ERROR: {label}: shape must be a string")
    elif declared_shape != shape:
        errors.append(
            f"ERROR: {label}: this validator parses {text!r} as "
            f"{shape!r}, not the {declared_shape!r} the vector records"
        )
    return errors


def validate_grammar_contract(root: Path, contract: str):
    """Check one closed grammar against its rule and this validator."""
    label = str(GRAMMAR_DIR / f'{contract}.yaml')
    document_path = root / GRAMMAR_DIR / f'{contract}.yaml'
    if not document_path.exists():
        return [f"ERROR: {label}: missing machine-readable grammar"]

    try:
        with open(document_path, 'r', encoding='utf-8') as handle:
            document = yaml.load(handle, Loader=UniqueKeyLoader)
    except Exception as exc:
        return [f"ERROR: {label}: {exc}"]

    if not isinstance(document, dict):
        return [f"ERROR: {label}: expected a mapping"]

    errors = []
    unknown = sorted(set(document) - GRAMMAR_DOCUMENT_KEYS)
    if unknown:
        errors.append(f"ERROR: {label}: unknown keys {unknown}")
    missing = [key for key in GRAMMAR_REQUIRED_KEYS if key not in document]
    if missing:
        errors.append(f"ERROR: {label}: missing keys {missing}")
        return errors
    if document['contract'] != contract:
        errors.append(
            f"ERROR: {label}: contract must be {contract!r}, got "
            f"{document['contract']!r}"
        )
    rule_id = GRAMMAR_CONTRACTS[contract]
    if document['rule'] != rule_id:
        errors.append(
            f"ERROR: {label}: rule must be {rule_id!r}, got "
            f"{document['rule']!r}"
        )

    productions = document['productions']
    if not isinstance(productions, list) or not productions:
        errors.append(f"ERROR: {label}: productions must be a non-empty list")
        return errors

    names = []
    for index, production in enumerate(productions):
        if not isinstance(production, dict):
            errors.append(
                f"ERROR: {label}: productions[{index}] must be a mapping"
            )
            continue
        unknown = sorted(set(production) - GRAMMAR_PRODUCTION_KEYS)
        if unknown:
            errors.append(
                f"ERROR: {label}: productions[{index}] unknown keys {unknown}"
            )
        name = production.get('name')
        definition = production.get('definition')
        if not isinstance(name, str) or not name:
            errors.append(
                f"ERROR: {label}: productions[{index}].name must be a name"
            )
            continue
        if name in names:
            errors.append(f"ERROR: {label}: {name}: duplicate production")
            continue
        names.append(name)
        if not isinstance(definition, str) or not definition:
            errors.append(
                f"ERROR: {label}: {name}: definition must be a non-empty "
                "string"
            )
        if 'prose' in production and production['prose'] is not True:
            errors.append(
                f"ERROR: {label}: {name}: prose must be true or absent"
            )
    if errors:
        return errors

    if document['start'] not in names:
        errors.append(
            f"ERROR: {label}: start {document['start']!r} names no production"
        )

    imports = document['imports']
    if not isinstance(imports, dict):
        errors.append(f"ERROR: {label}: imports must be a mapping")
        imports = {}
    defined = grammar_defined_symbols(document) | set(imports)
    for production in productions:
        if production.get('prose') is True:
            continue
        undefined = sorted(
            grammar_symbol_references(production['definition']) - defined
        )
        if undefined:
            errors.append(
                f"ERROR: {label}: {production['name']}: undefined symbols "
                f"{undefined}"
            )

    for symbol, source in sorted(imports.items()):
        if source == 'schema':
            continue
        if source not in GRAMMAR_CONTRACTS:
            errors.append(
                f"ERROR: {label}: {symbol} is imported from unknown contract "
                f"{source!r}"
            )
            continue
        source_path = root / GRAMMAR_DIR / f'{source}.yaml'
        try:
            with open(source_path, 'r', encoding='utf-8') as handle:
                source_document = yaml.load(handle, Loader=UniqueKeyLoader)
        except Exception:
            errors.append(
                f"ERROR: {label}: {symbol} is imported from {source!r}, "
                "which cannot be read"
            )
            continue
        if symbol not in grammar_defined_symbols(source_document):
            errors.append(
                f"ERROR: {label}: the {source!r} contract defines no "
                f"{symbol!r} to import"
            )

    errors.extend(grammar_vocabulary_errors(contract, document, label))

    rule_path = root / 'rules' / f'{rule_id}.md'
    if not rule_path.is_file():
        errors.append(f"ERROR: {label}: rule {rule_id} has no file")
    else:
        block = rule_grammar_block(rule_path.read_text(encoding='utf-8'))
        rendered = render_grammar_block(productions)
        if block is None:
            errors.append(
                f"ERROR: {rule_path.relative_to(root)}: no grammar block to "
                f"compare with {label}"
            )
        elif block != rendered:
            errors.append(
                f"ERROR: {rule_path.relative_to(root)}: the grammar block is "
                f"not the one {label} renders; replace it with:\n{rendered}"
            )

    cases = document['cases']
    if not isinstance(cases, list) or not cases:
        errors.append(f"ERROR: {label}: cases must be a non-empty list")
        return errors

    seen = set()
    covered = set()
    for index, case in enumerate(cases):
        if not isinstance(case, dict):
            errors.append(f"ERROR: {label}: cases[{index}] must be a mapping")
            continue
        case_id = case.get('id')
        if not isinstance(case_id, str) or not case_id:
            errors.append(
                f"ERROR: {label}: cases[{index}].id must be a non-empty "
                "string"
            )
            continue
        if case_id in seen:
            errors.append(f"ERROR: {label}: {case_id}: duplicate case id")
            continue
        seen.add(case_id)
        if isinstance(case.get('covers'), list):
            covered.update(
                name for name in case['covers'] if isinstance(name, str)
            )
        errors.extend(
            grammar_case_errors(contract, case, f"{label}: {case_id}")
        )

    missing_covers = sorted(GRAMMAR_COVERS[contract] - covered)
    if missing_covers:
        errors.append(f"ERROR: {label}: no case covers {missing_covers}")
    return errors


def validate_grammar_contracts(root: Path):
    """Check every closed grammar the language defines."""
    errors = []
    for contract in sorted(GRAMMAR_CONTRACTS):
        errors.extend(validate_grammar_contract(root, contract))
    return errors


REGEX_CONFORMANCE_PATH = PurePosixPath('yaml/conformance/regex.yaml')
REGEX_FIXTURE_COVERS = {
    'anchors',
    'capture-groups',
    'dialect',
    'empty-match',
    'escaping',
    'search',
    'unicode',
    'unsupported',
}
# The categories R022 requires the shared fixtures to exercise. A fixture set
# that stops covering one of them stops being evidence for that behavior.
REGEX_FIXTURE_REQUIRED_COVERS = {
    'anchors',
    'capture-groups',
    'empty-match',
    'escaping',
    'unicode',
    'unsupported',
}
REGEX_FIXTURE_CASE_KEYS = {
    'id', 'covers', 'pattern', 'subject', 'schema_pattern', 'matches',
    'str_extract', 'invalid',
}


def validate_regex_conformance(root: Path):
    """Replay R022's shared fixtures against the portable contract binding.

    Every case records what all three consumers produce for one pattern and
    subject. Replaying them here keeps the fixtures, this validator, and the
    shared binding from drifting apart, and gives the R implementation a file
    whose expected values are already known to be reachable.
    """
    label = str(REGEX_CONFORMANCE_PATH)
    document_path = root / REGEX_CONFORMANCE_PATH
    if not document_path.exists():
        return [f"ERROR: {label}: missing R022 regular-expression fixtures"]

    try:
        with open(document_path, 'r', encoding='utf-8') as handle:
            document = yaml.load(handle, Loader=UniqueKeyLoader)
    except Exception as exc:
        return [f"ERROR: {label}: {exc}"]

    if not isinstance(document, dict):
        return [f"ERROR: {label}: expected a mapping"]

    errors = []
    if document.get('contract') != REGEX_CONTRACT:
        errors.append(
            f"ERROR: {label}: contract {document.get('contract')!r} does not "
            f"name the portable contract this validator replays, "
            f"{REGEX_CONTRACT!r}"
        )
    if document.get('contract_version') != REGEX_CONTRACT_VERSION:
        errors.append(
            f"ERROR: {label}: contract_version "
            f"{document.get('contract_version')!r} is not the contract "
            f"version this validator replays, {REGEX_CONTRACT_VERSION!r}"
        )
    if 'engine' in document:
        errors.append(
            f"ERROR: {label}: the portable contract pins no engine"
        )

    cases = document.get('cases')
    if not isinstance(cases, list) or not cases:
        errors.append(f"ERROR: {label}: cases must be a non-empty list")
        return errors

    seen = set()
    covered = set()
    for index, case in enumerate(cases):
        case_label = f"{label}: cases[{index}]"
        if not isinstance(case, dict):
            errors.append(f"ERROR: {case_label}: expected a mapping")
            continue
        case_id = case.get('id')
        if not isinstance(case_id, str) or not case_id:
            errors.append(
                f"ERROR: {case_label}: id must be a non-empty string"
            )
            continue
        case_label = f"{label}: {case_id}"
        if case_id in seen:
            errors.append(f"ERROR: {case_label}: duplicate case id")
            continue
        seen.add(case_id)

        unknown = sorted(set(case) - REGEX_FIXTURE_CASE_KEYS)
        if unknown:
            errors.append(f"ERROR: {case_label}: unknown keys {unknown}")

        covers = case.get('covers')
        if not isinstance(covers, list) or not covers:
            errors.append(
                f"ERROR: {case_label}: covers must be a non-empty list"
            )
        else:
            outside = sorted(set(covers) - REGEX_FIXTURE_COVERS)
            if outside:
                errors.append(f"ERROR: {case_label}: unknown covers {outside}")
            covered.update(covers)

        pattern = case.get('pattern')
        if not isinstance(pattern, str):
            errors.append(f"ERROR: {case_label}: pattern must be a string")
            continue

        errors.extend(_regex_fixture_case_errors(case, case_label, pattern))

    missing = sorted(REGEX_FIXTURE_REQUIRED_COVERS - covered)
    if missing:
        errors.append(
            f"ERROR: {label}: no case covers {missing}"
        )
    return errors


def _regex_fixture_case_errors(case, case_label, pattern):
    """Compare one fixture case with what the portable binding produces."""
    errors = []
    if case.get('invalid') is True:
        for key in ('subject', 'schema_pattern', 'matches', 'str_extract'):
            if key in case:
                errors.append(
                    f"ERROR: {case_label}: a rejected pattern records no {key}"
                )
        try:
            compile_regex(pattern)
        except InvalidRegex:
            return errors
        errors.append(
            f"ERROR: {case_label}: the portable binding accepts pattern "
            f"{pattern!r} the fixture records as invalid"
        )
        return errors

    if 'invalid' in case:
        errors.append(
            f"ERROR: {case_label}: invalid must be true or absent"
        )
    subject = case.get('subject')
    if not isinstance(subject, str):
        errors.append(f"ERROR: {case_label}: subject must be a string")
        return errors

    try:
        actual_full = regex_full_match(pattern, subject)
        actual_search = regex_search(pattern, subject) is not None
    except InvalidRegex as exc:
        errors.append(
            f"ERROR: {case_label}: the portable binding rejects pattern "
            f"{pattern!r}: {exc.reason}"
        )
        return errors

    for key, actual in (
        ('schema_pattern', actual_full),
        ('matches', actual_search),
    ):
        recorded = case.get(key)
        if type(recorded) is not bool:
            errors.append(f"ERROR: {case_label}: {key} must be true or false")
        elif recorded is not actual:
            errors.append(
                f"ERROR: {case_label}: {key} records {recorded} but the "
                f"portable binding produces {actual}"
            )

    errors.extend(
        _regex_fixture_extract_errors(case, case_label, pattern, subject)
    )
    return errors


def _regex_fixture_extract_errors(case, case_label, pattern, subject):
    """Compare one fixture's recorded str_extract result with the engine."""
    if 'str_extract' not in case:
        return [f"ERROR: {case_label}: str_extract is required"]
    recorded = case['str_extract']
    if recorded is None:
        group, expected = 0, REGEX_NO_MATCH
    elif isinstance(recorded, dict) and set(recorded) == {'group', 'value'}:
        group = recorded['group']
        expected = recorded['value']
        if type(group) is not int:
            return [f"ERROR: {case_label}: str_extract group must be an int"]
        if expected is not None and not isinstance(expected, str):
            return [
                f"ERROR: {case_label}: str_extract value must be a string "
                f"or null"
            ]
    else:
        return [
            f"ERROR: {case_label}: str_extract must be null or a mapping of "
            f"group and value"
        ]

    try:
        actual = regex_extract(pattern, subject, group)
    except (InvalidRegex, RegexGroupOutOfRange) as exc:
        return [f"ERROR: {case_label}: str_extract group {group}: {exc}"]

    if actual is REGEX_NO_MATCH and expected is REGEX_NO_MATCH:
        return []
    if actual is REGEX_NO_MATCH:
        return [
            f"ERROR: {case_label}: str_extract records a match but the "
            f"portable binding finds none"
        ]
    if expected is REGEX_NO_MATCH:
        return [
            f"ERROR: {case_label}: str_extract records no match but the "
            f"portable binding matches"
        ]
    if actual != expected:
        return [
            f"ERROR: {case_label}: str_extract group {group} records "
            f"{expected!r} but the portable binding produces {actual!r}"
        ]
    return []


def check_yaml_files(root: Path):
    errors = []
    warnings = []
    errors.extend(validate_ascii_sources(root))
    errors.extend(validate_literal_canonical_form(root))
    errors.extend(validate_source_canonical_form(root))
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
    errors.extend(validate_project_configurations(root))
    condition_registry, condition_registry_load_errors = (
        load_condition_registry(root)
    )
    errors.extend(condition_registry_load_errors)
    if condition_registry is not None:
        errors.extend(
            validate_condition_registry(root, condition_registry)
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
    errors.extend(validate_examples_define_documents(root))
    errors.extend(validate_expected_error_contracts(root))
    errors.extend(validate_csv_shapes(root))
    errors.extend(validate_key_column_order(root))
    errors.extend(validate_grammar_contracts(root))
    errors.extend(validate_regex_conformance(root))

    csv_errors, csv_warnings = validate_examples_csv(root, env)
    errors.extend(csv_errors)
    warnings.extend(csv_warnings)
    return errors, warnings


def validate_examples_csv(root: Path, env=None):
    errors = []
    warnings = []
    examples_dir = root / 'benchmarks'
    if not examples_dir.exists():
        return errors, warnings
    if env is None:
        candidate_env, _ = build_schema_env(root)
        env = candidate_env

    profile_checked = set()
    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith('.'):
            continue

        owners = example_artifact_owners(ex_dir)
        for spec_path in example_entry_specs(ex_dir):
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
            primary_path = output.get('path')
            primary_name = (
                PurePosixPath(primary_path).name
                if isinstance(primary_path, str)
                else None
            )
            violation_path = output.get('warning_log')
            violation_name = (
                PurePosixPath(violation_path).name
                if isinstance(violation_path, str)
                else None
            )
            verification_path = output.get('verification_log')
            verification_name = (
                PurePosixPath(verification_path).name
                if isinstance(verification_path, str)
                else None
            )

            expected_dir = ex_dir / 'expected'
            if not expected_dir.exists():
                continue
            for csv_file in sorted(expected_dir.glob('*.csv')):
                # Each golden CSV must be one of the three artifacts the
                # specification names. The logs have their own fixed schemas:
                # R009's for the warning log, REQ-1173's for the verification
                # log.
                if csv_file.name == primary_name:
                    file_columns = expected_cols
                    file_output = output
                    file_spec = spec
                elif csv_file.name == violation_name:
                    file_columns = list(WARNING_LOG_TYPES)
                    file_output = {'path': violation_path}
                    file_spec = {
                        'output': file_output,
                        'columns': [
                            {'name': name, 'type': column_type}
                            for name, column_type in WARNING_LOG_TYPES.items()
                        ],
                    }
                elif csv_file.name == verification_name:
                    file_columns = list(VERIFICATION_LOG_TYPES)
                    file_output = {'path': verification_path}
                    file_spec = {
                        'output': file_output,
                        'columns': [
                            {'name': name, 'type': column_type}
                            for name, column_type in VERIFICATION_LOG_TYPES.items()
                        ],
                    }
                elif owners.get(csv_file.name, set()) - {spec_path.name}:
                    # Another spec of this example produces this golden, and
                    # its own pass checks the header.
                    continue
                else:
                    permitted = [
                        name
                        for name in (primary_name, violation_name, verification_name)
                        if name is not None
                    ]
                    errors.append(
                        f"ERROR: {ex_dir.name}/{csv_file.name}: expected "
                        f"artifact name in {permitted!r}, which output "
                        "declares"
                    )
                    continue
                try:
                    with open(csv_file, 'r', encoding='utf-8') as f:
                        reader = csv.reader(f)
                        header = next(reader)
                        if header != file_columns:
                            errors.append(
                                f"ERROR: {ex_dir.name}/{csv_file.name}: "
                                f"header mismatch for {spec_path.name}. "
                                f"Expected {file_columns}, got {header}"
                            )
                except StopIteration:
                    if file_columns:
                        errors.append(
                            f"ERROR: {ex_dir.name}/{csv_file.name} is "
                            f"empty for {spec_path.name}"
                        )
                except (OSError, UnicodeError, csv.Error):
                    continue

                profile = artifact_profile(file_output)
                if profile == 'csv' and csv_file not in profile_checked:
                    profile_checked.add(csv_file)
                    errors.extend(
                        validate_csv_artifact(
                            csv_file,
                            f"{ex_dir.name}/{csv_file.name}",
                            file_spec,
                        )
                    )

    return errors, warnings


QUALIFIED_REFERENCE = re.compile(r'\b([A-Za-z][A-Za-z0-9_]*)\.([A-Za-z][A-Za-z0-9_]*)\b')


def validate_examples_define_documents(root: Path):
    """Validate each study document against the entry point R026 defines.

    A define document is a separate entry point with its own class, so the
    specification schema never sees it. Nothing else reads these files, and
    a field renamed in the specification language would otherwise reach one
    unnoticed.
    """
    errors = []
    documents = sorted((root / 'benchmarks').glob('*/define.yaml'))
    if not documents:
        return errors

    env, env_errors = build_schema_env(root, entrypoint='schema_define.yaml')
    if env is None:
        return env_errors
    errors.extend(env_errors)

    for path in documents:
        label = str(path.relative_to(root))
        try:
            with open(path, 'r', encoding='utf-8') as handle:
                document = yaml.load(handle, Loader=UniqueKeyLoader)
        except (OSError, UnicodeError, yaml.YAMLError) as exc:
            errors.append(f"ERROR: {label}: {exc}")
            continue
        if not isinstance(document, dict):
            errors.append(f"ERROR: {label}: document must be a mapping")
            continue
        errors.extend(validate_type(document, ['define_class'], env, label))
    return errors


def validate_examples_layout(root: Path):
    errors = []
    examples_dir = root / 'benchmarks'
    if not examples_dir.exists():
        return errors

    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith('.'):
            continue

        rel = ex_dir.relative_to(root)

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
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[3],
                        help="Repository root directory")
    parser.add_argument('--warnings-as-errors', action='store_true',
                        help="Treat warnings as errors")
    args = parser.parse_args()

    try:
        require_regex_binding()
    except RegexBindingUnavailable as exc:
        print(f"ERROR: {exc}")
        return 1

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
