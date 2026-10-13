"""Check and fix the written form of yamaa specifications.

The specification style contract (`rules/specification/style.md`) fixes one
written form of a specification. A style finding names a
departure from it; it is not a validation diagnostic, and a specification
with findings stays valid and runs exactly as its canonical form does.

`python -m yamaa.style PATH ...` runs the same check from a shell and exits
nonzero when a finding remains.
"""

from __future__ import annotations

import json
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass
from importlib.resources import files
from pathlib import Path

import yaml

from yamaa.style._checks import FINDINGS as _FINDINGS
from yamaa.style._checks import check, suppressed
from yamaa.style._document import parse
from yamaa.style._fix import fix_text


@dataclass(frozen=True, slots=True)
class _StyleFinding:
    """One private departure from the specification style contract."""

    name: str
    requirement: str
    path: str
    line: int
    column: int
    spec_path: str
    message: str

    def render(self) -> str:
        return (
            f"{self.path}:{self.line}:{self.column}: {self.name} "
            f"{self.message} ({self.requirement})"
        )


def _field_orders() -> dict[str, tuple[str, ...]]:
    """The packaged field order of every class REQ-1281 orders."""
    fields = json.loads(files("yamaa.style").joinpath("_fields.json").read_text())
    return {name: tuple(order) for name, order in fields.items()}


def _orders(orders: Mapping[str, Sequence[str]] | None) -> Mapping[str, Sequence[str]]:
    return _field_orders() if orders is None else orders


def _findings(
    path: str | Path,
    text: str,
    orders: Mapping[str, Sequence[str]],
    selected: frozenset[str] | None,
) -> tuple[_StyleFinding, ...]:
    document = parse(text)
    kept, _ = suppressed(document, check(document, orders))
    return tuple(
        _StyleFinding(
            name=item.name,
            requirement=_FINDINGS[item.name],
            path=str(path),
            line=item.line + 1,
            column=item.column + 1,
            spec_path=item.spec_path,
            message=item.message,
        )
        for item in kept
        if selected is None or item.name in selected
    )


def _check_file(
    path: str | Path,
    *,
    select: Iterable[str] | None = None,
    orders: Mapping[str, Sequence[str]] | None = None,
) -> tuple[_StyleFinding, ...]:
    """Every unsuppressed style finding in one specification file."""
    text = Path(path).read_text(encoding="utf-8")
    selected = _selection(select)
    return _findings(path, text, _orders(orders), selected)


def _fix_file(
    path: str | Path,
    *,
    select: Iterable[str] | None = None,
    orders: Mapping[str, Sequence[str]] | None = None,
    write: bool = True,
) -> tuple[bool, tuple[_StyleFinding, ...]]:
    """Apply the proved layout fixes; return (changed, remaining findings)."""
    source = Path(path)
    text = source.read_text(encoding="utf-8")
    selected = _selection(select)
    resolved = _orders(orders)
    fixed = fix_text(text, resolved, selected)
    if write and fixed != text:
        source.write_text(fixed, encoding="utf-8")
    return fixed != text, _findings(path, fixed, resolved, selected)


def _is_specification(path: str | Path, root_fields: Sequence[str]) -> bool:
    """True when the file's root declares `schema_version` and only root fields."""
    try:
        node = yaml.compose(
            Path(path).read_text(encoding="utf-8"), Loader=yaml.BaseLoader
        )
    except (OSError, UnicodeDecodeError, yaml.YAMLError):
        return False
    if not isinstance(node, yaml.MappingNode):
        return False
    names = {key.value for key, _ in node.value}
    return "schema_version" in names and names <= set(root_fields)


def _specification_files(
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
            if _is_specification(candidate, root_fields):
                found.append(candidate)
    return found


def _selection(select: Iterable[str] | None) -> frozenset[str] | None:
    if select is None:
        return None
    names = frozenset(select)
    unknown = sorted(names - _FINDINGS.keys())
    if unknown:
        raise ValueError(f"unknown style finding: {', '.join(unknown)}")
    return names


__all__: list[str] = []
