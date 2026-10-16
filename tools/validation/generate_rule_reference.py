"""Generate schema field and requirement indexes without duplicating semantics."""

import argparse
import json
from pathlib import Path

from tools.paths import ROOT
from tools.validation.check_rule_rewrite import (
    description_requirement,
    load_schema,
    requirement_owners,
    schema_descriptions,
)


def descriptors(node, parts=(), classes=None, active=()):
    if isinstance(node, dict):
        if "type" in node and not isinstance(node["type"], dict):
            yield ".".join(parts), node
        for key, value in node.items():
            if isinstance(value, (dict, list)):
                yield from descriptors(value, (*parts, key), classes, active)
    elif isinstance(node, list):
        for value in node:
            if isinstance(value, dict) and "fields_from" in value:
                name = value["fields_from"]
                if not isinstance(name, str) or classes is None or name not in classes:
                    raise ValueError(f"unknown fields_from class {name!r}")
                if name in active:
                    raise ValueError(f"class fields_from cycle at {name!r}")
                yield from descriptors(classes[name], parts, classes, (*active, name))
            else:
                yield from descriptors(value, parts, classes, active)


def code_span(text):
    # A code span's fence must be longer than any backtick run inside it,
    # or the first embedded backtick ends the span and corrupts the table.
    fence = "`"
    while fence in text:
        fence += "`"
    return fence + text + fence


def cell(value):
    return code_span(json.dumps(value, ensure_ascii=True).replace("|", "\\|"))


def generated(root):
    owners = requirement_owners(root)
    fields = [
        "# Schema fields",
        "",
        "<!-- generated: generate_rule_reference.py -->",
        "",
        "This table is a generated view of schema shape and defaults. Follow the",
        "requirement link for behavior. It is not an additional semantic contract.",
        "",
    ]
    documents = {
        path: load_schema(path) for path in sorted((root / "yaml").glob("schema*.yaml"))
    }
    for path, document in documents.items():
        for field, value in schema_descriptions(document):
            source = f"{path.name}:{field}"
            target = description_requirement(source, value)
            if target not in owners:
                raise ValueError(f"unknown requirement: {target} ({source})")
    classes = {
        name: definition
        for document in documents.values()
        for name, definition in document.items()
        if isinstance(definition, list)
    }
    for path, document in documents.items():
        fields.extend(
            [
                f"## {path.name}",
                "",
                "| Field or value type | Type | Required | Default | Constraints | Contract |",
                "| --- | --- | --- | --- | --- | --- |",
            ]
        )
        for name, descriptor in descriptors(document, classes=classes):
            target = (
                description_requirement(
                    f"{path.name}:{name}.description", descriptor["description"]
                )
                if "description" in descriptor
                else None
            )
            contract = (
                f"[{target}](../{owners[target]}#{target.lower()})"
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
                f"| {code_span(name)} | {cell(descriptor['type'])} | {cell(descriptor.get('required', False))} | {default} | {cell(constraints) if constraints else '--'} | {contract} |"
            )
        fields.append("")
    index = [
        "# Requirement index",
        "",
        "<!-- generated: generate_rule_reference.py -->",
        "",
        "Active IDs identify requirements independently of file paths.",
        "",
        "| Requirement | Contract |",
        "| --- | --- |",
    ]
    for identifier, owner in sorted(owners.items()):
        index.append(f"| [{identifier}](../{owner}#{identifier.lower()}) | `{owner}` |")
    index.append("")
    return {
        root / "rules/reference/schema-fields.md": "\n".join(fields),
        root / "rules/reference/requirements.md": "\n".join(index),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    stale = []
    try:
        references = generated(args.root)
    except ValueError as exc:
        print(f"ERROR: {exc}")
        return 1
    for path, text in references.items():
        if args.check:
            # The references are generated at docs build time and are not
            # committed; an absent file is current by construction. A file
            # materialized by a local build must still match its sources.
            if path.exists():
                try:
                    current = path.read_text(encoding="ascii")
                except UnicodeDecodeError:
                    # The generated text is pure ASCII, so a materialized
                    # reference with non-ASCII bytes is stale by definition;
                    # report it instead of dying with a traceback.
                    current = None
                if current != text:
                    stale.append(str(path.relative_to(args.root)))
        else:
            path.write_text(text, encoding="ascii")
    for path in stale:
        print(f"ERROR: regenerate {path}")
    return int(bool(stale))


if __name__ == "__main__":
    raise SystemExit(main())
