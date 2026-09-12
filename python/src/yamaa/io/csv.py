"""The fixed CSV source profile from R023.

Quoting follows the Python csv-module default: double quotes escape,
surrounding whitespace is preserved, and a field with no characters is
missing whether it was bare or quoted.
"""

from __future__ import annotations

from pydantic import BaseModel, ConfigDict


class _FrozenModel(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class CsvSource(_FrozenModel):
    """An ordered header and records parsed from one CSV snapshot."""

    names: tuple[str, ...]
    records: tuple[tuple[str | None, ...], ...]


class CsvProfileFailure(ValueError):
    """One R019/R023 ingest failure at a source coordinate."""

    def __init__(self, condition: str, record: int, field: int | str) -> None:
        self.condition = condition
        self.record = record
        self.field = field
        super().__init__(f"{condition} at record {record}, field {field!r}")


def _coordinates(prefix: str) -> tuple[int, int]:
    record = 1
    field = 1
    index = 0
    while index < len(prefix):
        character = prefix[index]
        if character == '"':
            index += 1
            while index < len(prefix):
                if prefix[index] == '"':
                    if prefix[index + 1 : index + 2] == '"':
                        index += 2
                        continue
                    index += 1
                    break
                index += 1
            continue
        if character == ",":
            field += 1
        elif character == "\n":
            record += 1
            field = 1
        index += 1
    return record, field


def _parse_records(data: str) -> list[list[str | None]]:
    if not data:
        return []
    records: list[list[str | None]] = []
    record: list[str | None] = []
    index = 0
    number = 1
    field_number = 1
    while True:
        if data[index : index + 1] == '"':
            index += 1
            chunks: list[str] = []
            while True:
                if index >= len(data):
                    raise CsvProfileFailure(
                        "source_quote_unterminated", number, field_number
                    )
                character = data[index]
                if character == '"':
                    if data[index + 1 : index + 2] == '"':
                        chunks.append('"')
                        index += 2
                        continue
                    index += 1
                    break
                if character == "\r":
                    raise CsvProfileFailure(
                        "source_carriage_return", number, field_number
                    )
                chunks.append(character)
                index += 1
            field: str | None = "".join(chunks) or None
            if index < len(data) and data[index] not in ",\r\n":
                raise CsvProfileFailure("source_text_after_quote", number, field_number)
        else:
            start = index
            while index < len(data) and data[index] not in ",\n":
                character = data[index]
                if character == '"':
                    raise CsvProfileFailure(
                        "source_quote_in_bare_field", number, field_number
                    )
                if character == "\r":
                    break
                index += 1
            field = data[start:index] or None
        record.append(field)
        if index >= len(data):
            records.append(record)
            break
        if data[index] == ",":
            index += 1
            field_number += 1
            continue
        if data[index] == "\r":
            if data[index + 1 : index + 2] != "\n":
                raise CsvProfileFailure("source_carriage_return", number, field_number)
            index += 2
        else:
            index += 1
        records.append(record)
        record = []
        number += 1
        field_number = 1
        if index >= len(data):
            break
    return records


def parse_csv(content: bytes) -> CsvSource:
    """Parse one immutable byte snapshot under the closed CSV profile."""
    try:
        data = content.decode("utf-8")
    except UnicodeDecodeError as error:
        prefix = content[: error.start].decode("utf-8")
        record, field = _coordinates(prefix)
        raise CsvProfileFailure("invalid_text", record, field) from error
    if data.startswith("\ufeff"):
        raise CsvProfileFailure("source_byte_order_mark", 1, 1)

    records = _parse_records(data)
    if not records:
        raise CsvProfileFailure("source_header_absent", 1, 1)

    header = records[0]
    names: list[str] = []
    seen: set[str] = set()
    for index, name in enumerate(header, 1):
        if not name:
            raise CsvProfileFailure("source_field_name_empty", 1, index)
        if name in seen:
            raise CsvProfileFailure("source_field_name_duplicate", 1, name)
        seen.add(name)
        names.append(name)

    for number, record in enumerate(records[1:], 2):
        if len(record) != len(header):
            field = min(len(record), len(header)) + 1
            raise CsvProfileFailure("source_record_width", number, field)

    return CsvSource(
        names=tuple(names),
        records=tuple(tuple(record) for record in records[1:]),
    )
