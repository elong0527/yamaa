"""Repository schema source integrity; language admission is tested in Rust."""

import re
from pathlib import Path

import yaml
from tools.validation.editorial import UniqueKeyLoader

# A description that only points at its rule.
POINTER = re.compile(r"See (?P<id>[A-Z]+-\d+)\.")


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


def description_rules(source, value, rules):
    """Return the rule IDs one schema description cites, each resolved.

    During the 1.0.0 conversion a description is ``See ID.`` or free text;
    every rule ID it names must resolve in ``rules``, a
    :class:`tools.rules.model.RuleSet`.
    """
    if not isinstance(value, str):
        raise SchemaSourceError(f"schema description must be a string: {source}")
    if not value.strip():
        raise SchemaSourceError(f"schema description is empty: {source}")
    pointer = POINTER.fullmatch(value)
    cited = [pointer["id"]] if pointer else rules.citation.findall(value)
    for identifier in cited:
        if rules.resolve(identifier) is None:
            raise SchemaSourceError(
                f"schema description cites {identifier}, which names no rule: {source}"
            )
    return cited


def validate_schema_sources(root: Path):
    """Check physical source files and local include spelling without interpretation."""
    directory = root / "yaml"
    entry = directory / "schema.yaml"
    if not entry.is_file():
        return ["ERROR: yaml/schema.yaml: missing repository schema source"]
    errors = []
    for path in sorted(directory.glob("schema*.yaml")):
        label = path.relative_to(root)
        if path.is_symlink() or not path.is_file():
            errors.append(f"ERROR: {label}: schema source must be a regular file")
            continue
        try:
            document = load_schema(path)
        except (SchemaSourceError, UnicodeError) as error:
            errors.append(f"ERROR: {label}: {error}")
            continue
        includes = document.get("includes", [])
        if not isinstance(includes, list):
            errors.append(f"ERROR: {label}.includes: expected file names")
            continue
        for name in includes:
            if (
                not isinstance(name, str)
                or re.fullmatch(r"schema(?:_[a-z0-9_]+)?\.yaml", name) is None
            ):
                errors.append(
                    f"ERROR: {label}.includes: expected a local schema file name"
                )
            elif not (directory / name).is_file():
                errors.append(
                    f"ERROR: {label}.includes: missing schema source {name!r}"
                )
    return errors
