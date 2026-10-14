"""Repository IO and lossless codecs around installed shared schema facts.

This module captures an explicit schema closure. Rust owns descriptor admission,
type validation, defaults and normalization; no reference interpreter is used.
"""

import json
from pathlib import Path

import yaml
from yamaa._native import _capture_schema_metadata

from editorial import UniqueKeyLoader

ROOT_CLASSES = {
    'schema.yaml': 'root_class',
    'schema_environment.yaml': 'environment_class',
    'schema_define.yaml': 'define_class',
}


def decode_document(encoded):
    """Decode tagged native values without interpreting their schema or meaning."""
    arena = json.loads(encoded)
    nodes = arena['nodes']

    def value(index):
        node = nodes[index]
        kind = node[0]
        if kind == 'null':
            return None
        if kind == 'int':
            return int(node[1])
        if kind in {'str', 'float', 'bool'}:
            return node[1]
        if kind == 'list':
            return [value(child) for child in node[1]]
        if kind == 'dict':
            return {value(key): value(child) for key, child in node[1]}
        raise RuntimeError(f'unknown native metadata scalar kind: {kind!r}')

    return value(arena['root'])


def capture(directory, entrypoint='schema.yaml'):
    directory = Path(directory)
    pending, captured = [entrypoint], {}
    while pending:
        name = pending.pop(0)
        if name in captured:
            continue
        path = directory / name
        try:
            source = path.read_bytes()
        except OSError:
            if name == entrypoint:
                raise
            continue
        captured[name] = source
        # Reading the authored include list is IO discovery. Admission of its
        # shape, identities, cycles and semantics is the native capability's job.
        document = yaml.load(source, Loader=UniqueKeyLoader)
        includes = document.get('includes', []) if isinstance(document, dict) else []
        if isinstance(includes, list):
            for included in includes:
                if isinstance(included, str):
                    # An invalid include is retained for native admission but
                    # never grants the discovery pass read authority elsewhere.
                    relative = Path(included)
                    if not relative.is_absolute() and '..' not in relative.parts:
                        pending.append(included)
    return _capture_schema_metadata(
        list(captured.items()), 0, ROOT_CLASSES.get(entrypoint, 'root_class')
    )


def schema_errors(error, label):
    try:
        defects = json.loads(str(error))['schema_defects']
    except (ValueError, TypeError, KeyError):
        return [f'ERROR: {label}: {error}']
    return [f"ERROR: {label}.module[{item['module']}].{item['path']}: {item['kind']}"
            for item in defects]


def fixture(environment):
    """Capture the schema declarations an independent partial control authors."""
    if 'native' in environment:
        return environment['native']
    authored = {
        'version': environment.get('version', '1.0'),
        'root_class': [],
        **environment.get('classes', {}),
        **environment.get('aliases', {}),
        **environment.get('registries', {}),
    }
    return _capture_schema_metadata(
        [('schema.yaml', yaml.safe_dump(authored).encode('utf-8'))],
        0, 'root_class',
    )


def bytes_for(value):
    return yaml.safe_dump(value, allow_unicode=True).encode('utf-8')


def failure_rows(error):
    """Only a complete native semantic response can be a language finding."""
    try:
        observed = json.loads(str(error))['outcome']
    except (ValueError, TypeError, KeyError):
        raise error
    if observed.get('status') != 'invalid':
        raise error
    return [
        (row['phase'], row['condition'], row['requirement'],
         row['spec_paths'], json.dumps(row['context']))
        for row in observed['diagnostics']
    ]
