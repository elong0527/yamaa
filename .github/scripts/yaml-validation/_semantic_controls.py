"""Test-only authored metadata adapters to the retained Rust compiler/binder.

The controls author complete model fixtures around the historical partial inputs.
Python assembles documents and represents canonical observations; Rust owns every
name, type, operation, phase and diagnostic decision. No execution is requested.
"""
import copy
import csv
import json
from functools import lru_cache
from pathlib import Path

import _typed_fixtures
import _native_schema


class Finding(str):
    def __new__(cls, row, label):
        path = label + ('.' + row['spec_paths'][0] if row['spec_paths'][0] else '')
        value = super().__new__(cls, f"ERROR: {path}: {row['condition']}: "
            + json.dumps(row['context'], sort_keys=True))
        value.path, value.condition = path, row['condition']
        value.context, value.requirement = row['context'], row['requirement']
        value.phase = row['phase']
        value.span = tuple(row['span']) if row.get('span') is not None else None
        value.message = str(value)
        return value


def findings(observation, label='spec'):
    return [Finding({**row, 'spec_paths': [path]}, label)
        for row in observation['issues'] for path in row['spec_paths']]


def complete(specification, sources=None, *, fill_derivations=True):
    """Author only omitted scaffold metadata, retaining every written operand."""
    spec = copy.deepcopy(specification)
    spec.setdefault('schema_version', '1.0')
    spec.setdefault('domain', 'TEST')
    spec.setdefault('input', {'SRC': {'path': 'source.csv'}})
    sources = copy.deepcopy(sources or {})
    for dataset, source in spec['input'].items():
        sources.setdefault(dataset, {'ID': 'str'})
        if isinstance(source, dict):
            sources[dataset].update(source.get('types') or {})
    if 'columns' not in spec:
        spec['columns'] = [{'name': 'ID', 'type': 'str', 'derivation': {'literal': 'fixture'}}]
    for column in spec['columns']:
        column.setdefault('type', 'str')
        if 'derivation' not in column and fill_derivations and not spec.get('rows'):
            column['derivation'] = {'literal': None}
    spec.setdefault('keys', [spec['columns'][0]['name']] if spec['columns'] else [])
    spec.setdefault('output', {})
    spec['output'].setdefault('path', 'result.csv')
    spec['output'].setdefault('columns', [column['name'] for column in spec['columns']])
    for row in spec.get('rows', []):
        row.setdefault('derivations', {})
    return spec, sources


def observation(specification, sources=None, *, fill_derivations=True, bind=True,
        output=False):
    spec, sources = complete(specification, sources,
        fill_derivations=fill_derivations)
    source = _native_schema.bytes_for(spec)
    admission = _schema().validate(source, ['root_class'], False)
    if admission:
        return {'issues': [{'phase': phase, 'condition': condition,
            'requirement': requirement, 'spec_paths': paths,
            'context': json.loads(context), 'span': None}
            for phase, condition, requirement, paths, context in admission],
            'driver': None}
    spec = _native_schema.decode_document(_schema().normalize(
        source, ['root_class'], True))
    return _typed_fixtures.observe('semantic', json.dumps({
        'specification': spec, 'sources': sources, 'bind': bind, 'output': output,
        'source': source.decode('ascii'),
        'schema_modules': [(name, raw.decode('utf-8')) for name, raw in _modules()]}))


def declarations(specification, label='spec', *, fill_derivations=True, output=False):
    """Observe complete native declaration rows without source binding authority."""
    return findings(observation(specification,
        fill_derivations=fill_derivations, bind=False, output=output), label)


def driver(specification):
    """Return the held model's driver selection fact without Python inference."""
    return observation(specification, bind=False)['driver']


@lru_cache(maxsize=1)
def _schema():
    return _native_schema.capture(Path(__file__).resolve().parents[3] / 'yaml')


@lru_cache(maxsize=1)
def _modules():
    return _native_schema.capture_modules(
        Path(__file__).resolve().parents[3] / 'yaml', 'schema.yaml')


def document(specification, label='spec', path=None, _environment=None, *, sources=None,
        fill_derivations=True):
    if sources is None and path is not None:
        sources = {}
        for dataset, source in specification.get('input', {}).items():
            written = source if isinstance(source, str) else source['path']
            source_path = Path(path).parent / written
            if source_path.exists() and source_path.suffix == '.csv':
                with source_path.open(newline='') as stream:
                    sources[dataset] = {name: 'str' for name in next(csv.reader(stream))}
    return findings(observation(specification, sources,
        fill_derivations=fill_derivations), label)


