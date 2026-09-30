"""Authoring lints that read the specification value, not only its layout.

REQ-1288, REQ-1289, and REQ-1290 name departures that leave the parsed
value unchanged only when the author rewrites them: a uniform row-template
derivation that belongs at column level, a named intermediate equivalent
to the implicit join, and an output column without a label. A finding is
not a validation diagnostic; the specification stays valid and runs the
same way. No fix is offered: moving a derivation, removing an
intermediate, or writing a label changes the written text of a value, so
its author corrects it.
"""

from __future__ import annotations

from collections.abc import Iterator, Mapping, Sequence

import yaml

from yamaa.specification._yaml import read_yaml_bytes
from yamaa.specification.diagnostics import SpecificationError
from yamaa.style._checks import Departure
from yamaa.style._document import Document, member


def _filter_identifiers(predicate: object) -> frozenset[str]:
    """Bare identifiers a row filter reads; empty when unparsable."""
    if not isinstance(predicate, str):
        return frozenset()
    try:
        from yamaa.expressions import parse_predicate, predicate_identifiers
    except ImportError:
        return frozenset()
    try:
        return frozenset(predicate_identifiers(parse_predicate(predicate)))
    except ValueError:
        return frozenset()


_DATASET_LEVEL_OPERATIONS = frozenset(
    {
        "aggregate",
        "row_number",
        "rank",
        "row_value",
        "previous_non_missing",
        "locf",
        "baseline_flag",
    }
)


def spec_value(document: Document) -> dict | None:
    """The engine-reader value of `document`, or None when it has none."""
    try:
        value = read_yaml_bytes(document.text.encode("utf-8"), "<style>")
    except SpecificationError:
        return None
    return value if isinstance(value, dict) else None


def _values_equal(first: object, second: object) -> bool:
    """Type-sensitive equality: 1, 1.0, and true stay apart."""
    if type(first) is not type(second):
        return False
    if isinstance(first, dict) and isinstance(second, dict):
        if set(first) != set(second):
            return False
        return all(_values_equal(first[key], second[key]) for key in first)
    if isinstance(first, list) and isinstance(second, list):
        return len(first) == len(second) and all(
            _values_equal(a, b) for a, b in zip(first, second)
        )
    return first == second


def _walk_strings(value: object) -> Iterator[str]:
    if isinstance(value, str):
        yield value
    elif isinstance(value, Mapping):
        for key, item in value.items():
            yield from _walk_strings(key)
            yield from _walk_strings(item)
    elif isinstance(value, Sequence) and not isinstance(value, (bytes, bytearray)):
        for item in value:
            yield from _walk_strings(item)


def _uses_dataset_operation(value: object) -> bool:
    if isinstance(value, Mapping):
        for key, item in value.items():
            if key in _DATASET_LEVEL_OPERATIONS:
                return True
            if _uses_dataset_operation(item):
                return True
    elif isinstance(value, Sequence) and not isinstance(value, (str, bytes, bytearray)):
        return any(_uses_dataset_operation(item) for item in value)
    return False


def _reads_intermediate(value: object, intermediate_ids: frozenset[str]) -> bool:
    if not intermediate_ids:
        return False
    for text in _walk_strings(value):
        for identifier in intermediate_ids:
            if f"{identifier}." in text:
                return True
    return False


def _is_literal(value: object) -> bool:
    """True when `value` is a literal derivation, driver-independent."""
    return isinstance(value, Mapping) and set(value) == {"literal"}


def _blocked_columns(
    columns: Sequence[Mapping], intermediate_ids: frozenset[str]
) -> frozenset[str]:
    """Column names whose column-level derivations are not row-local.

    REQ-1260: a column-level derivation is row-local unless it uses a
    dataset-level operation, reads a named intermediate, or reads another
    column-level column that is not row-local.
    """
    reads: dict[str, frozenset[str]] = {}
    blocked: set[str] = set()
    for entry in columns:
        if not isinstance(entry, Mapping):
            continue
        name = entry.get("name")
        derivation = entry.get("derivation")
        if not isinstance(name, str) or derivation is None:
            continue
        if _uses_dataset_operation(derivation) or _reads_intermediate(
            derivation, intermediate_ids
        ):
            blocked.add(name)
        bare = {
            text
            for text in _walk_strings(derivation)
            if isinstance(text, str) and "." not in text
        }
        reads[name] = frozenset(bare)
    changed = True
    while changed:
        changed = False
        for name, names in reads.items():
            if name not in blocked and names & blocked:
                blocked.add(name)
                changed = True
    return frozenset(blocked)


def _column_entries(value: Mapping) -> list[Mapping]:
    columns = value.get("columns")
    if not isinstance(columns, list):
        return []
    return [entry for entry in columns if isinstance(entry, Mapping)]


def _row_entries(value: Mapping) -> list[Mapping]:
    rows = value.get("rows")
    if not isinstance(rows, list):
        return []
    return [entry for entry in rows if isinstance(entry, Mapping)]


