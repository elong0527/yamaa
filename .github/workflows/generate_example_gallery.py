#!/usr/bin/env python3
"""Generate every example-gallery surface from `yaml/examples/`.

The example directories are the single source of truth. A directory name
decides where an example is listed, its README title supplies the description,
and a negative example's `expected/error.yaml` supplies the phase that rejects
it. Nothing is typed twice, so adding an example is one command:

    python3 .github/workflows/generate_example_gallery.py

Generated surfaces:

- `yaml/examples/<name>/README.md` -- a back-link to the example index, placed
  directly under the title;
- `yaml/examples/README.md` -- the index, split into positive and negative
  sections;
- `docs/examples.md` -- the published gallery, positive examples by standard
  and negative examples by the phase that rejects them;
- `docs/yaml-examples-walkthrough.md` -- the suite counts;
- every `docs/*.md` page -- the navigation bar that leads back to the docs
  index.

Each generated region is delimited by BEGIN/END markers and rewritten whole,
so the same tree always produces the same bytes. `--check` rewrites nothing
and exits non-zero when a file is out of date, which is what CI runs.
"""

import argparse
import re
import sys
import textwrap
from pathlib import Path

REPO_TREE = 'https://github.com/elong0527/yamaa/tree/main'
EXAMPLES_TREE = f'{REPO_TREE}/yaml/examples'

BACKLINK = '[Back to the example index](../README.md)'

BEGIN = '<!-- BEGIN GENERATED: {key} -->'
END = '<!-- END GENERATED: {key} -->'

# Positive examples are grouped by the standard their directory name names.
# The blurb is written to read under both `ADaM` and `adam-*`.
STANDARDS = [
    ('adam', 'ADaM', 'analysis datasets derived from collected data'),
    ('sdtm', 'SDTM', 'tabulation datasets built from collected data'),
    ('odm', 'ODM', 'how a collected item resolves to a value'),
]
NEGATIVE_BLURB = 'specifications the design must reject, with the exact error'

# Negative examples are grouped by `expected/error.yaml` `phase`. The order and
# the wording come from the two closed tables in `yaml/examples/agents.md`;
# whole-run phases first, then the stages an operation answers locally.
PHASES = [
    ('validation', 'the specification itself, before any data is read'),
    ('ingest', 'a stored value, against the type its field carries'),
    ('row_construction', 'evaluating a row template'),
    ('derivation', "evaluating a column's expression over a row"),
    ('output', 'output identity, once every column holds its final value'),
    ('verification', 'a declared assertion'),
    ('bind', 'an absent source variable or ODM item'),
    ('join', 'an unresolved multiple match'),
    ('mapping', 'a missing or unmapped lookup input'),
    ('cut', 'a missing numeric classification input'),
    ('extract', 'a missing string or unmatched pattern'),
    ('template', 'a missing placeholder value'),
    ('impute', 'a missing or unusable partial-date input'),
    ('convert', 'a result that cannot take its declared type'),
    ('final', 'a failed final replacement'),
]

DOCS_PAGES = [
    ('index.md', 'Index'),
    ('why-yamaa.md', 'Why'),
    ('excel-to-yamaa.md', 'Excel to YAMAA'),
    ('schema-concepts.md', 'Schema concepts'),
    ('yaml-examples-walkthrough.md', 'Examples walkthrough'),
    ('examples.md', 'Example gallery'),
]

PHASE_RE = re.compile(r'^phase:\s*([A-Za-z_]+)\s*$', re.MULTILINE)


class GalleryError(Exception):
    """A source file does not meet the contract the generator relies on."""


class Example:
    def __init__(self, path):
        self.path = path
        self.name = path.name
        self.negative = self.name.startswith('negative-')
        self.standard = 'negative' if self.negative else self.name.split('-')[0]
        self.description = read_description(path / 'README.md')
        self.phase = read_phase(path) if self.negative else None


def read_description(readme):
    """Return the part of the README title after the standard and domain."""
    if not readme.is_file():
        raise GalleryError(f'{readme}: missing README.md')
    title = readme.read_text(encoding='utf-8').split('\n', 1)[0]
    if not title.startswith('# '):
        raise GalleryError(f'{readme}: first line must be the "# " title')
    if ':' not in title:
        raise GalleryError(
            f'{readme}: title must read "# <STANDARD> <DOMAIN>: <what it does>"'
        )
    return title.split(':', 1)[1].strip()