def catalog(unqualified=None, qualified=None):
    return {'unqualified': unqualified or {}, 'qualified': qualified or {}}


class AuthoredAst(dict):
    def __init__(self, ast, text, names):
        super().__init__(ast)
        self.text, self.names = text, names


class SyntaxError(ValueError):
    pass


def parsed(contract, text):
    result = syntax(contract, text)
    if result['status'] == 'invalid':
        raise SyntaxError(result)
    if result['status'] != 'parsed':
        raise RuntimeError(f'typed {contract} fixture refused: {result!r}')
    return AuthoredAst(result['ast'], text, result['identifiers'])


def predicate_syntax(text):
    return parsed('predicate', text)


def numeric_syntax(text):
    return parsed('numeric', text)


def aggregate_syntax(text):
    return parsed('aggregate', text)


def result_type(contract, expression, path):
    return _typed_fixtures.observe('semantic', json.dumps({
        'mode': contract + '-type', 'expression': expression, 'path': path}))['result_type']


def numeric_names(text):
    return identifiers('numeric', text)


def aggregate_names(text):
    return identifiers('aggregate', text)


def template_names(text):
    return identifiers('string-template', text)


def template_placeholders(text):
    return [part for part in template_parts(text) if part['kind'] == 'placeholder']


def predicate_ast(ast, types):
    # Resolve fixture metadata only; semantic decisions remain in the binder.
    if callable(types):
        types = {name: types(name) for name in ast.names if types(name) is not None}
    return predicate(ast.text, 'spec.predicate', catalog(types))


def template_parts(text):
    result = syntax('string-template', text)
    if result['status'] == 'invalid':
        raise SyntaxError(result)
    if result['status'] != 'parsed':
        raise RuntimeError(f'typed string-template fixture refused: {result!r}')
    return result['parts']


def intermediate(specification, label, datasets, output_types):
    datasets = copy.deepcopy(datasets)
    for fields in datasets.values():
        fields.setdefault('CONTROL_KEY', 'str')
    spec = {**specification, 'input': {name: {'path': name + '.csv'} for name in datasets},
        'base': next(iter(datasets)), 'keys': ['CONTROL_KEY'], 'columns': [
            {'name': 'CONTROL_KEY', 'type': 'str', 'derivation': {'literal': 'fixture'}},
            *[
            {'name': name, 'type': kind, 'derivation': {'literal': None}}
            for name, kind in output_types.items()]]}
    return document(spec, label, sources=datasets)


def operation(expression, path, context):
    types = context.get('output_types', context.get('resolver', {}).get('unqualified', {}))
    datasets = copy.deepcopy(context.get('input', {}))
    datasets.setdefault('SRC', {'ID': 'str'})
    columns = [{'name': name, 'type': kind, 'derivation': {'literal': None}}
        for name, kind in types.items()]
    keys = context.get('keys') or [columns[0]['name'] if columns else 'ID']
    for key in keys:
        if not any(column['name'] == key for column in columns):
            kind = next((fields[key] for fields in datasets.values() if key in fields), 'str')
            columns.append({'name': key, 'type': kind, 'derivation': {'literal': None}})
        for fields in datasets.values():
            fields.setdefault(key, next(column['type'] for column in columns if column['name'] == key))
    target = path.split('.columns.', 1)[1].split('.', 1)[0] if '.columns.' in path else 'RESULT'
    columns = [column for column in columns if column['name'] != target]
    columns.append({'name': target, 'type': 'float', 'derivation': expression})
    spec = {'domain': 'TEST', 'input': {name: {'path': name + '.csv',
            'types': {field: kind for field, kind in fields.items() if kind != 'str'}}
            for name, fields in datasets.items()},
        'base': 'SRC', 'columns': columns, 'keys': keys}
    if context.get('intermediates'):
        spec['intermediates'] = copy.deepcopy(context['intermediates'])
    actual = 'columns.' + target + '.derivation'
    label = path.split('.columns.', 1)[0] if '.columns.' in path else 'spec'
    if context.get('kind') in {'ungrouped_row', 'grouped_row'}:
        row = {'id': 'control', 'dataset': context['driver'],
            'derivations': {column['name']: column.pop('derivation') for column in columns}}
        if context.get('row_group_by') is not None:
            row['group_by'] = context['row_group_by']
        spec['rows'] = [row]
        actual = 'rows[0].derivations.' + target
    result = document(spec, label, sources=datasets)
    errors = [error for error in result if error.path.startswith(label + '.' + actual)]
    unrelated = [error for error in result if not error.path.startswith(label + '.' + actual)]
    if unrelated:
        raise RuntimeError('native semantic fixture scaffold refused: ' + '\n'.join(unrelated))
    if context.get('kind') in {'ungrouped_row', 'grouped_row'}:
        for error in errors:
            error.path = path + error.path.removeprefix(label + '.' + actual)
    return errors


