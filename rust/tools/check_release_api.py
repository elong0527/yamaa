"""Reconcile declared exports with explicit migration dispositions; import no host code."""
from __future__ import annotations

import argparse
import ast
import json
import re
from pathlib import Path


def discover(root: Path) -> dict[str, list[str]]:
    sources = {}
    for path in sorted((root / 'python/src/yamaa').rglob('*.py')):
        declarations = []
        seen = False
        tree = ast.parse(path.read_text(encoding="utf-8"))
        targets_seen = set()
        for node in tree.body:
            targets = node.targets if isinstance(node, ast.Assign) else [node.target] if isinstance(node, (ast.AnnAssign, ast.AugAssign)) else []
            if not any(isinstance(t, ast.Name) and t.id == '__all__' for t in targets):
                continue
            if seen or isinstance(node, ast.AugAssign) or len(targets) != 1:
                raise ValueError(f'nonliteral or repeated __all__: {path}')
            seen = True
            targets_seen.add(id(targets[0]))
            try:
                values = ast.literal_eval(node.value)
            except (ValueError, TypeError) as error:
                raise ValueError(f'nonliteral __all__: {path}') from error
            if not isinstance(values, (list, tuple)) or not all(isinstance(x, str) for x in values) or len(set(values)) != len(values):
                raise ValueError(f'invalid __all__: {path}')
            declarations = list(values)
        if any(isinstance(node, ast.Name) and node.id == "__all__" and id(node) not in targets_seen for node in ast.walk(tree)):
            raise ValueError(f"dynamic __all__: {path}")
        if seen:
            sources[path.relative_to(root).as_posix()] = sorted(declarations)
    path = root / 'rust/crates/yamaa-python/yamaa_native.pyi'
    if path.is_file():
        declarations = []
        tree = ast.parse(path.read_text(encoding="utf-8"))
        for node in tree.body:
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                declarations.append(node.name)
                if isinstance(node, ast.ClassDef):
                    declarations.extend(node.name + '.' + member.name for member in node.body if isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef)))
        sources[path.relative_to(root).as_posix()] = sorted(declarations)
    for path in sorted((root / 'R').glob('*/NAMESPACE')):
        declarations = []
        for line in path.read_text(encoding='utf-8').splitlines():
            line = line.strip()
            if not line.startswith(('export', 'S3method')):
                continue
            match = re.fullmatch(r'export\(([A-Za-z0-9_.]+)\)', line)
            if match:
                declarations.append(match[1])
                continue
            match = re.fullmatch(r'S3method\(([A-Za-z0-9_.]+),\s*([A-Za-z0-9_.]+)\)', line)
            if match:
                declarations.append('S3:' + match[1] + '.' + match[2])
                continue
            raise ValueError(f'unsupported namespace declaration: {path}: {line}')
        sources[path.relative_to(root).as_posix()] = sorted(declarations)
    return sources


def check(root: Path, inventory: dict) -> None:
    if inventory.get('version') != 1 or not isinstance(inventory.get('sources'), dict):
        raise ValueError('invalid release API inventory')
    actual = discover(root)
    registered = inventory['sources']
    if set(actual) != set(registered):
        raise ValueError(f'API sources differ: unregistered={sorted(set(actual)-set(registered))}, stale={sorted(set(registered)-set(actual))}')
    allowed = {'replace', 'remove_public', 'retain_candidate', 'decision_required', 'internal'}
    for source, declarations in actual.items():
        record = registered[source]
        names = record.get('names')
        if not isinstance(names, list) or len(names) != len(set(names)) or sorted(names) != declarations:
            raise ValueError(f'API declarations differ: {source}')
        if record.get('disposition') not in allowed or not record.get('issue') or not record.get('rationale'):
            raise ValueError(f'missing API disposition: {source}')
        for name, override in record.get('overrides', {}).items():
            if name not in names or override.get('disposition') not in allowed or not override.get('rationale'):
                raise ValueError(f'invalid API override: {source}: {name}')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    inventory = json.loads((args.root / 'rust/planning/release-api.json').read_text())
    check(args.root, inventory)
    print('PASS: declared API names and migration dispositions reconcile.')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
