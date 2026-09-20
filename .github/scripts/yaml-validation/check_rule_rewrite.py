#!/usr/bin/env python3
"""Validate canonical rule ownership, legacy aliases, and schema provenance."""

import argparse
import re
from collections import Counter
from pathlib import Path

import yaml

DEFINITION = re.compile(r"\*\*(REQ-[0-9]{4,})\.\*\*")
REFERENCE = re.compile(r"\bREQ-[0-9]{4,}\b")
LEGACY = re.compile(r"R[0-9]{3}-[1-9][0-9]*[a-z]?\Z")
SECTIONS = [
    "Purpose",
    "Scope and dependencies",
    "Requirements",
    "Error conditions",
    "Conformance examples",
    "Rationale",
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


def load_migration(root):
    return yaml.load(
        (root / "rules/migration.yaml").read_text(encoding="ascii"),
        Loader=UniqueLoader,
    )


def resolve_requirement(identifier, migration):
    """Resolve canonical or historical IDs without depending on a filename."""
    if identifier in migration["requirements"]:
        return [identifier]
    return migration["sources"].get(identifier, {}).get("targets", [])


def check(root):
    """Return errors and per-source (mapped, total) coverage."""
    errors = []
    directory = root / "rules"
    try:
        migration = load_migration(root)
        if not isinstance(migration, dict) or migration.get("version") != 2:
            return ["migration.yaml: expected version 2 mapping"], {}
        sources = migration["sources"]
        requirements = migration["requirements"]
        prose = migration["schema_prose"]
        addenda = migration.get("provenance_addenda", [])
        if (
            not isinstance(sources, dict)
            or not isinstance(requirements, dict)
            or not isinstance(prose, list)
            or not isinstance(addenda, list)
        ):
            return ["migration.yaml: invalid inventory shape"], {}
    except (ValueError, OSError, KeyError, yaml.YAMLError) as exc:
        return [f"migration.yaml: {exc}"], {}

    # The baseline is immutable. New requirements do not replace source records.
    if migration.get("baseline") != "67b0e56c" or len(sources) != 1071:
        errors.append(
            "migration.yaml: baseline must retain all 1071 source requirements"
        )
    if len(prose) != 332:
        errors.append(
            "migration.yaml: baseline must retain all 332 schema prose entries"
        )
    # Post-rewrite requirements register provenance in the addenda. The
    # baseline above stays immutable; the duplicate check below rejects an
    # addendum that replaces a baseline source record.
    all_prose = prose + addenda
    index = (directory / "README.md").read_text(encoding="ascii")
    found = {}
    references = set()
    paths = sorted(directory.glob("*/*.md"))
    for path in paths:
        if path.name in {"schema-fields.md", "requirements.md"}:
            continue  # Generated reference, not a normative contract.
        label = path.relative_to(directory).as_posix()
        try:
            body = path.read_text(encoding="ascii")
            match = re.match(r"---\n(.*?)\n---\n", body, re.DOTALL)
            meta = yaml.load(match[1], Loader=UniqueLoader) if match else None
        except (ValueError, yaml.YAMLError) as exc:
            errors.append(f"{label}: invalid contract source: {exc}")
            continue
        expected = {
            "id": label.removesuffix(".md"),
            "title": meta.get("title") if isinstance(meta, dict) else None,
            "status": "normative",
        }
        if (
            meta != expected
            or not isinstance(expected["title"], str)
            or not expected["title"]
        ):
            errors.append(f"{label}: expected id, title, and normative status")
        if not re.search(
            rf"^\| .*\]\({re.escape(label)}\) \| normative \|", index, re.MULTILINE
        ):
            errors.append(f"{label}: absent from normative rule index")
        if re.findall(r"^## (.+)$", body, re.MULTILINE) != SECTIONS:
            errors.append(f"{label}: incorrect contract section order")
        definitions = DEFINITION.findall(body)
        if not definitions:
            errors.append(f"{label}: no requirements")
        for identifier in definitions:
            if identifier in found:
                errors.append(f"duplicate requirement: {identifier}")
            found[identifier] = label
        references.update(REFERENCE.findall(body))
        for link in re.findall(r"\]\(([^)]+)\)", body):
            if "://" not in link and not link.startswith("#"):
                target = (path.parent / link.split("#")[0]).resolve()
                if not target.is_file():
                    errors.append(f"{label}: broken local link: {link}")
    for identifier in sorted(references - found.keys()):
        errors.append(f"unresolved requirement reference: {identifier}")
    for identifier in sorted(found.keys() - requirements.keys()):
        errors.append(f"requirement has no migration entry: {identifier}")
    for identifier, entry in requirements.items():
        if (
            not isinstance(entry, dict)
            or entry.get("file") != found.get(identifier)
            or identifier not in found
        ):
            errors.append(f"migration target missing or in wrong file: {identifier}")

    coverage = Counter()
    mapped = Counter()
    for source, entry in sources.items():
        family = source.split("-")[0]
        coverage[family] += 1
        if not LEGACY.fullmatch(source) or not isinstance(entry, dict):
            errors.append(f"invalid legacy source: {source}")
            continue
        targets = entry.get("targets")
        if (
            not isinstance(targets, list)
            or not targets
            or not all(isinstance(target, str) for target in targets)
            or len(targets) != len(set(targets))
        ):
            errors.append(f"unmapped or duplicate legacy targets: {source}")
            continue
        valid = True
        for target in targets:
            if target not in found:
                errors.append(f"unknown migration target: {source} -> {target}")
                valid = False
            elif source not in requirements.get(target, {}).get("sources", []):
                errors.append(f"missing reverse provenance: {source} -> {target}")
                valid = False
        if valid:
            mapped[family] += 1

    prose_by_source = {}
    for entry in all_prose:
        if not isinstance(entry, dict) or not isinstance(entry.get("source"), str):
            errors.append("invalid schema prose entry")
            continue
        source, target = entry["source"], entry.get("target")
        if source in prose_by_source:
            errors.append(f"duplicate schema provenance: {source}")
        prose_by_source[source] = target
        if target not in found or source not in requirements.get(target, {}).get(
            "sources", []
        ):
            errors.append(f"unresolved schema provenance: {source}")
    for target, entry in requirements.items():
        provenance = entry.get("sources", []) if isinstance(entry, dict) else []
        if not provenance or len(provenance) != len(set(provenance)):
            errors.append(f"missing or duplicate provenance: {target}")
        for source in provenance:
            if source in sources:
                if target not in sources[source].get("targets", []):
                    errors.append(f"invalid reverse legacy alias: {target} -> {source}")
            elif prose_by_source.get(source) != target:
                errors.append(f"unknown provenance: {target} -> {source}")

    # Schema descriptions are navigation. Reject a new second semantic source.
    def descriptions(node, parts=()):
        if isinstance(node, dict):
            for key, value in node.items():
                if key == "description" and isinstance(value, str):
                    yield ".".join(parts) + ".description", value
                else:
                    yield from descriptions(value, (*parts, key))
        elif isinstance(node, list):
            for value in node:
                yield from descriptions(value, parts)

    for path in sorted((root / "yaml").glob("schema*.yaml")):
        schema_text = path.read_text()
        for comment in re.findall(
            r"(?:^\s*#|\s+#)\s*([^\n]*)", schema_text, re.MULTILINE
        ):
            pointer = re.fullmatch(r"See (REQ-[0-9]{4,}) in rules/([^ ]+)\.", comment)
            if (
                pointer is None
                or requirements.get(pointer[1], {}).get("file") != pointer[2]
            ):
                errors.append(f"schema comment has no canonical owner: {path.name}")
        for field, value in descriptions(yaml.load(schema_text, Loader=UniqueLoader)):
            source = f"{path.name}:{field}"
            target = prose_by_source.get(source)
            file = requirements.get(target, {}).get("file")
            if value != f"See {target} in rules/{file}.":
                errors.append(f"schema description has no canonical owner: {source}")
    return errors, {
        family: (mapped[family], total) for family, total in sorted(coverage.items())
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root", type=Path, default=Path(__file__).resolve().parents[3]
    )
    args = parser.parse_args()
    errors, coverage = check(args.root)
    for error in errors:
        print(f"ERROR: {error}")
    mapped = sum(count for count, _ in coverage.values())
    total = sum(count for _, count in coverage.values())
    print(f"Legacy coverage: {mapped}/{total}; {total - mapped} unmapped")
    if not errors:
        print(
            "PASS: Canonical ownership, schema provenance, and compatibility aliases."
        )
    return int(bool(errors))


if __name__ == "__main__":
    raise SystemExit(main())