def predicate(text, path, types):
    context = {'output_types': types.get('unqualified', {}),
        'input': types.get('qualified', {})}
    errors = operation({'case': [{'when': text, 'then': {'literal': 1}}]},
        'spec.columns.RESULT.derivation', context)
    return [Finding({
        'phase': error.phase, 'condition': error.condition,
        'requirement': error.requirement, 'context': error.context,
        'span': error.span, 'spec_paths': ['']}, path) for error in errors]


def numeric(text, path, types):
    inputs, intermediates = {}, []
    for name, fields in types.get('qualified', {}).items():
        donor = 'CONTROL_' + name
        inputs[donor] = {**fields, 'CONTROL_KEY': 'str'}
        intermediates.append({'id': name, 'dataset': donor, 'key': ['CONTROL_KEY'],
            'order_by': [donor + '.CONTROL_KEY'], 'keep': 'first'})
    errors = operation({'compute': {'expr': text}},
        'spec.columns.RESULT.derivation', {
            'output_types': types.get('unqualified', {}),
            'input': inputs, 'keys': ['CONTROL_KEY'], 'intermediates': intermediates})
    errors = [Finding({
        'phase': error.phase, 'condition': error.condition,
        'requirement': error.requirement, 'context': error.context,
        'span': error.span, 'spec_paths': ['']}, path) for error in errors]
    return _syntax_geometry('numeric', text, errors)


def aggregate(payload, path, context):
    if isinstance(payload, dict):
        admission = _schema().validate(_native_schema.bytes_for(payload),
            ['aggregate_class'], False)
        if admission:
            return findings({'issues': [{'phase': phase, 'condition': condition,
                'requirement': requirement, 'spec_paths': paths,
                'context': json.loads(context), 'span': None}
                for phase, condition, requirement, paths, context in admission]}, path)
    context = {**context.get('aggregate', {}), **context}
    errors = operation({'aggregate': payload}, path.rsplit('.aggregate', 1)[0], context)
    text = payload if isinstance(payload, str) else payload.get('expr', '')
    return _syntax_geometry('aggregate', text, errors)


def template(text, path, types):
    if callable(types):
        result = syntax('string-template', text)
        types = catalog({name: types(name) for name in result.get('identifiers', [])
            if types(name) is not None})
    errors = operation({'str_template': text}, 'spec.columns.RESULT.derivation', {
        'output_types': types.get('unqualified', {}),
        'input': types.get('qualified', {})})
    errors = [Finding({'phase': error.phase, 'condition': error.condition,
        'requirement': error.requirement, 'context': error.context,
        'span': error.span, 'spec_paths': ['']}, path) for error in errors]
    return _syntax_geometry('string-template', text, errors)


def _syntax_geometry(contract, text, errors):
    outcome = syntax(contract, text)
    if outcome['status'] not in {'parsed', 'invalid'}:
        raise RuntimeError(f'typed {contract} fixture refused: {outcome!r}')
    if outcome['status'] == 'invalid':
        start = outcome['position']['character']
        span = outcome.get('span', [start, min(start + 1, len(text))])
        if isinstance(span, dict):
            span = [span['character_start'], span['character_end']]
        for error in errors:
            if error.condition == outcome['condition']:
                error.span = tuple(span)
    return errors


def syntax(contract, text):
    return _typed_fixtures.spans(_typed_fixtures.observe(contract, text))


def identifiers(contract, text):
    result = syntax(contract, text)
    if result['status'] == 'invalid':
        raise ValueError(result)
    if result['status'] != 'parsed':
        raise RuntimeError(f'typed {contract} fixture refused: {result!r}')
    return set(result['identifiers'])