def read_phase(path):
    error_file = path / 'expected' / 'error.yaml'
    if not error_file.is_file():
        raise GalleryError(f'{path.name}: negative example has no expected/error.yaml')
    match = PHASE_RE.search(error_file.read_text(encoding='utf-8'))
    if not match:
        raise GalleryError(f'{error_file}: no "phase:" field')
    phase = match.group(1)
    if phase not in dict(PHASES):
        raise GalleryError(
            f'{error_file}: phase "{phase}" is not one of the phases listed in '
            'yaml/examples/agents.md; add it to PHASES in this script first'
        )
    return phase


def load_examples(root):
    examples_dir = root / 'yaml' / 'examples'
    if not examples_dir.is_dir():
        raise GalleryError(f'{examples_dir}: not a directory')
    examples = [
        Example(path)
        for path in sorted(examples_dir.iterdir())
        if path.is_dir() and not path.name.startswith('.')
    ]
    if not examples:
        raise GalleryError(f'{examples_dir}: holds no examples')
    unknown = sorted(
        {e.standard for e in examples if e.standard != 'negative'}
        - {key for key, _, _ in STANDARDS}
    )
    if unknown:
        raise GalleryError(
            'unrecognised standard prefix(es): ' + ', '.join(unknown)
            + '; add them to STANDARDS in this script first'
        )
    return examples


def replace_region(text, key, body, label):
    """Rewrite the marked region named `key` with `body`."""
    begin = BEGIN.format(key=key)
    end = END.format(key=key)
    start = text.find(begin)
    stop = text.find(end)
    if start < 0 or stop < 0 or stop < start:
        raise GalleryError(f'{label}: missing "{begin}" / "{end}" markers')
    return text[:start] + begin + '\n' + body + text[stop:]


def plural(count, noun):
    return f'{count} {noun}' + ('' if count == 1 else 's')


def prose(text):
    """Wrap one paragraph at the 79 columns this repository writes to."""
    return textwrap.fill(' '.join(text.split()), width=79).split('\n')


def index_rows(examples, link, heading='Derives'):
    rows = [f'| Example | {heading} |', '|---|---|']
    rows += [
        f'| [`{e.name}`]({link(e)}) | {e.description} |' for e in examples
    ]
    return rows


def render_example_index(examples):
    """The index that `yaml/examples/README.md` publishes."""
    positive = [e for e in examples if not e.negative]
    negative = [e for e in examples if e.negative]

    lines = ['### Positive examples', '']
    lines += prose(
        f'{plural(len(positive), "example")} that must run and produce the '
        'artifact its `expected/` directory holds.'
    )
    lines += ['']
    lines += index_rows(positive, lambda e: f'{e.name}/')

    lines += ['', '### Negative examples', '']
    lines += prose(
        f'{plural(len(negative), "example")} that must fail, each with the '
        'exact error in `expected/error.yaml` and a `How to fix` section in '
        'its README.'
    )
    lines += ['']
    lines += index_rows(negative, lambda e: f'{e.name}/', heading='Rejects')
    return '\n'.join(lines) + '\n'


def render_gallery(examples):
    """The gallery that `docs/examples.md` publishes."""
    positive = [e for e in examples if not e.negative]
    negative = [e for e in examples if e.negative]

    def tree(example):
        return f'{EXAMPLES_TREE}/{example.name}'

    lines = prose(
        f'The suite holds **{plural(len(examples), "example")}**: '
        f'{len(positive)} that must run, and {len(negative)} that must be '
        'rejected.'
    )
    lines += ['', '| Group | Count | What it is |', '|---|---|---|']
    for key, label, blurb in STANDARDS:
        count = len([e for e in positive if e.standard == key])
        lines.append(f'| [{label}](#{label.lower()}) | {count} | {blurb} |')
    lines.append(
        f'| [Rejected](#negative-examples) | {len(negative)} | '
        f'{NEGATIVE_BLURB} |'
    )

    lines += ['', '## Positive examples', '']
    lines += prose(
        'Every one of these runs to completion and produces the artifact its '
        '`expected/` directory holds, byte for byte.'
    )
    for key, label, _ in STANDARDS:
        group = [e for e in positive if e.standard == key]
        if not group:
            continue
        lines += ['', f'### {label}', '']
        lines += index_rows(group, tree)

    lines += ['', '## Negative examples', '']
    lines += prose(
        'Each of these must fail, and `expected/error.yaml` pins exactly how. '
        'They are grouped by the phase that rejects the run, so an '
        'implementation can close one phase at a time.'
    )
    for phase, blurb in PHASES:
        group = [e for e in negative if e.phase == phase]
        if not group:
            continue
        lines += ['', f'### Rejected at `{phase}`', '']
        lines += prose(f'{plural(len(group), "example")}, rejecting {blurb}.')
        lines += ['']
        lines += index_rows(group, tree, heading='Rejects')

    return '\n'.join(lines) + '\n'


