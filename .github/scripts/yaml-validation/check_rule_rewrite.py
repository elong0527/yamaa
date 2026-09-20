#!/usr/bin/env python3
"""Check draft identity and migration coverage, not semantic equivalence."""

import argparse
import re
from collections import Counter
from pathlib import Path

import yaml

LEGACY = re.compile(r"\*\*(R[0-9]{3}-[1-9][0-9]*[a-z]?)\.\*\*")
DEFINITION = re.compile(r"\*\*(REQ-[0-9]{4,})\.\*\*")
REFERENCE = re.compile(r"\bREQ-[0-9]{4,}\b")
SECTIONS = [
    "Purpose", "Scope and dependencies", "Requirements", "Error conditions",
    "Conformance examples", "Rationale",
]


class UniqueLoader(yaml.SafeLoader):
    """Reject duplicate keys instead of hiding migration entries."""

    def construct_mapping(self, node, deep=False):
        result = {}
        for key_node, value_node in node.value:
            key = self.construct_object(key_node, deep=deep)
            if key in result:
                raise ValueError(f"duplicate migration key: {key}")
            result[key] = self.construct_object(value_node, deep=deep)
        return result


def check(root):
    """Return errors and per-source (mapped, total) coverage."""
    errors = []
    directory = root / "yaml" / "rules-next"
    legacy_by_rule = {}
    for path in sorted((root / "yaml" / "rules").glob("R[0-9]*.md")):
        legacy_by_rule[path.name.split("-")[0]] = set(
            LEGACY.findall(path.read_text(encoding="ascii"))
        )
    legacy = set().union(*legacy_by_rule.values())
    definitions = []
    references = set()
    for path in sorted(directory.glob("*/*.md")):
        label = path.relative_to(root)
        try:
            body = path.read_text(encoding="ascii")
        except UnicodeDecodeError:
            errors.append(f"{label}: draft source must be ASCII")
            continue
        if "Status: non-normative draft for issue #606." not in body:
            errors.append(f"{label}: missing draft status")
        if re.findall(r"^## (.+)$", body, re.MULTILINE) != SECTIONS:
            errors.append(f"{label}: incorrect contract section order")
        found = DEFINITION.findall(body)
        if not found:
            errors.append(f"{label}: no requirement definitions")
        definitions.extend(found)
        references.update(REFERENCE.findall(body))
    counts = Counter(definitions)
    for identifier, count in counts.items():
        if count > 1:
            errors.append(f"duplicate draft requirement: {identifier}")
    for identifier in sorted(references - counts.keys()):
        errors.append(f"unresolved draft reference: {identifier}")

    try:
        migration = yaml.load(
            (directory / "migration.yaml").read_text(encoding="ascii"),
            Loader=UniqueLoader,
        )
    except (ValueError, OSError, yaml.YAMLError) as exc:
        return errors + [f"migration.yaml: {exc}"], {}
    if (not isinstance(migration, dict)
            or set(migration) != {"version", "baseline", "complete_sources", "mappings"}
            or migration.get("version") != 1
            or not isinstance(migration.get("baseline"), str)
            or not re.fullmatch(r"[0-9a-f]{7,40}", migration["baseline"])
            or not isinstance(migration.get("complete_sources"), list)
            or not all(isinstance(item, str) for item in migration["complete_sources"])
            or not isinstance(migration.get("mappings"), list)):
        return errors + ["migration.yaml: invalid migration structure"], {}

    mapped = set()
    targets = set()
    for entry in migration["mappings"]:
        if (not isinstance(entry, dict) or set(entry) != {"target", "sources"}
                or not isinstance(entry.get("target"), str)
                or not isinstance(entry.get("sources"), list)
                or not entry["sources"]
                or not all(isinstance(item, str) for item in entry["sources"])):
            errors.append("migration.yaml: each mapping needs a target and nonempty sources")
            continue
        target, sources = entry["target"], entry["sources"]
        if target not in counts:
            errors.append(f"unknown migration target: {target}")
        if target in targets:
            errors.append(f"duplicate migration target: {target}")
        targets.add(target)
        if len(sources) != len(set(sources)):
            errors.append(f"{target}: duplicate migration source")
        for source in sources:
            if source not in legacy:
                errors.append(f"unknown legacy requirement: {source}")
            else:
                mapped.add(source)
    for target in sorted(counts.keys() - targets):
        errors.append(f"draft requirement has no legacy provenance: {target}")
    for rule in migration["complete_sources"]:
        if rule not in legacy_by_rule:
            errors.append(f"unknown complete source: {rule}")
        else:
            for source in sorted(legacy_by_rule[rule] - mapped):
                errors.append(f"complete source has unmapped requirement: {source}")
    coverage = {
        rule: (len(ids & mapped), len(ids))
        for rule, ids in legacy_by_rule.items()
    }
    return errors, coverage


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[3])
    args = parser.parse_args()
    errors, coverage = check(args.root)
    for error in errors:
        print(f"ERROR: {error}")
    for rule, (mapped, total) in coverage.items():
        if mapped:
            print(f"{rule}: {mapped}/{total} legacy requirements mapped to drafts")
    mapped = sum(count for count, _ in coverage.values())
    total = sum(count for _, count in coverage.values())
    print(f"Rewrite coverage: {mapped}/{total}; {total - mapped} pending")
    if not errors:
        print("PASS: Draft traceability checks; semantic review and cutover remain pending.")
    return int(bool(errors))


if __name__ == "__main__":
    raise SystemExit(main())
