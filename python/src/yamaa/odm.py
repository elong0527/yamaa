"""The fixed ODM schema an `odm` expression reads (specification/binding).

REQ-1266 fixes the eleven fields an ODM input carries and REQ-1267 binds
stored field names to them by ASCII case folding. This module owns those
facts, finds every `odm` read a specification writes, and parses one read's
payload; the engine loads the input under the schema and resolves the read.
"""

ODM_SCHEMA_FIELDS = (
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

ODM_HIERARCHY_FIELDS = (
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

ODM_IDENTIFYING_FIELDS = tuple(f for f in ODM_SCHEMA_FIELDS if f != "Value")
"""REQ-1266: the fields that can tell two records apart (REQ-1278)."""

ODM_REQUIRED_VALUES = (
    "StudyOID",
    "MetaDataVersionOID",
    "SubjectKey",
    "StudyEventOID",
    "FormOID",
    "ItemGroupOID",
    "ItemOID",
)
"""REQ-1268: the fields every record of an ODM input carries."""


def fold_name(name):
    """Fold `A` through `Z` to `a` through `z` and nothing else (REQ-1267).

    Unicode case folding would bind names REQ-1267 keeps apart, such as one
    spelled with a dotted capital I.
    """
    return "".join(chr(ord(c) + 32) if "A" <= c <= "Z" else c for c in name)


_CANONICAL = {fold_name(name): name for name in ODM_SCHEMA_FIELDS}


def schema_field(name):
    """The schema field a stored name binds to, or None for a vendor field."""
    return _CANONICAL.get(fold_name(name))


class FieldBinding:
    """How an ODM input's stored fields bind to the schema (REQ-1267).

    `stored` maps each schema field bound once to its stored name,
    `missing` lists the schema fields nothing binds to, and `ambiguous`
    maps each schema field several stored fields bind to onto those names.
    """

    def __init__(self, stored, missing, ambiguous):
        self.stored = stored
        self.missing = missing
        self.ambiguous = ambiguous


def bind_fields(names):
    """Bind stored field names to the ODM schema without reading a record."""
    seen = {}
    for name in names:
        canonical = schema_field(name)
        if canonical is not None:
            seen.setdefault(canonical, []).append(name)
    return FieldBinding(
        stored={f: found[0] for f, found in seen.items() if len(found) == 1},
        missing=tuple(f for f in ODM_SCHEMA_FIELDS if f not in seen),
        ambiguous={f: tuple(found) for f, found in seen.items() if len(found) > 1},
    )


class OdmRead:
    """One `odm` expression's payload (REQ-1265, REQ-1271)."""

    def __init__(self, dataset, item_oid, events, forms, item_groups, filter):
        self.dataset = dataset
        self.item_oid = item_oid
        self.events = events
        self.forms = forms
        self.item_groups = item_groups
        self.filter = filter

    @property
    def item(self):
        return f"{self.dataset}.{self.item_oid}"


def _oids(value):
    if value is None:
        return None
    if isinstance(value, str):
        return (value,)
    if isinstance(value, list):
        return tuple(str(v) for v in value)
    return None


def parse_odm_read(payload):
    """Parse an `odm` payload; None when it names no `DATASET.ItemOID`.

    REQ-1265: a bare string is the item. The dataset identifier holds no
    period, so the qualifier ends at the first one and the rest, periods
    included, is the complete ItemOID.
    """
    if isinstance(payload, str):
        payload = {"item": payload}
    if not isinstance(payload, dict):
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


_HANDLED = frozenset({"value", "unconvertible"})


def iter_odm_payloads(node, path, derive=False):
    """Yield (path, payload, derive) for every `odm` payload under a derivation.

    `derive` marks a payload inside an aggregate, whose derive step
    evaluates per record of another relation and so has no row scope.
    Literal values and mapping dictionaries are data, never expressions.
    """
    if isinstance(node, dict) and "value" in node and set(node) <= _HANDLED:
        # REQ-0358: the handled wrapper is named only where it was written.
        suffix = ".value" if "unconvertible" in node else ""
        yield from iter_odm_payloads(node["value"], f"{path}{suffix}", derive)
        return
    if isinstance(node, dict):
        if len(node) == 1:
            ((operation, payload),) = node.items()
            if operation == "odm":
                yield f"{path}.odm", payload, derive
                return
            if operation == "literal":
                return
            if operation == "mapping" and isinstance(payload, dict):
                payload = {k: v for k, v in payload.items() if k != "dict"}
            inner = derive or operation == "aggregate"
            yield from iter_odm_payloads(payload, f"{path}.{operation}", inner)
            return
        for key, value in node.items():
            yield from iter_odm_payloads(value, f"{path}.{key}", derive)
    elif isinstance(node, list):
        for index, value in enumerate(node):
            yield from iter_odm_payloads(value, f"{path}[{index}]", derive)


def odm_sites(spec, row_path=None):
    """Every `odm` payload a specification writes, in declaration order.

    Each site is (path, location, owner, payload). The location is
    `column`, `row`, `intermediate`, or `derive`; the owner is the column
    name, the row template's index, or the intermediate's index.
    `row_path(index, row)` spells a row template's path, `rows[index]` by
    default.
    """
    if row_path is None:

        def row_path(index, row):
            return f"rows[{index}]"

    sites = []

    def add(derivation, path, location, owner):
        for site_path, payload, derive in iter_odm_payloads(derivation, path):
            sites.append((site_path, "derive" if derive else location, owner, payload))

    for column in spec.get("columns") or []:
        if isinstance(column, dict) and "derivation" in column:
            name = column.get("name")
            add(column["derivation"], f"columns.{name}.derivation", "column", name)
    for index, row in enumerate(spec.get("rows") or []):
        if isinstance(row, dict) and isinstance(row.get("derivations"), dict):
            prefix = f"{row_path(index, row)}.derivations"
            for name, derivation in row["derivations"].items():
                add(derivation, f"{prefix}.{name}", "row", index)
    for index, intermediate in enumerate(spec.get("intermediates") or []):
        if isinstance(intermediate, dict) and isinstance(
            intermediate.get("derivations"), dict
        ):
            prefix = f"intermediates[{index}].derivations"
            for name, derivation in intermediate["derivations"].items():
                add(derivation, f"{prefix}.{name}", "intermediate", index)
    return sites


def odm_inputs(spec):
    """The declared inputs an `odm` read names: the ODM inputs (REQ-1266)."""
    declared = spec.get("input")
    if not isinstance(declared, dict):
        return frozenset()
    reads = (parse_odm_read(payload) for _, _, _, payload in odm_sites(spec))
    return frozenset(
        read.dataset for read in reads if read is not None and read.dataset in declared
    )
