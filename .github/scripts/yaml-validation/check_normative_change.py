#!/usr/bin/env python3
"""Check the governance gates a normative change must clear.

A normative change is an edit to a rule file under `yaml/rules/` or to a
schema module `yaml/schema*.yaml`. Two gates apply, both measured against
the base branch:

1. A rule ID this branch defines must already be reserved on the base, so
   two branches cannot allocate one ID (issue #174).
2. A normative change must add a `changelog.d/` fragment, so every change
   to the contract is recorded where release notes are assembled from.

CONTRIBUTING.md states both procedures. Run against a base branch:

    python3 .github/scripts/yaml-validation/check_normative_change.py \\
        --base origin/main

With no `--base` and no GITHUB_BASE_REF the check reports that it has no
base to compare against and passes, so a push build stays green.
"""

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from validate_repository import rule_index_entries, reserved_rule_entries


ROOT = Path(__file__).resolve().parents[3]
RULES_INDEX = 'yaml/rules/README.md'
RULE_FILE_PATTERN = re.compile(r'\Ayaml/rules/(R[0-9]{3})-[^/]+\.md\Z')
SCHEMA_FILE_PATTERN = re.compile(r'\Ayaml/schema[^/]*\.yaml\Z')
FRAGMENT_DIRECTORY = 'changelog.d'
FRAGMENT_PATTERN = re.compile(
    r'\Achangelog\.d/([1-9][0-9]*)-[a-z0-9][a-z0-9-]*\.md\Z'
)


def git(*arguments, root=ROOT):
    """Return stdout of one git command, or None when it fails."""
    result = subprocess.run(
        ('git', *arguments),
        cwd=root,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return None
    return result.stdout


def resolve_base(argument):
    """Return the base revision to compare against, or None."""
    if argument:
        return argument
    base_ref = os.environ.get('GITHUB_BASE_REF', '').strip()
    if not base_ref:
        return None
    return f'origin/{base_ref}'


def is_normative(path):
    return bool(
        RULE_FILE_PATTERN.match(path) or SCHEMA_FILE_PATTERN.match(path)
    )


def head_rule_ids(root):
    """Return the rule IDs the working tree defines."""
    rules = root / 'yaml' / 'rules'
    return {path.name[:4] for path in rules.glob('R[0-9][0-9][0-9]-*.md')}


def base_rule_ids(base, root=ROOT):
    """Return (indexed, reserved) rule IDs as the base branch has them."""
    index = git('show', f'{base}:{RULES_INDEX}', root=root)
    if index is None:
        return None, None
    indexed, _ = rule_index_entries(index)
    reserved, _ = reserved_rule_entries(index)
    return set(indexed), set(reserved)


def check_reserved_ids(base, root=ROOT):
    """A rule ID this branch defines must be reserved on the base."""
    indexed, reserved = base_rule_ids(base, root=root)
    if indexed is None:
        return [
            f"ERROR: cannot read {RULES_INDEX} at {base}; fetch the base "
            "branch before running this check"
        ]
    errors = []
    for rule_id in sorted(head_rule_ids(root) - indexed):
        if rule_id not in reserved:
            errors.append(
                f"ERROR: {rule_id} is defined here but was neither indexed "
                f"nor reserved on {base}; reserve the ID on the base branch "
                "first (CONTRIBUTING.md, 'Reserving a rule ID')"
            )
    return errors


def check_changelog_fragment(normative, added):
    """A normative change must add a changelog fragment."""
    if not normative:
        return []
    fragments = [
        path for path in added
        if path.startswith(f'{FRAGMENT_DIRECTORY}/')
        and path != f'{FRAGMENT_DIRECTORY}/README.md'
    ]
    errors = []
    for path in sorted(fragments):
        if not FRAGMENT_PATTERN.match(path):
            errors.append(
                f"ERROR: {path} must be named "
                f"{FRAGMENT_DIRECTORY}/<issue>-<slug>.md, where <issue> is "
                "the issue number the change closes and <slug> is lowercase "
                "words joined by hyphens"
            )
    if not fragments:
        changed = ', '.join(sorted(normative))
        errors.append(
            f"ERROR: this change is normative ({changed}) and adds no "
            f"{FRAGMENT_DIRECTORY}/ fragment; add one entry file "
            "(CONTRIBUTING.md, 'Recording a normative change')"
        )
    return errors


def changed_paths(base, root=ROOT):
    """Return (all changed paths, added paths) against the merge base."""
    merge_base = git('merge-base', base, 'HEAD', root=root)
    if merge_base is None:
        return None, None
    revision = merge_base.strip()
    changed = git('diff', '--name-only', revision, 'HEAD', root=root)
    added = git(
        'diff', '--name-only', '--diff-filter=A', revision, 'HEAD', root=root
    )
    if changed is None or added is None:
        return None, None
    return changed.split(), added.split()


def main():
    parser = argparse.ArgumentParser(
        description='Check the governance gates for a normative change.'
    )
    parser.add_argument('--base', default='',
                        help='base revision, default $GITHUB_BASE_REF')
    parser.add_argument('--root', type=Path, default=ROOT,
                        help='repository root directory')
    arguments = parser.parse_args()

    base = resolve_base(arguments.base)
    if base is None:
        print('SKIP: no base branch to compare against.')
        return 0

    changed, added = changed_paths(base, root=arguments.root)
    if changed is None:
        print(
            f"ERROR: cannot compare against {base}; fetch the base branch "
            "with its history before running this check"
        )
        return 1

    errors = check_reserved_ids(base, root=arguments.root)
    errors.extend(check_changelog_fragment(
        [path for path in changed if is_normative(path)], added
    ))

    if errors:
        for error in errors:
            print(error)
        return 1

    print(f'PASS: normative change gates clear against {base}.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