def _intermediate_entries(value: Mapping) -> list[Mapping]:
    intermediates = value.get("intermediates")
    if not isinstance(intermediates, list):
        return []
    return [entry for entry in intermediates if isinstance(entry, Mapping)]


def _sequence_node(document: Document, field: str) -> yaml.SequenceNode | None:
    node = member(document.root, field)
    return node if isinstance(node, yaml.SequenceNode) else None


def _mapping_at(
    sequence: yaml.SequenceNode | None, index: int
) -> yaml.MappingNode | None:
    if sequence is None or index >= len(sequence.value):
        return None
    item = sequence.value[index]
    return item if isinstance(item, yaml.MappingNode) else None


def _key_node(mapping: yaml.MappingNode | None, name: str) -> yaml.ScalarNode | None:
    if mapping is None:
        return None
    for key, _ in mapping.value:
        if isinstance(key, yaml.ScalarNode) and key.value == name:
            return key
    return None


def _value_node(mapping: yaml.MappingNode | None, name: str) -> yaml.Node | None:
    if mapping is None:
        return None
    for key, value in mapping.value:
        if isinstance(key, yaml.ScalarNode) and key.value == name:
            return value
    return None


def _column_name_key(
    document: Document, column_index: int
) -> tuple[int, int, str] | None:
    """(line, column, spec_path) of columns[i].name, for missing labels."""
    sequence = _sequence_node(document, "columns")
    mapping = _mapping_at(sequence, column_index)
    key = _key_node(mapping, "name")
    if key is not None:
        return key.start_mark.line, key.start_mark.column, f"columns[{column_index}]"
    if mapping is not None:
        return (
            mapping.start_mark.line,
            mapping.start_mark.column,
            f"columns[{column_index}]",
        )
    return None


def _row_derivation_key(
    document: Document, row_index: int, column: str
) -> tuple[int, int, str] | None:
    """(line, column, spec_path) of rows[i].derivations.<column>."""
    rows = _sequence_node(document, "rows")
    mapping = _mapping_at(rows, row_index)
    derivations = _value_node(mapping, "derivations")
    if not isinstance(derivations, yaml.MappingNode):
        return None
    for key, _ in derivations.value:
        if isinstance(key, yaml.ScalarNode) and key.value == column:
            return (
                key.start_mark.line,
                key.start_mark.column,
                f"rows[0].derivations.{column}",
            )
    return None


def _intermediate_id_key(
    document: Document, intermediate_index: int
) -> tuple[int, int, str] | None:
    """(line, column, spec_path) of intermediates[i].id."""
    sequence = _sequence_node(document, "intermediates")
    mapping = _mapping_at(sequence, intermediate_index)
    key = _key_node(mapping, "id")
    if key is not None:
        return (
            key.start_mark.line,
            key.start_mark.column,
            f"intermediates[{intermediate_index}]",
        )
    if mapping is not None:
        return (
            mapping.start_mark.line,
            mapping.start_mark.column,
            f"intermediates[{intermediate_index}]",
        )
    return None


def missing_labels(document: Document, value: Mapping) -> Iterator[Departure]:
    """REQ-1290: every output column declares `label:`."""
    if not isinstance(value.get("output"), Mapping):
        return
    if "parents" in value:
        # A child layer inherits labels; only a standalone file owns them.
        return
    output_columns = value["output"].get("columns")
    if not isinstance(output_columns, list):
        return
    entries = _column_entries(value)
    by_name: dict[str, int] = {}
    for index, entry in enumerate(entries):
        name = entry.get("name")
        if isinstance(name, str) and name not in by_name:
            by_name[name] = index
    for name in output_columns:
        if not isinstance(name, str) or name not in by_name:
            # Undeclared output columns fail validation; style stays silent.
            continue
        entry = entries[by_name[name]]
        label = entry.get("label")
        if isinstance(label, str) and label:
            continue
        position = _column_name_key(document, by_name[name])
        if position is None:
            continue
        line, column, spec_path = position
        yield Departure(
            "missing_label",
            line,
            column,
            spec_path,
            f"output column `{name}` has no label; declare `label:`",
        )


