from __future__ import annotations

import pytest

from yamaa.io.csv import CsvProfileFailure, parse_csv


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
