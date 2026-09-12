from __future__ import annotations

import pytest

from yamaa.ingest import CsvProfileFailure, parse_csv


def test_preserves_quote_provenance_embedded_values_and_record_order() -> None:
    source = parse_csv(
        b"ID,EMPTY,COMMENT,SPACE\r\n"
        b'007,,"line one\nline two, still one field", kept \r\n'
        b'008,"","said ""hello""",next'
    )

    assert source.names == ("ID", "EMPTY", "COMMENT", "SPACE")
    assert [field.text for field in source.records[0]] == [
        "007",
        None,
        "line one\nline two, still one field",
        " kept ",
    ]
    assert source.records[0][1].quoted is False
    assert source.records[1][1].text == ""
    assert source.records[1][1].quoted is True
    assert source.records[1][2].text == 'said "hello"'
    assert [record[0].text for record in source.records] == ["007", "008"]


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
    assert [[field.text for field in record] for record in source.records] == [
        ["1", "2"]
    ]


def test_header_only_source_is_valid() -> None:
    source = parse_csv(b"A,B\n")
    assert source.names == ("A", "B")
    assert source.records == ()
