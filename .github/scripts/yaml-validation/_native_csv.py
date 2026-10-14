"""Repository fixture policy over the installed public native source/codec lifecycle.

The shared engine admits CSV syntax and formats typed scalar values. Python's CSV
module only retains already admitted fixture cells and checks repository layout.
"""

import csv
import io
import json
from functools import lru_cache
from pathlib import Path
from tempfile import TemporaryDirectory

from yamaa import domain

SYNTAX_CONDITIONS = {
    "source_quote_unterminated",
    "source_quote_in_bare_field",
    "source_text_after_quote",
    "source_carriage_return",
    "invalid_text",
    "source_byte_order_mark",
}
STRUCTURE_CONDITIONS = {
    "source_header_absent",
    "source_field_name_empty",
    "source_field_name_duplicate",
    "source_record_width",
}


class CsvProfileFailure(ValueError):
    def __init__(self, condition, record, field):
        super().__init__(f"{condition} at record {record}, field {field}")
        self.condition = condition
        self.record = record
        self.field = field


def _specification(columns, *, profile=False, decimals=None):
    if profile:
        source = {"path": "source.csv", "ordinal": "ValidatorOrdinal"}
        fields = [("ROWID", "int", "SOURCE.ValidatorOrdinal")]
    else:
        source = {"path": "source.csv", "types": {"VALUE": columns[0]}}
        fields = [("ID", "str", "SOURCE.ID"), ("VALUE", columns[0], "SOURCE.VALUE")]
    output = {"path": "result.csv", "columns": [name for name, _, _ in fields]}
    if decimals is not None:
        output["decimals"] = decimals
    return {
        "schema_version": "1.0",
        "domain": "VALIDATOR",
        "keys": [fields[0][0]],
        "input": {"SOURCE": source},
        "output": output,
        "columns": [
            {"name": name, "type": kind, "label": name, "derivation": variable}
            for name, kind, variable in fields
        ],
    }


def profile_issues(raw):
    """The exact source refusal from an owned native build, without publication."""
    with TemporaryDirectory(prefix="yamaa-validator-source-") as temporary:
        root = Path(temporary)
        (root / "source.csv").write_bytes(raw)
        specification = root / "spec.yaml"
        specification.write_text(
            json.dumps(_specification([], profile=True)), encoding="ascii"
        )
        result = domain(specification)
        rows = result.issues.to_dicts()
        if len(rows) == 1 and rows[0]["condition"] == "ordinal_field_collision":
            # Complete syntax, header and width admission precedes the ordinal.
            return []
        return rows


def source_issue(raw):
    rows = profile_issues(raw)
    if not rows:
        return None
    if (
        len(rows) == 1
        and rows[0]["condition"] in SYNTAX_CONDITIONS | STRUCTURE_CONDITIONS
    ):
        row = rows[0]
        return row["condition"], json.loads(row["context"])
    raise RuntimeError(f"native fixture source refused: {rows!r}")


def records(text):
    """Read admitted cells; collected blanks follow the input convention."""
    issue = source_issue(text.encode("utf-8"))
    if issue is not None and issue[0] in SYNTAX_CONDITIONS:
        condition, context = issue
        raise CsvProfileFailure(condition, context["record"], context["field"])
    return [
        [None if value == "" else value for value in (record or [""])]
        for record in csv.reader(io.StringIO(text, newline=""), strict=True)
    ]


def coordinates(text):
    """Use the shared source decoder's original-byte coordinate calculation."""
    condition, context = source_issue(text.encode("utf-8") + b"\xff")
    assert condition == "invalid_text"
    return context["record"], context["field"]


def render_records(header, rows):
    """Repository quoting/layout policy for missing-or-text fixture cells."""
    output = io.StringIO(newline="")
    writer = csv.writer(output, lineterminator="\n")
    for row in [header, *rows]:
        writer.writerow([] if row == [None] else row)
    return output.getvalue().encode("utf-8")


@lru_cache(maxsize=65536, typed=True)
def scalar_text(value, kind, decimals=None):
    """Canonical text retained and explicitly saved by the native codec."""
    with TemporaryDirectory(prefix="yamaa-validator-scalar-") as temporary:
        root = Path(temporary)
        source = io.StringIO(newline="")
        writer = csv.writer(source, lineterminator="\n")
        writer.writerow(["ID", "VALUE"])
        writer.writerow(["1", value])
        (root / "source.csv").write_text(
            source.getvalue(), encoding="utf-8", newline=""
        )
        specification = root / "spec.yaml"
        specification.write_text(
            json.dumps(_specification([kind], decimals=decimals)), encoding="ascii"
        )
        result = domain(specification)
        rows = result.issues.to_dicts()
        if rows:
            raise ValueError(f"native scalar fixture refused: {rows!r}")
        result.save()
        with (root / "result.csv").open(encoding="utf-8", newline="") as stream:
            rendered = list(csv.reader(stream))
        return rendered[1][1]
