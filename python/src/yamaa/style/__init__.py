"""Check and fix the written form of yamaa specifications.

The specification style contract (`rules/specification/style.md`) fixes one
written form of a specification. A style finding names a
departure from it; it is not a validation diagnostic, and a specification
with findings stays valid and runs exactly as its canonical form does.

    from yamaa.style import check_file, fix_file

    for finding in check_file("adsl.yaml"):
        print(finding.render())

`python -m yamaa.style PATH ...` runs the same check from a shell and exits
nonzero when a finding remains.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping, Sequence
from pathlib import Path

import yaml
from pydantic import BaseModel, ConfigDict, Field

from yamaa.specification._yaml import _Yaml12Loader
from yamaa.specification.schema import class_fields, load_schema_bundle
from yamaa.style._checks import FINDINGS, LINE_WIDTH, check, suppressed
from yamaa.style._document import parse
from yamaa.style._fix import fix_text

_ORDERED_CLASSES = (
    "root_class",
    "dataset_class",
    "output_class",
    "intermediate_class",
    "column_class",
    "row_class",
)


class StyleFinding(BaseModel):
    """One departure from the specification style contract."""

    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)

    name: str = Field(min_length=1)
    requirement: str = Field(pattern=r"^REQ-[0-9]{4,}$")
    path: str = Field(min_length=1)
    line: int = Field(ge=1)
    column: int = Field(ge=1)
    spec_path: str = Field(min_length=1)
    message: str = Field(min_length=1)

    def render(self) -> str:
        return (
            f"{self.path}:{self.line}:{self.column}: {self.name} "
            f"{self.message} ({self.requirement})"
        )


def field_orders(schema_root: str | Path) -> dict[str, tuple[str, ...]]:
    """The schema field order of every class REQ-1281 orders."""
    bundle = load_schema_bundle(schema_root)
    return {name: tuple(class_fields(bundle, name)) for name in _ORDERED_CLASSES}


def discover_schema_root(path: str | Path) -> Path:
    """The schema bundle the engine would use for the file at `path`."""
    from yamaa._reference_domain import _discover_schema_root

    return _discover_schema_root(Path(path).resolve())


def _orders(
    path: str | Path,
    schema_root: str | Path | None,
    orders: Mapping[str, Sequence[str]] | None,
) -> Mapping[str, Sequence[str]]:
    if orders is not None:
        return orders
    root = schema_root if schema_root is not None else discover_schema_root(path)
    return field_orders(root)


def _findings(
    path: str | Path,
    text: str,
    orders: Mapping[str, Sequence[str]],
    selected: frozenset[str] | None,
) -> tuple[StyleFinding, ...]:
    document = parse(text)
    kept, _ = suppressed(document, check(document, orders))
    return tuple(
        StyleFinding(
            name=item.name,
            requirement=FINDINGS[item.name],
            path=str(path),
            line=item.line + 1,
            column=item.column + 1,
            spec_path=item.spec_path,
            message=item.message,
        )
        for item in kept
        if selected is None or item.name in selected
    )


def check_file(
    path: str | Path,
    *,
    schema_root: str | Path | None = None,
    select: Iterable[str] | None = None,
    orders: Mapping[str, Sequence[str]] | None = None,
) -> tuple[StyleFinding, ...]:
    """Every unsuppressed style finding in one specification file."""
    text = Path(path).read_text(encoding="utf-8")
    selected = _selection(select)
    return _findings(path, text, _orders(path, schema_root, orders), selected)


def fix_file(
    path: str | Path,
    *,
    schema_root: str | Path | None = None,
    select: Iterable[str] | None = None,
    orders: Mapping[str, Sequence[str]] | None = None,
    write: bool = True,
) -> tuple[bool, tuple[StyleFinding, ...]]:
    """Apply the proved layout fixes; return (changed, remaining findings)."""
    source = Path(path)
    text = source.read_text(encoding="utf-8")
    selected = _selection(select)
    resolved = _orders(path, schema_root, orders)
    fixed = fix_text(text, resolved, selected)
    if write and fixed != text:
        source.write_text(fixed, encoding="utf-8")
    return fixed != text, _findings(path, fixed, resolved, selected)


def is_specification(path: str | Path, root_fields: Sequence[str]) -> bool:
    """True when the file's root declares `schema_version` and only root fields."""
    try:
        node = yaml.compose(
            Path(path).read_text(encoding="utf-8"), Loader=_Yaml12Loader
        )
    except (OSError, UnicodeDecodeError, yaml.YAMLError):
        return False
    if not isinstance(node, yaml.MappingNode):
        return False
    names = {key.value for key, _ in node.value}
    return "schema_version" in names and names <= set(root_fields)


def specification_files(
    paths: Iterable[str | Path], root_fields: Sequence[str]
) -> list[Path]:
    """Expand directories into the specification files beneath them.

    A named file is always checked. Under a directory, a `.yaml` file is a
    specification when its root holds `schema_version` and only root fields,
    so a study document, a project environment, and a project configuration
    are left out. Hidden directories are skipped.
    """
    found: list[Path] = []
    for entry in map(Path, paths):
        if not entry.is_dir():
            found.append(entry)
            continue
        for candidate in sorted(entry.rglob("*.yaml")):
            relative = candidate.relative_to(entry).parts[:-1]
            if any(part.startswith(".") for part in relative):
                continue
            if is_specification(candidate, root_fields):
                found.append(candidate)
    return found


def _selection(select: Iterable[str] | None) -> frozenset[str] | None:
    if select is None:
        return None
    names = frozenset(select)
    unknown = sorted(names - FINDINGS.keys())
    if unknown:
        raise ValueError(f"unknown style finding: {', '.join(unknown)}")
    return names


__all__ = [
    "FINDINGS",
    "LINE_WIDTH",
    "StyleFinding",
    "check_file",
    "discover_schema_root",
    "field_orders",
    "fix_file",
    "is_specification",
    "specification_files",
]
