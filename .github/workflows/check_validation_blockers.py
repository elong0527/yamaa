#!/usr/bin/env python3
import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path

import yaml


def load_blockers(root):
    manifest_path = root / 'yaml' / 'examples' / 'validation-manifest.yaml'
    with open(manifest_path, 'r', encoding='utf-8') as handle:
        manifest = yaml.safe_load(handle)
    fixtures = (
        manifest.get('fixtures', {}) if isinstance(manifest, dict) else {}
    )
    blockers = {}
    for name, entry in fixtures.items():
        if not isinstance(entry, dict) or 'blocked_by' not in entry:
            continue
        blocker = entry['blocked_by']
        if not isinstance(blocker, str) or re.fullmatch(
            r'#[1-9][0-9]*', blocker
        ) is None:
            continue
        blockers.setdefault(int(blocker[1:]), []).append(name)
    return blockers


def github_issue_state(repository, issue_number, token=None):
    url = (
        f'https://api.github.com/repos/{repository}/issues/{issue_number}'
    )
    headers = {
        'Accept': 'application/vnd.github+json',
        'X-GitHub-Api-Version': '2022-11-28',
        'User-Agent': 'yamaa-validation-blocker-check',
    }
    if token:
        headers['Authorization'] = f'Bearer {token}'
    request = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(request, timeout=20) as response:
        payload = json.load(response)
    return payload.get('state'), 'pull_request' in payload


def validate_blocker_states(blockers, state_lookup):
    errors = []
    for issue_number, fixtures in sorted(blockers.items()):
        state, is_pull_request = state_lookup(issue_number)
        if is_pull_request:
            errors.append(
                f'ERROR: #{issue_number} is a pull request, not a blocking issue'
            )
        elif state != 'open':
            errors.append(
                f"ERROR: #{issue_number} is {state!r} but still blocks "
                f"validation fixtures: {', '.join(sorted(fixtures))}"
            )
    return errors


def main():
    parser = argparse.ArgumentParser(
        description='Reject closed issues in the validation manifest.'
    )
    parser.add_argument('--root', type=Path, default=Path(__file__).parents[2])
    parser.add_argument(
        '--repository', default=os.environ.get('GITHUB_REPOSITORY')
    )
    args = parser.parse_args()
    if not args.repository or re.fullmatch(
        r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', args.repository
    ) is None:
        print('ERROR: --repository OWNER/REPO is required')
        return 1

    try:
        blockers = load_blockers(args.root)
        errors = validate_blocker_states(
            blockers,
            lambda number: github_issue_state(
                args.repository, number, os.environ.get('GITHUB_TOKEN')
            ),
        )
    except (OSError, ValueError, yaml.YAMLError, urllib.error.URLError) as exc:
        print(f'ERROR: unable to check validation blockers: {exc}')
        return 1

    if errors:
        for error in errors:
            print(error)
        return 1
    print(f'PASS: {len(blockers)} validation blocker issue(s) remain open.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
