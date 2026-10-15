"""Repository schema source integrity; language admission is tested in Rust."""

import re
from pathlib import Path

from tools.validation.check_rule_rewrite import SchemaSourceError, load_schema


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
            if not isinstance(name, str) or re.fullmatch(r"schema(?:_[a-z0-9_]+)?\.yaml", name) is None:
                errors.append(f"ERROR: {label}.includes: expected a local schema file name")
            elif not (directory / name).is_file():
                errors.append(f"ERROR: {label}.includes: missing schema source {name!r}")
    return errors