def render_counts(examples):
    """The suite counts that the walkthrough quotes."""
    positive = [e for e in examples if not e.negative]
    negative = [e for e in examples if e.negative]
    lines = prose(
        f'`yaml/examples/` holds **{len(examples)} directories**. Each one is '
        'a complete, runnable specification with its input data and the exact '
        'output an implementation must reproduce:'
    )
    lines += ['', '| Group | Count | What it is |', '|---|---|---|']
    for key, _, blurb in STANDARDS:
        count = len([e for e in positive if e.standard == key])
        lines.append(f'| `{key}-*` | {count} | {blurb} |')
    lines.append(f'| `negative-*` | {len(negative)} | {NEGATIVE_BLURB} |')
    lines += ['']
    lines += prose(
        'The complete list, with every example linked, is the '
        '[example gallery](examples.md).'
    )
    return '\n'.join(lines) + '\n'


def render_nav(page):
    """The navigation bar, with the current page left unlinked."""
    parts = []
    for name, label in DOCS_PAGES:
        parts.append(f'**{label}**' if name == page else f'[{label}]({name})')
    return '> **YAMAA docs:** ' + ' | '.join(parts) + '\n'


def build(root):
    """Return {path: new content} for every generated surface."""
    examples = load_examples(root)
    updates = {}

    for example in examples:
        readme = example.path / 'README.md'
        updates[readme] = apply_backlink(
            readme.read_text(encoding='utf-8'), readme
        )

    def region(relative, key, body):
        path = root / relative
        if not path.is_file():
            raise GalleryError(f'{relative}: missing')
        text = updates.get(path, path.read_text(encoding='utf-8'))
        updates[path] = replace_region(text, key, body, relative)

    region(
        'yaml/examples/README.md', 'example-index',
        render_example_index(examples),
    )
    region('docs/examples.md', 'example-gallery', render_gallery(examples))
    region(
        'docs/yaml-examples-walkthrough.md', 'example-counts',
        render_counts(examples),
    )
    for name, _ in DOCS_PAGES:
        if name == 'index.md':
            continue
        region(f'docs/{name}', 'docs-nav', render_nav(name))

    return updates


def apply_backlink(text, label):
    """Put the index back-link directly under the title, exactly once."""
    lines = text.split('\n')
    if not lines or not lines[0].startswith('# '):
        raise GalleryError(f'{label}: first line must be the "# " title')
    body = [line for line in lines[1:] if line.strip() != BACKLINK]
    while body and not body[0].strip():
        body.pop(0)
    if not body:
        body = ['']
    return '\n'.join([lines[0], '', BACKLINK, ''] + body)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split('\n', 1)[0])
    parser.add_argument(
        '--root', type=Path, default=Path(__file__).parent.parent.parent,
        help='repository root directory',
    )
    parser.add_argument(
        '--check', action='store_true',
        help='report out-of-date files instead of rewriting them',
    )
    args = parser.parse_args(argv)

    try:
        updates = build(args.root)
    except GalleryError as error:
        print(f'ERROR: {error}')
        return 1

    stale = sorted(
        path for path, content in updates.items()
        if path.read_text(encoding='utf-8') != content
    )

    if args.check:
        if stale:
            for path in stale:
                print(f'ERROR: {path.relative_to(args.root)} is out of date')
            print(
                'Run: python3 .github/workflows/generate_example_gallery.py'
            )
            return 1
        print(f'PASS: example gallery is up to date ({len(updates)} files).')
        return 0

    for path in stale:
        path.write_text(updates[path], encoding='utf-8')
        print(f'wrote {path.relative_to(args.root)}')
    print(f'{len(stale)} file(s) updated, {len(updates)} checked.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
