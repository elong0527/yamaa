from __future__ import annotations

import pytest

from yamaa.io.csv import CsvProfileFailure, parse_csv, record_coordinates


def test_quoted_and_bare_empty_are_missing_records_keep_order() -> None:
    source = parse_csv(
        b"ID,EMPTY,COMMENT,SPACE\r\n"
        b'007,,"line one\nline two, still one field", kept \r\n'
        b'008,"","said ""hello""",next'
    )

    assert source.names == ("ID", "EMPTY", "COMMENT", "SPACE")
    assert source.records[0] == (
        "007",
        None,
        "line one\nline two, still one field",
        " kept ",
    )
    assert source.records[1] == ("008", None, 'said "hello"', "next")
    assert [record[0] for record in source.records] == ["007", "008"]


@pytest.mark.parametrize(
    ("content", "condition", "record", "field"),
    [
        (b"", "source_header_absent", 1, 1),
        (b"\xef\xbb\xbfID\n1\n", "source_byte_order_mark", 1, 1),
        (b"ID,\n1,2\n", "source_field_name_empty", 1, 2),
        (b"ID,ID\n1,2\n", "source_field_name_duplicate", 1, "ID"),
        (b"A,B\n1,2,3\n", "source_record_width", 2, 3),
        (b"A,B,C\n1,2\n", "source_record_width", 2, 3),
        (b'A,B\n1,"x\n', "source_quote_unterminated", 2, 2),
        (b'A,B\n1,x"y\n', "source_quote_in_bare_field", 2, 2),
        (b'A,B\n1,"x"y\n', "source_text_after_quote", 2, 2),
        (b"A,B\n1,x\ry\n", "source_carriage_return", 2, 2),
        (b'A,B\n1,"x\ry"\n', "source_carriage_return", 2, 2),
        (b"A,B\n1,x\r", "source_carriage_return", 2, 2),
        (b"A,B\n1,ok\n2,\xf4bad\n", "invalid_text", 3, 2),
    ],
)
def test_rejects_nonconforming_csv_at_exact_coordinate(
    content: bytes,
    condition: str,
    record: int,
    field: int | str,
) -> None:
    with pytest.raises(CsvProfileFailure) as raised:
        parse_csv(content)

    assert raised.value.condition == condition
    assert raised.value.record == record
    assert raised.value.field == field


@pytest.mark.parametrize(
    "content",
    [b"A,B\n1,2\n", b"A,B\r\n1,2\r\n", b"A,B\n1,2"],
)
def test_admitted_record_terminators_produce_identical_records(
    content: bytes,
) -> None:
    source = parse_csv(content)
    assert [list(record) for record in source.records] == [["1", "2"]]


def test_header_only_source_is_valid() -> None:
    source = parse_csv(b"A,B\n")
    assert source.names == ("A", "B")
    assert source.records == ()


@pytest.mark.parametrize(
    ("prefix", "record", "field"),
    [
        # The reader stands at record one, field one before any character.
        ("", 1, 1),
        # A comma before any newline still counts fields of the first record.
        ("A,", 1, 2),
        # Newlines reset the field count; the standing field is the next one.
        ("A,B\n1,", 2, 2),
        ("A\nB\n", 3, 1),
        # Quoting decides how a field is read: commas and newlines inside a
        # quoted field do not move the coordinates, and a doubled quote does
        # not close the field.
        ('A,"x,y",', 1, 3),
        ('A,"x""y",', 1, 3),
        ('A,"x\ny",', 1, 3),
        # An unterminated quoted field keeps the reader inside it.
        ('A,"x', 1, 2),
        # A quoted field that opens with an escaped quote still closes at
        # the right quote: the first content character is examined, never
        # skipped.
        ('"",x\n', 2, 1),
        # A tab is field content, not a record terminator.
        ("A\tB", 1, 1),
    ],
)
def test_record_coordinates_skip_quoted_fields(
    prefix: str, record: int, field: int
) -> None:
    assert record_coordinates(prefix) == (record, field)


@pytest.mark.parametrize(
    ("content", "record", "field"),
    [
        # The standing field before any newline is field one, not zero.
        (b"\xff", 1, 1),
        (b"A,\xff", 1, 2),
    ],
)
def test_invalid_text_in_first_record_names_exact_coordinate(
    content: bytes, record: int, field: int
) -> None:
    with pytest.raises(CsvProfileFailure) as raised:
        parse_csv(content)

    assert raised.value.condition == "invalid_text"
    assert raised.value.record == record
    assert raised.value.field == field


@pytest.mark.parametrize(
    ("content", "record", "field"),
    [
        # A scan failure in the first record names field one, not zero.
        (b'"abc', 1, 1),
        (b'a"b', 1, 1),
        # The width diagnostic names one past the shorter side, whatever the
        # shorter side is: min(1, 3) + 1 and min(5, 3) + 1, not just min + 1
        # when the minimum happens to be two.
        (b"A,B,C\n1\n", 2, 2),
        (b"A,B,C\n1,2,3,4,5\n", 2, 4),
        (b"A\n1,2,3\n", 2, 2),
    ],
)
def test_first_record_failures_name_fields_from_one(
    content: bytes, record: int, field: int
) -> None:
    with pytest.raises(CsvProfileFailure) as raised:
        parse_csv(content)

    assert raised.value.record == record
    assert raised.value.field == field


def test_bare_field_keeps_control_characters_below_carriage_return() -> None:
    # Only U+000D itself ends a bare field early; lesser control characters
    # are content. (U+000A still terminates the record around them.)
    source = parse_csv(b"A\n\x0b\n")
    assert source.records == (("\x0b",),)


def test_long_fields_do_not_trip_identity_comparisons() -> None:
    # Lengths past the small-int cache must still compare by value: a field
    # longer than 256 characters parses, and an unterminated quote that long
    # still raises instead of indexing past the end.
    long_field = "x" * 300
    source = parse_csv(f"A\n{long_field}\n".encode())
    assert source.records == ((long_field,),)
    # The final record may omit its terminator, however long the field is.
    source = parse_csv(f"A\n{long_field}".encode())
    assert source.records == ((long_field,),)

    with pytest.raises(CsvProfileFailure) as raised:
        parse_csv(f'A\n"{long_field}'.encode())
    assert raised.value.condition == "source_quote_unterminated"
