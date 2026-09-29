"""The fixed ODM schema an `odm` expression reads, and the reads themselves.

REQ-1266 fixes the eleven vendor-neutral fields an ODM input carries and
REQ-1267 binds stored field names to them by ASCII case folding. This module
owns those facts, finds every `odm` read a specification writes, and parses
one read's payload; ingestion verifies the input and the row resolver reads
it.
"""

from __future__ import annotations

from collections.abc import Iterator, Mapping, Sequence
from dataclasses import dataclass
from typing import Literal

from yamaa.specification.models import Expression, Specification

ODM_SCHEMA_FIELDS: tuple[str, ...] = (
    "StudyOID",
    "MetaDataVersionOID",
    "SubjectKey",
    "StudyEventOID",
    "StudyEventRepeatKey",
    "FormOID",
    "FormRepeatKey",
    "ItemGroupOID",
    "ItemGroupRepeatKey",
    "ItemOID",
    "Value",
)
"""REQ-1266: the fields of an ODM input, in ODM order."""

ODM_HIERARCHY_FIELDS: tuple[str, ...] = (
    "StudyOID",
    "SubjectKey",
    "StudyEventOID",
    "StudyEventRepeatKey",
    "FormOID",
    "FormRepeatKey",
    "ItemGroupOID",
    "ItemGroupRepeatKey",
)
"""REQ-1266: the fields a row's ODM scope is taken on (REQ-1269)."""

ODM_IDENTIFYING_FIELDS: tuple[str, ...] = tuple(
    field for field in ODM_SCHEMA_FIELDS if field != "Value"
)
"""REQ-1266: the fields that can tell two records apart (REQ-1278)."""

ODM_REQUIRED_VALUES: tuple[str, ...] = (
    "StudyOID",
    "MetaDataVersionOID",
    "SubjectKey",
    "StudyEventOID",
    "FormOID",
    "ItemGroupOID",
    "ItemOID",
)
"""REQ-1268: the fields every record of an ODM input carries."""

_CANONICAL = {name.lower(): name for name in ODM_SCHEMA_FIELDS}


def fold_name(name: str) -> str:
    """Fold `A` through `Z` to `a` through `z` and nothing else (REQ-1267).

    Unicode case folding would bind names REQ-1267 keeps apart, such as one
    spelled with a dotted capital I, and would let the engines disagree.
    """
    return "".join(chr(ord(char) + 32) if "A" <= char <= "Z" else char for char in name)


def schema_field(name: str) -> str | None:
    """Return the schema field a stored name binds to, or None for a vendor field."""
    return _CANONICAL.get(fold_name(name))


@dataclass(frozen=True, slots=True)
class FieldBinding:
    """How an ODM input's stored fields bind to the schema (REQ-1267)."""

    stored: dict[str, str]
    """Each schema field bound once, to its stored name."""
    missing: tuple[str, ...]
    """Schema fields no stored field binds to."""
    ambiguous: dict[str, tuple[str, ...]]
    """Schema fields more than one stored field binds to."""


def bind_fields(names: Sequence[str]) -> FieldBinding:
    """Bind stored field names to the ODM schema without reading a record."""
    seen: dict[str, list[str]] = {}
    for name in names:
        canonical = schema_field(name)
        if canonical is not None:
            seen.setdefault(canonical, []).append(name)
    return FieldBinding(
        stored={field: found[0] for field, found in seen.items() if len(found) == 1},
        missing=tuple(field for field in ODM_SCHEMA_FIELDS if field not in seen),
        ambiguous={
            field: tuple(found) for field, found in seen.items() if len(found) > 1
        },
    )


@dataclass(frozen=True, slots=True)
class OdmRead:
    """One `odm` expression's canonical payload (REQ-1265, REQ-1271)."""

    dataset: str
    item_oid: str
    events: tuple[str, ...] | None
    forms: tuple[str, ...] | None
    item_groups: tuple[str, ...] | None
    filter: str | None

    @property
    def item(self) -> str:
        return f"{self.dataset}.{self.item_oid}"


def _oids(value: object) -> tuple[str, ...] | None:
    if value is None:
        return None
    if isinstance(value, str):
        return (value,)
    if isinstance(value, Sequence):
        return tuple(str(entry) for entry in value)
    return None


def parse_odm_read(payload: object) -> OdmRead | None:
    """Parse a validated payload; None when it is not an `odm` payload.

    The schema has already expanded the string shorthand (REQ-0266), but a
    caller that dispatches a raw expression may still hand over the string.
    """
    if isinstance(payload, str):
        payload = {"item": payload}
    if not isinstance(payload, Mapping):
        return None
    item = payload.get("item")
    if not isinstance(item, str) or "." not in item:
        return None
    dataset, item_oid = item.split(".", 1)
    selector = payload.get("filter")
    return OdmRead(
        dataset=dataset,
        item_oid=item_oid,
        events=_oids(payload.get("event")),
        forms=_oids(payload.get("form")),
        item_groups=_oids(payload.get("item_group")),
        filter=selector if isinstance(selector, str) else None,
    )


