#!/usr/bin/env python3
"""Validate current rule ownership and schema navigation.

The verification contract and the verification schema must also name the same
checks: a requirement that defines a verification no registry declares, or a
registered verification no requirement defines, is a second authority that
neither the validator nor an engine will honor.
"""

import argparse
import re
from pathlib import Path

from editorial import UniqueKeyLoader

import yaml

DEFINITION = re.compile(r"\*\*(REQ-[0-9]{4,})\.\*\*")
RETIRED_DEFINITION = re.compile(
    r"^\*\*(REQ-[0-9]{4,})\.\*\*\s*(?:this form is )?retired\b",
    re.IGNORECASE | re.MULTILINE,
)
REFERENCE = re.compile(r"\bREQ-[0-9]{4,}\b")
SECTIONS = [
    "Requirements",
    "Error conditions",
]
VERIFICATION_CONTRACT = "execution/verification.md"
VERIFICATION_REGISTRIES = (
    "column_verifications",
    "dataset_verifications",
    "intermediate_verifications",
)
# A requirement defines a verification when it opens with the check's name,
# as in "**REQ-0381.** `unique` requires ...".
DEFINING_NAME = re.compile(
    r"^\*\*(REQ-[0-9]{4,})\.\*\*\s+(?:An?\s+|The\s+)?`([a-z][a-z0-9_]*)`",
    re.MULTILINE,
)


class SchemaSourceError(ValueError):
    """Invalid authored schema source or navigation metadata."""


def load_schema(path):
    """Parse contract-bearing schemas with the repository's strict source loader."""
    try:
        document = yaml.load(path.read_text(encoding="ascii"), Loader=UniqueKeyLoader)
    except (OSError, yaml.YAMLError) as exc:
        raise SchemaSourceError(f"{path.name}: invalid schema source: {exc}") from exc
    if not isinstance(document, dict):
        raise SchemaSourceError(f"{path.name}: expected schema mapping")
    return document


def contract_paths(root):
    """Read normative sources; generated reference pages are navigation only."""
    return [
        path
        for path in sorted((root / "rules").glob("*/*.md"))
        if path.name not in {"schema-fields.md", "requirements.md", "glossary.md"}
    ]


def requirement_owners(root):
    """Derive the current requirement index from its owning contracts."""
    owners = {}
    for path in contract_paths(root):
        label = path.relative_to(root / "rules").as_posix()
        for identifier in DEFINITION.findall(path.read_text(encoding="ascii")):
            if identifier in owners:
                raise ValueError(f"duplicate requirement: {identifier}")
            owners[identifier] = label
    return owners


def verification_registry_errors(root):
    """Match the checks the verification contract defines with the registries.

    A requirement in the verification contract may open with a registered
    check or with a payload field one declares (`filter`, `when`); any other
    opening name defines a check the schema rejects. Every registered check must
    open at least one requirement, so none is registered without its contract.
    """
    schema_path = root / "yaml/schema_verification.yaml"
    contract_path = root / "rules" / VERIFICATION_CONTRACT
    if not schema_path.is_file() or not contract_path.is_file():
        return []  # A partial root carries no verification vocabulary to match.
    try:
        schema = load_schema(schema_path)
    except ValueError as exc:
        return [str(exc)]
    classes = {name: value for name, value in schema.items() if isinstance(value, list)}

    def fields(definition):
        """Yield the payload fields one check declares, through named classes."""
        if isinstance(definition, list):
            for item in definition:
                if isinstance(item, dict):
                    yield from (key for key in item if key != "fields_from")
        elif isinstance(definition, dict):
            types = definition.get("type", [])
            for name in [types] if isinstance(types, str) else types:
                if name in classes:
                    yield from fields(classes[name])

    registered = set()
    declared = set()
    for registry in VERIFICATION_REGISTRIES:
        operations = schema.get(registry)
        if isinstance(operations, dict):
            registered.update(operations)
            for definition in operations.values():
                declared.update(fields(definition))
    declared |= registered
    contract = contract_path.read_text(encoding="ascii")
    defined = {}
    for identifier, name in DEFINING_NAME.findall(contract):
        defined.setdefault(name, identifier)
    errors = [
        f"{VERIFICATION_CONTRACT}: {identifier} defines `{name}`, "
        "which no verification registry declares"
        for name, identifier in sorted(defined.items())
        if name not in declared
    ]
    errors.extend(
        f"schema_verification.yaml: `{name}` is registered but no "
        f"{VERIFICATION_CONTRACT} requirement defines it"
        for name in sorted(registered - defined.keys())
    )
    return errors