def repeated_row_derivations(document: Document, value: Mapping) -> Iterator[Departure]:
    """REQ-1288: a derivation identical in every row template belongs up top.

    Only row-local derivations are reported: dataset-level operations,
    named-intermediate reads, and reads of non-row-local columns stay in
    their row template, where the row phase evaluates them. A derivation
    reading a driver dataset stays as well when the row templates build
    from different datasets: the driver read and the column-phase implicit
    join bind different records. A column read by any row filter stays:
    filters resolve only columns their own template derives.
    """
    rows = _row_entries(value)
    if len(rows) < 2:
        return
    entries = _column_entries(value)
    column_derivations = {
        entry["name"]
        for entry in entries
        if isinstance(entry.get("name"), str) and entry.get("derivation") is not None
    }
    intermediate_ids = frozenset(
        entry["id"]
        for entry in _intermediate_entries(value)
        if isinstance(entry.get("id"), str)
    )
    blocked = _blocked_columns(entries, intermediate_ids)
    datasets = [row.get("dataset") for row in rows]
    uniform_driver = all(dataset == datasets[0] for dataset in datasets[1:])
    filter_reads: set[str] = set()
    for row in rows:
        filter_reads |= _filter_identifiers(row.get("filter"))
    for row in rows:
        derivations = row.get("derivations")
        if not isinstance(derivations, Mapping):
            return
    first = rows[0].get("derivations")
    assert isinstance(first, Mapping)
    for column in first:
        if not isinstance(column, str):
            continue
        if column in column_derivations:
            # A column-level default already exists; row overrides stay.
            continue
        if column in filter_reads:
            # Grouped and ungrouped row filters resolve only columns the
            # row template derives; a column-level default is not available.
            continue
        values = [row.get("derivations", {}).get(column, ...) for row in rows]
        if any(item is ... for item in values):
            continue
        if not all(_values_equal(values[0], item) for item in values[1:]):
            continue
        derivation = values[0]
        if _uses_dataset_operation(derivation) or _reads_intermediate(
            derivation, intermediate_ids
        ):
            continue
        if not uniform_driver and not _is_literal(derivation):
            # Different drivers bind different records; only a literal is
            # driver-independent and safe to hoist.
            continue
        if any(
            isinstance(text, str) and "." not in text and text in blocked
            for text in _walk_strings(derivation)
        ):
            continue
        position = _row_derivation_key(document, 0, column)
        if position is None:
            continue
        line, column_pos, spec_path = position
        yield Departure(
            "repeated_row_derivation",
            line,
            column_pos,
            spec_path,
            f"derivation of `{column}` is identical in every row template; "
            "move it to columns[].derivation",
        )


def _key_equals_output_keys(key: object, output_keys: Sequence[str]) -> bool:
    """True when an explicit key states exactly the output keys."""
    wanted = set(output_keys)
    if isinstance(key, str):
        return {key} == wanted
    if isinstance(key, list):
        return all(isinstance(item, str) for item in key) and set(key) == wanted
    if isinstance(key, Mapping):
        return set(key) == wanted and all(
            isinstance(item, str) and item == column for column, item in key.items()
        )
    return False


def _intermediate_is_read(value: Mapping, identifier: str) -> bool:
    for text in _walk_strings(value):
        if f"{identifier}." in text:
            return True
    return False


def redundant_intermediates(document: Document, value: Mapping) -> Iterator[Departure]:
    """REQ-1289: a named intermediate equivalent to the implicit join.

    A simple intermediate with no filter, selection, custom key, or handler
    that changes behavior reads what `DATASET.COLUMN` already reads. The
    check reports only explicit keys that restate the output keys; an
    omitted key with `no_match: null` fails validation as
    `rename_only_intermediate` instead, and an intermediate without
    `no_match` requires a match the implicit join cannot state.
    """
    keys = value.get("keys")
    if not isinstance(keys, list) or not all(isinstance(k, str) for k in keys):
        return
    intermediates = _intermediate_entries(value)
    raw_intermediates = value.get("intermediates")
    if not isinstance(raw_intermediates, list):
        return
    for index, entry in enumerate(intermediates):
        identifier = entry.get("id")
        dataset = entry.get("dataset")
        if not isinstance(identifier, str) or not isinstance(dataset, str):
            continue
        if dataset == "SELF":
            continue
        if any(
            entry.get(field) is not None
            for field in (
                "filter",
                "between",
                "order_by",
                "keep",
                "derivations",
                "verifications",
                "columns",
            )
        ):
            continue
        raw = (
            raw_intermediates[index]
            if index < len(raw_intermediates)
            and isinstance(raw_intermediates[index], Mapping)
            else {}
        )
        assert isinstance(raw, Mapping)
        if "no_match" not in raw or raw["no_match"] is not None:
            # Absent requires a match; a literal answers where the join
            # yields missing. Only `no_match: null` matches the join.
            continue
        key = entry.get("key")
        if key is None:
            # REQ-1248 owns the omitted-key rename; style stays silent.
            continue
        if not _key_equals_output_keys(key, keys):
            continue
        if not _intermediate_is_read(value, identifier):
            continue
        position = _intermediate_id_key(document, index)
        if position is None:
            continue
        line, column, spec_path = position
        yield Departure(
            "redundant_intermediate",
            line,
            column,
            spec_path,
            f"intermediate `{identifier}` reads exactly what the implicit "
            f"join to `{dataset}` reads; read `{dataset}` directly",
        )


def check_authoring(document: Document) -> list[Departure]:
    """Every REQ-1288 to REQ-1290 departure in `document`."""
    value = spec_value(document)
    if value is None:
        return []
    found: list[Departure] = []
    found.extend(missing_labels(document, value))
    found.extend(repeated_row_derivations(document, value))
    found.extend(redundant_intermediates(document, value))
    return sorted(found, key=lambda item: (item.line, item.column, item.name))
