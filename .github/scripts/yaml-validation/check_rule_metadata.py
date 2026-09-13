#!/usr/bin/env python3
"""Enforce rule metadata structure in CI (issue #168).

Each yaml/rules/RNNN-*.md file must carry minimal frontmatter
(id, title, status, applies_to), the required section headings
(Intent, Boundaries, Rationale, Errors), and sequentially numbered
requirements of the form **RNNN-n.** in document order. The depends_on
and supersedes fields are retired: the former was a cyclic graph the
validator never checked, the latter never had a schema.

Every yaml/examples/negative-*/expected/error.yaml must cite the
pinned requirement as `requirement: RNNN-n`, and the cited ID must
exist in the owning rule.
"""

import re
import sys
from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parents[3]
RULES = REPO / "yaml" / "rules"
EXAMPLES = REPO / "yaml" / "examples"
ALLOWED_KEYS = {"id", "title", "status", "applies_to"}
REQUIRED_SECTIONS = ("Intent", "Boundaries", "Rationale", "Errors")
REQUIREMENT = re.compile(r"\*\*R([0-9]{3})-([1-9][0-9]*)\.\*\*")
CITATION = re.compile(r"R[0-9]{3}-[1-9][0-9]*$")


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
    if meta.get("id") != stem:
        errors.append(f"ERROR: {label}: id {meta.get('id')!r} does not match filename")
    if meta.get("status") != "normative":
        errors.append(f"ERROR: {label}: status must be normative")
    if not isinstance(meta.get("applies_to"), list) or not meta.get("applies_to"):
        errors.append(f"ERROR: {label}: applies_to must be a non-empty list")
    headings = re.findall(r"^## (.+)$", body, re.M)
    for section in REQUIRED_SECTIONS:
        if section not in headings:
            errors.append(f"ERROR: {label}: missing ## {section}")
    found = REQUIREMENT.findall(body)
    numbers = [int(number) for rule, number in found if f"R{rule}" == stem]
    stray = [f"R{rule}-{number}" for rule, number in found if f"R{rule}" != stem]
    if stray:
        errors.append(f"ERROR: {label}: requirements of other rules: {stray}")
    if numbers != list(range(1, len(numbers) + 1)):
        errors.append(f"ERROR: {label}: requirements must run R{stem[1:]}-1..-{len(numbers)} in order")
    requirements.update(f"{stem}-{number}" for number in numbers)


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
    if not isinstance(cited, str) or CITATION.fullmatch(cited) is None:
        errors.append(f"ERROR: {label}: requirement must cite RNNN-n")
    elif cited not in requirements:
        errors.append(f"ERROR: {label}: {cited} names no numbered requirement")


def main():
    errors = []
    requirements = set()
    for path in sorted(RULES.glob("R[0-9]*.md")):
        check_rule(path, errors, requirements)
    for path in sorted(EXAMPLES.glob("negative-*/expected/error.yaml")):
        check_error(path, requirements, errors)
    for error in errors:
        print(error, file=sys.stderr)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
