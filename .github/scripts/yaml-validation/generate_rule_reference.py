#!/usr/bin/env python3
"""Generate schema field and requirement indexes without duplicating semantics."""

import argparse
import json
from pathlib import Path

from check_rule_rewrite import UniqueLoader, load_migration

import yaml


def descriptors(node, parts=()):
    if isinstance(node, dict):
        if "type" in node and not isinstance(node["type"], dict):
            yield ".".join(parts), node
        for key, value in node.items():
            if isinstance(value, (dict, list)):
                yield from descriptors(value, (*parts, key))
    elif isinstance(node, list):
        for value in node:
            yield from descriptors(value, parts)


def cell(value):
    return "`" + json.dumps(value, ensure_ascii=True).replace("|", "\\|") + "`"


def generated(root):
    migration = load_migration(root)
    prose = {entry["source"]: entry["target"] for entry in migration["schema_prose"]}
    # Post-rewrite requirements register provenance in the addenda; the
    # generated field table links those contracts the same way.
    prose.update(
        {
            entry["source"]: entry["target"]
            for entry in migration.get("provenance_addenda", [])
        }
    )
    fields = [
        "# Schema fields",
        "",
        "<!-- generated: generate_rule_reference.py -->",
        "",
        "This table is a generated view of schema shape and defaults. Follow the",
        "requirement link for behavior. It is not an additional semantic contract.",
        "",
    ]
    for path in sorted((root / "yaml").glob("schema*.yaml")):
        fields.extend(
            [
                f"## {path.name}",
                "",
                "| Field or value type | Type | Required | Default | Constraints | Contract |",
                "| --- | --- | --- | --- | --- | --- |",
            ]
        )
        document = yaml.load(path.read_text(encoding="ascii"), Loader=UniqueLoader)
        for name, descriptor in descriptors(document):
            target = prose.get(f"{path.name}:{name}.description")
            entry = migration["requirements"].get(target, {})
            if entry.get("retired"):
                replacement = entry["replacement"]
                target = replacement[0] if len(replacement) == 1 else None
            owner = migration["requirements"].get(target, {}).get("file")
            contract = (
                f"[{target}](../{owner}#{target.lower()})"
                if target
                else "Schema constraint"
            )
            constraints = {
                k: descriptor[k]
                for k in ("values", "pattern", "min_length", "size")
                if k in descriptor
            }
            default = (
                cell(descriptor["default"]) if "default" in descriptor else "Absent"
            )
            fields.append(
                f"| `{name}` | {cell(descriptor['type'])} | {cell(descriptor.get('required', False))} | {default} | {cell(constraints) if constraints else '--'} | {contract} |"
            )
        fields.append("")
    index = [
        "# Requirement index",
        "",
        "<!-- generated: generate_rule_reference.py -->",
        "",
        "Active IDs identify requirements independently of file paths.",
        "Historical and retired IDs are tracked in migration.yaml.",
        "",
        "| Requirement | Contract | Historical citations |",
        "| --- | --- | --- |",
    ]
    requirements = migration["requirements"]
    historical_sources = {
        identifier: [source for source in entry["sources"] if source.startswith("R")]
        for identifier, entry in requirements.items()
        if not entry.get("retired")
    }
    for entry in requirements.values():
        if entry.get("retired"):
            for replacement in entry["replacement"]:
                historical_sources[replacement].extend(
                    source for source in entry["sources"] if source.startswith("R")
                )
    for identifier, entry in requirements.items():
        if entry.get("retired"):
            continue
        historical = (
            ", ".join(dict.fromkeys(historical_sources[identifier]))
            or "Schema prose"
        )
        index.append(
            f"| [{identifier}](../{entry['file']}#{identifier.lower()}) | `{entry['file']}` | {historical} |"
        )
    index.append("")
    return {
        root / "rules/reference/schema-fields.md": "\n".join(fields),
        root / "rules/reference/requirements.md": "\n".join(index),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root", type=Path, default=Path(__file__).resolve().parents[3]
    )
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    stale = []
    for path, text in generated(args.root).items():
        if args.check:
            if not path.exists() or path.read_text(encoding="ascii") != text:
                stale.append(str(path.relative_to(args.root)))
        else:
            path.write_text(text, encoding="ascii")
    for path in stale:
        print(f"ERROR: regenerate {path}")
    return int(bool(stale))


if __name__ == "__main__":
    raise SystemExit(main())