def schema_descriptions(node, parts=(), field_entry=False):
    """Distinguish navigation metadata from a class field named description."""
    if isinstance(node, dict):
        for key, value in node.items():
            if key == "description" and not field_entry:
                yield ".".join(parts) + ".description", value
            else:
                yield from schema_descriptions(value, (*parts, key))
    elif isinstance(node, list):
        for value in node:
            yield from schema_descriptions(value, parts, field_entry=True)


def description_requirement(source, value):
    """Validate every present navigation description, including non-string values."""
    if not isinstance(value, str):
        raise SchemaSourceError(f"schema description must be a string: {source}")
    pointer = re.fullmatch(r"See (REQ-[0-9]{4,})\.", value)
    if pointer is None:
        raise SchemaSourceError(f"schema description has no canonical owner: {source}")
    return pointer[1]


def check(root):
    """Return errors and the current requirement-to-contract index."""
    errors = []
    directory = root / "rules"
    index = (directory / "README.md").read_text(encoding="ascii")
    found = {}
    references = set()
    paths = contract_paths(root)
    for path in paths:
        label = path.relative_to(directory).as_posix()
        try:
            body = path.read_text(encoding="ascii")
            match = re.match(r"---\n(.*?)\n---\n", body, re.DOTALL)
            meta = yaml.load(match[1], Loader=UniqueKeyLoader) if match else None
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
        sections = re.findall(r"^## (.+)$", body, re.MULTILINE)
        if sections not in (["Requirements"], SECTIONS):
            errors.append(f"{label}: incorrect contract section order")
        definitions = DEFINITION.findall(body)
        if not definitions:
            errors.append(f"{label}: no requirements")
        for identifier in RETIRED_DEFINITION.findall(body):
            errors.append(f"{label}: retired requirement must be deleted: {identifier}")
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
    # Schema descriptions are navigation to current normative definitions.
    for path in sorted((root / "yaml").glob("schema*.yaml")):
        schema_text = path.read_text()
        for comment in re.findall(
            r"(?:^\s*#|\s+#)\s*([^\n]*)", schema_text, re.MULTILINE
        ):
            pointer = re.fullmatch(r"See (REQ-[0-9]{4,})\.", comment)
            if pointer is None or pointer[1] not in found:
                errors.append(f"schema comment has no canonical owner: {path.name}")
        try:
            document = load_schema(path)
        except ValueError as exc:
            errors.append(str(exc))
            continue
        for field, value in schema_descriptions(document):
            source = f"{path.name}:{field}"
            try:
                identifier = description_requirement(source, value)
            except ValueError as exc:
                errors.append(str(exc))
                continue
            if identifier not in found:
                errors.append(f"schema description has no canonical owner: {source}")
    errors.extend(verification_registry_errors(root))
    return errors, found


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root", type=Path, default=Path(__file__).resolve().parents[3]
    )
    args = parser.parse_args()
    errors, owners = check(args.root)
    for error in errors:
        print(f"ERROR: {error}")
    if not errors:
        print(
            f"PASS: {len(owners)} current requirements, canonical ownership,"
            " schema navigation, and verification registry."
        )
    return int(bool(errors))


if __name__ == "__main__":
    raise SystemExit(main())