Location = Literal["column", "row", "intermediate", "derive"]

_HANDLED = frozenset({"value", "unconvertible"})


@dataclass(frozen=True, slots=True)
class OdmReadSite:
    """Where one `odm` expression is written."""

    path: str
    location: Location
    read: OdmRead
    row_index: int | None = None
    column_name: str | None = None


def iter_odm_payloads(
    node: object, path: str, derive: bool = False
) -> Iterator[tuple[str, object, bool]]:
    """Yield every `odm` payload under a derivation, with its path.

    `derive` marks a payload inside an aggregate's derive step, which
    evaluates per record of another relation and so has no row scope. The
    walk takes a loaded expression or the authored mapping alike, so the
    repository validator finds the reads the planner does, at its paths.
    """
    if isinstance(node, Expression):
        node = node.root
    if isinstance(node, Mapping) and "value" in node and set(node) <= _HANDLED:
        # A nested derivation, such as a derive binding's, is stored in the
        # REQ-0358 wrapper, which the path names only where it was written.
        suffix = ".value" if "unconvertible" in node else ""
        yield from iter_odm_payloads(node["value"], f"{path}{suffix}", derive)
        return
    if isinstance(node, Mapping):
        if len(node) == 1:
            ((operation, payload),) = node.items()
            if operation == "odm":
                yield f"{path}.odm", payload, derive
                return
            inner = derive or operation == "aggregate"
            yield from iter_odm_payloads(payload, f"{path}.{operation}", inner)
            return
        for key, value in node.items():
            yield from iter_odm_payloads(value, f"{path}.{key}", derive)
    elif isinstance(node, Sequence) and not isinstance(node, str):
        for index, value in enumerate(node):
            yield from iter_odm_payloads(value, f"{path}[{index}]", derive)


def _derivation_root(derivation: object, path: str) -> tuple[object, str]:
    """Unwrap a handled derivation to its expression and its authored path.

    REQ-0358 stores every derivation in a `value` wrapper; the path names the
    wrapper only where the author wrote one, as the planner reports it.
    """
    fields = getattr(derivation, "model_fields_set", set())
    value = getattr(derivation, "value", derivation)
    return value, f"{path}.value" if "unconvertible" in fields else path


def odm_read_sites(specification: Specification) -> tuple[OdmReadSite, ...]:
    """Every `odm` expression a specification writes, in declaration order."""
    sites: list[OdmReadSite] = []

    def add(
        derivation: object,
        path: str,
        location: Location,
        row: int | None = None,
        column_name: str | None = None,
    ) -> None:
        node, path = _derivation_root(derivation, path)
        for site_path, payload, derive in iter_odm_payloads(node, path, False):
            read = parse_odm_read(payload)
            if read is not None:
                sites.append(
                    OdmReadSite(
                        path=site_path,
                        location="derive" if derive else location,
                        read=read,
                        row_index=row,
                        column_name=column_name,
                    )
                )

    for column in specification.columns:
        if column.derivation is not None:
            add(
                column.derivation,
                f"columns.{column.name}.derivation",
                "column",
                column_name=column.name,
            )
    for index, row in enumerate(specification.rows or ()):
        for name, derivation in row.derivations.items():
            add(derivation, f"rows[{index}].derivations.{name}", "row", index)
    for index, intermediate in enumerate(specification.intermediates or ()):
        for name, derivation in (intermediate.derivations or {}).items():
            add(
                derivation,
                f"intermediates[{index}].derivations.{name}",
                "intermediate",
            )
    return tuple(sites)


def odm_inputs(specification: Specification) -> frozenset[str]:
    """The declared inputs an `odm` expression names: the ODM inputs (REQ-1266)."""
    declared = set(specification.input)
    return frozenset(
        site.read.dataset
        for site in odm_read_sites(specification)
        if site.read.dataset in declared
    )


__all__ = [
    "ODM_HIERARCHY_FIELDS",
    "ODM_IDENTIFYING_FIELDS",
    "ODM_REQUIRED_VALUES",
    "ODM_SCHEMA_FIELDS",
    "FieldBinding",
    "OdmRead",
    "OdmReadSite",
    "bind_fields",
    "fold_name",
    "iter_odm_payloads",
    "odm_inputs",
    "odm_read_sites",
    "parse_odm_read",
    "schema_field",
]
