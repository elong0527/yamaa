#!/usr/bin/env python3
"""Check canonical contracts and resolve new or historical benchmark citations.

The legacy check_rule helper remains available for archived-contract tooling.
The repository gate reads the recursive canonical index and migration map.
"""

import re
import sys
from pathlib import Path

import yaml

sys.path.insert(0, str(Path(__file__).resolve().parent))

from editorial import rule_identity_errors

REPO = Path(__file__).resolve().parents[3]
RULES = REPO / "yaml" / "rules"
EXAMPLES = REPO / "benchmark"
ALLOWED_KEYS = {"id", "title", "status", "applies_to"}
REQUIRED_SECTIONS = ("Intent", "Boundaries", "Errors", "Rationale")
REQUIREMENT = re.compile(r"\*\*(R[0-9]{3}-[1-9][0-9]*[a-z]?)\.\*\*")
CITATION = re.compile(r"(?:REQ-[0-9]{4,}|R[0-9]{3}-[1-9][0-9]*[a-z]?)$")


def frontmatter(path):
    text = path.read_text(encoding="utf-8")
    match = re.match(r"---\n(.*?)\n---\n", text, re.S)
    if match is None:
        return None, text
    return yaml.safe_load(match.group(1)), text[match.end():]


def check_rule(path, errors, requirements):
    stem = path.stem.split("-")[0]
    meta, body = frontmatter(path)
    label = path.relative_to(REPO)
    if not isinstance(meta, dict):
        errors.append(f"ERROR: {label}: missing frontmatter mapping")
        return
    unknown = set(meta) - ALLOWED_KEYS
    if unknown:
        errors.append(f"ERROR: {label}: retired frontmatter keys: {sorted(unknown)}")
    errors.extend(rule_identity_errors(meta, stem, label))
    if not isinstance(meta.get("applies_to"), list) or not meta.get("applies_to"):
        errors.append(f"ERROR: {label}: applies_to must be a non-empty list")
    headings = re.findall(r"^## (.+)$", body, re.M)
    for section in REQUIRED_SECTIONS:
        if section not in headings:
            errors.append(f"ERROR: {label}: missing ## {section}")
    if headings[:2] != ["Intent", "Boundaries"]:
        errors.append(f"ERROR: {label}: begin with Intent and Boundaries")
    if headings[-2:] != ["Errors", "Rationale"]:
        errors.append(f"ERROR: {label}: end with Errors and Rationale")
    found = REQUIREMENT.findall(body)
    stray = [requirement for requirement in found
             if not requirement.startswith(f"{stem}-")]
    if stray:
        errors.append(f"ERROR: {label}: requirements of other rules: {stray}")
    if not found:
        errors.append(f"ERROR: {label}: no numbered requirements")
    duplicates = sorted({item for item in found if found.count(item) > 1})
    if duplicates:
        errors.append(f"ERROR: {label}: duplicate requirements: {duplicates}")
    requirements.update(item for item in found if item.startswith(f"{stem}-"))


def check_error(path, requirements, errors):
    try:
        contract = yaml.safe_load(path.read_text(encoding="utf-8"))
    except yaml.YAMLError as error:
        errors.append(f"ERROR: {path.relative_to(REPO)}: {error}")
        return
    if not isinstance(contract, dict):
        return
    label = path.relative_to(REPO)
    cited = contract.get("requirement")
    if cited is None:
        # Schema-phase failures (e.g. missing_required_field) carry no
        # requirement number; the rule documents them without one.
        return
    if not isinstance(cited, str) or CITATION.fullmatch(cited) is None:
        errors.append(f"ERROR: {label}: requirement must cite REQ-NNNN or a legacy alias")
    elif cited not in requirements:
        errors.append(f"ERROR: {label}: {cited} names no numbered requirement")


def main():
    errors = []
    requirements = set()
    from check_rule_rewrite import check, load_migration
    errors.extend(check(REPO)[0])
    migration = load_migration(REPO)
    requirements.update(migration["requirements"])
    requirements.update(migration["sources"])
    for path in sorted(EXAMPLES.glob("negative-*/expected/error.yaml")):
        check_error(path, requirements, errors)
    for error in errors:
        print(error, file=sys.stderr)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
