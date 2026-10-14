"""Rust syntax observations and authored grammar vector/editorial checks."""
import re
from pathlib import Path, PurePosixPath
import yaml
import _typed_fixtures
from _repository_yaml import UniqueKeyLoader
from validation_diagnostics import validation_diagnostic


class PredicateSemanticIssue(str):
    def __new__(cls, message, condition, context, span):
        value = super().__new__(cls, message)
        value.condition = condition
        value.context = dict(context)
        value.span = tuple(span)
        return value


PREDICATE_COMPARISON_OPERATORS = ("=", "<>", "<", "<=", ">", ">=")


PREDICATE_RESERVED_NAMES = frozenset({
    "AND", "BETWEEN", "DATE", "DATETIME", "ESCAPE", "FALSE", "IN", "IS",
    "LIKE", "NOT", "NULL", "OR", "TRUE",
})


class PredicateError(ValueError):
    condition = "invalid_predicate"

    def __init__(self, message, position, requirement="REQ-0188"):
        self.position = position
        self.requirement = requirement
        super().__init__(f"{message} at character {position + 1}")


def parse_predicate(text):
    if not isinstance(text, str) or not text:
        raise PredicateError("predicate must be a non-empty string", 0)
    outcome = _typed_fixtures.observe("predicate", text)
    if outcome["status"] == "parsed":
        return outcome["ast"]
    if outcome["status"] == "invalid":
        raise PredicateError(
            outcome["message"], outcome["position"]["character"],
            outcome["requirement"],
        )
    raise RuntimeError(f"typed predicate fixture refused: {outcome!r}")


def _ensure_predicate_binding():
    """The development fixture is resolved only when an assertion needs it."""


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
    'ROUND_HALF_AWAY_FROM_ZERO': (2, 2),
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


def _expression_fixture(contract, text, error_type):
    if not isinstance(text, str) or not text:
        raise error_type(f"{contract} expression must be a non-empty string", 0, 0)
    outcome = _typed_fixtures.observe(contract, text)
    if outcome["status"] == "parsed":
        return _typed_fixtures.spans(outcome["ast"])
    if outcome["status"] != "invalid":
        raise RuntimeError(f"typed {contract} fixture refused: {outcome!r}")
    context = outcome["context"]
    start = outcome["position"]["character"]
    end = start + len(context["function"]) if "function" in context else min(start + 1, len(text))
    start, end = outcome.get("span", [start, end])
    raise error_type(
        outcome["condition"].replace("_", " "), start, end,
        condition=outcome["condition"], context=context,
    )


def parse_numeric_expression(text):
    return _expression_fixture("numeric", text, NumericExpressionError)


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


def parse_aggregate_expression(text):
    return _expression_fixture("aggregate", text, AggregateExpressionError)


def predicate_identifier_names(text):
    return _native_identifiers('predicate', text)


def _native_identifiers(contract, text):
    if not isinstance(text, str):
        return set()
    outcome = _typed_fixtures.observe(contract, text)
    if outcome['status'] == 'parsed':
        return set(outcome['identifiers'])
    if outcome['status'] == 'invalid':
        return set()
    raise RuntimeError(f'typed {contract} fixture refused: {outcome!r}')


def numeric_expression_identifier_names(text):
    return _native_identifiers('numeric', text)


def aggregate_expression_identifier_names(text):
    return _native_identifiers('aggregate', text)


class StringTemplateError(ValueError):
    def __init__(self, message, start, end, reason, placeholder=None):
        self.message = message
        self.span = (start, end)
        self.reason = reason
        self.placeholder = placeholder
        super().__init__(f"{message} at characters [{start}, {end})")


def parse_string_template(text):
    if not isinstance(text, str):
        raise StringTemplateError(
            "string template must be a string", 0, 0, "invalid_template"
        )
    outcome = _typed_fixtures.observe("string-template", text)
    if outcome["status"] == "parsed":
        return _typed_fixtures.spans(outcome["parts"])
    if outcome["status"] != "invalid":
        raise RuntimeError(f"typed template fixture refused: {outcome!r}")
    facts = outcome["context"]
    span = outcome["span"]
    reason = facts["reason"]
    placeholder = facts.get("placeholder")
    raise StringTemplateError(
        f"invalid placeholder {placeholder!r}" if placeholder is not None else "unmatched brace",
        span["character_start"], span["character_end"], reason, placeholder,
    )


def string_template_placeholders(text):
    """Return only the placeholder parts of a parsed template."""
    return [
        part
        for part in parse_string_template(text)
        if part['kind'] == 'placeholder'
    ]


def string_template_identifier_names(text):
    return _native_identifiers('string-template', text)


def expand_spec_windows(spec, env, strict=True, provenance=None, label=""):
    """Replay named-window fixture truth through the shipped typed schema."""
    outcome = _typed_fixtures.observe(
        "windows", yaml.safe_dump(spec, allow_unicode=False), strict=strict
    )
    if outcome["status"] == "expanded":
        if provenance is not None:
            updates = dict(provenance)
            for logical, definition in outcome["references"]:
                prefix = f"windows.{definition}"
                for source_path, origin in list(updates.items()):
                    if source_path.startswith(prefix + "."):
                        updates[logical + source_path[len(prefix):]] = origin
            provenance.update(updates)
        return outcome["document"], []
    if outcome["status"] == "invalid":
        return spec, [
            validation_diagnostic(
                f"{label}.{item['path']}" if label else item['path'],
                item['condition'], item['condition'], context=item['context'],
            )
            for item in outcome["diagnostics"]
        ]
    raise RuntimeError(f"typed window fixture refused: {outcome!r}")


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
    return None, predicate_shape(ast), predicate_identifier_names(text)


def decide_numeric(text):
    try:
        ast = parse_numeric_expression(text)
    except NumericExpressionError as exc:
        return exc.condition, None, set()
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
