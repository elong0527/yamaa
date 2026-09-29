"""CSV source reading and artifact writing (storage/csv), Parquet source
reading and artifact writing (storage/parquet)."""

from .errors import YamaaError
from .values import (
    YDate,
    YDateTime,
    date_text,
    datetime_text,
    float_text,
    is_missing,
    parse_date,
    parse_datetime,
    parse_float_text,
    parse_int_text,
)


def _phase_condition(phase, condition, **kw):
    return YamaaError(phase=phase, condition=condition, **kw)


def _profile_failure(condition, record, field, spec_path, dataset, written_path):
    """REQ-0851: a CSV profile failure names the dataset, the path as
    written, and the record and field where it was decided, counted from
    one with the header as record one. It is reported at `input.X.path`."""
    return _phase_condition(
        "ingest",
        condition,
        requirement="REQ-0029" if condition == "invalid_text" else "REQ-0851",
        spec_paths=[f"{spec_path}.path"],
        context={
            "dataset": dataset,
            "path": written_path,
            "record": record,
            "field": field,
        },
    )


def _coordinates(prefix):
    """The record and field a reader stands at after reading `prefix`
    (REQ-0853): a quoted delimiter or terminator does not advance them."""
    record, field, quoted = 1, 1, False
    for c in prefix:
        if c == '"':
            quoted = not quoted
        elif quoted:
            continue
        elif c == ",":
            field += 1
        elif c == "\n":
            record, field = record + 1, 1
    return record, field


def _scan_records(text, path, written_path, spec_path, dataset):
    """Split text into records per REQ-0838/22. Raises YamaaError with the
    record/field where a quoting or carriage-return failure was decided."""

    def fail(condition, record, field):
        raise _profile_failure(
            condition, record, field, spec_path, dataset, written_path
        )

    records, rec, field = [], [], []
    state = "bare"  # bare | quoted | after_quote
    lineno, fieldno = 1, 1
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if state == "bare":
            if c == '"':
                if field:
                    fail("source_quote_in_bare_field", lineno, fieldno)
                state = "quoted"
            elif c == ",":
                rec.append("".join(field))
                field, fieldno = [], fieldno + 1
            elif c == "\r":
                if i + 1 < n and text[i + 1] == "\n":
                    rec.append("".join(field))
                    records.append(rec)
                    rec, field, fieldno = [], [], 1
                    lineno += 1
                    i += 2
                    continue
                fail("source_carriage_return", lineno, fieldno)
            elif c == "\n":
                rec.append("".join(field))
                records.append(rec)
                rec, field, fieldno = [], [], 1
                lineno += 1
            else:
                field.append(c)
        elif state == "quoted":
            if c == '"':
                if i + 1 < n and text[i + 1] == '"':
                    field.append('"')
                    i += 2
                    continue
                state = "after_quote"
            elif c == "\r":
                # storage/csv: inside quotes no U+000D begins a record terminator.
                fail("source_carriage_return", lineno, fieldno)
            else:
                field.append(c)
        else:  # after_quote
            if c == ",":
                rec.append("".join(field))
                field, fieldno, state = [], fieldno + 1, "bare"
            elif c == "\n":
                rec.append("".join(field))
                records.append(rec)
                rec, field, fieldno, state = [], [], 1, "bare"
                lineno += 1
            elif c == "\r":
                if i + 1 < n and text[i + 1] == "\n":
                    rec.append("".join(field))
                    records.append(rec)
                    rec, field, fieldno, state = [], [], 1, "bare"
                    lineno += 1
                    i += 2
                    continue
                fail("source_carriage_return", lineno, fieldno)
            else:
                fail("source_text_after_quote", lineno, fieldno)
        i += 1
    if state == "quoted":
        fail("source_quote_unterminated", lineno, fieldno)
    if field or rec:
        rec.append("".join(field))
        records.append(rec)
    return records


def read_csv(
    path, types, spec_path="<input>", dataset="<input>", written_path=None, raw=None
):
    """Read a delimited source per storage/csv. types: field -> column_type.
    `spec_path` is the input's declaration, `input.X`. raw, when given, is
    the snapshot of the bytes stored at path."""
    written_path = written_path if written_path is not None else path

    def fail(condition, record, field):
        raise _profile_failure(
            condition, record, field, spec_path, dataset, written_path
        )

    if raw is None:
        with open(path, "rb") as f:
            raw = f.read()
    if raw.startswith(b"\xef\xbb\xbf"):
        fail("source_byte_order_mark", 1, 1)
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as e:
        # REQ-0853: report where the reader had reached in the valid prefix.
        fail("invalid_text", *_coordinates(raw[: e.start].decode("utf-8")))
    # REQ-0838: records split on \n, optional preceding \r; final record may omit terminator.
    rows = _scan_records(text, path, written_path, spec_path, dataset)
    if not rows:
        fail("source_header_absent", 1, 1)
    header = rows[0]
    seen = set()
    for n, h in enumerate(header, start=1):
        if h == "":
            fail("source_field_name_empty", 1, n)
        if h in seen:
            fail("source_field_name_duplicate", 1, h)
        seen.add(h)
    records = []
    for lineno, row in enumerate(rows[1:], start=2):
        if len(row) != len(header):
            # The field where the width was decided: the first extra one,
            # or the first one the record lacks.
            fail("source_record_width", lineno, min(len(row), len(header)) + 1)
        rec = {}
        for name, cell in zip(header, row):
            t = types.get(name, "str")
            rec[name] = _parse_cell(cell, t, name, spec_path, dataset)
        records.append(rec)
    return header, records


def _parse_cell(cell, t, name, spec_path, dataset="<input>"):
    if cell == "":
        return None  # REQ-0845: an empty field is missing for every type
    try:
        if t == "str":
            return cell
        if t == "int":
            return parse_int_text(cell)
        if t == "float":
            return parse_float_text(cell)
        if t == "date":
            return parse_date(cell)
        if t == "datetime":
            return parse_datetime(cell)
    except ValueError:
        # REQ-0536: the stored text does not parse under the declared type.
        raise _phase_condition(
            "ingest",
            "field_parse_failed",
            requirement="REQ-0536",
            spec_paths=[f"{spec_path}.types.{name}"],
            context={"dataset": dataset, "field": name, "type": t, "value": cell},
        )
    raise _phase_condition(
        "validation",
        "invalid_field_type",
        requirement="REQ-0012",
        spec_paths=[f"{spec_path}.types.{name}"],
        context={"field": name, "type": t},
    )


def write_csv_text(columns, rows, col_types, decimals=None):
    """Render the csv artifact per storage/csv. rows: list of dicts."""
    lines = [_csv_record(columns)]
    for row in rows:
        fields = [
            _field_text(row.get(c), col_types.get(c, "str"), decimals) for c in columns
        ]
        lines.append(_csv_record(fields))
    return "".join(line + "\n" for line in lines)


def _csv_record(fields):
    out = []
    for f in fields:
        if any(c in f for c in '",\r\n'):
            # REQ-0728/15: quote when containing '"', ',', CR, LF.
            # REQ-0731: missing renders as no characters, unquoted.
            out.append('"' + f.replace('"', '""') + '"')
        else:
            out.append(f)
    return ",".join(out)


def _field_text(v, t, decimals):
    if is_missing(v):
        return ""  # REQ-0731: missing is no characters, unquoted
    if t == "str":
        return v
    if t == "int":
        return str(v)
    if t == "float":
        if decimals is None:
            return float_text(v)
        return _fixed_point(v, decimals)
    if t == "date":
        return date_text(v)
    if t == "datetime":
        return datetime_text(v)
    raise YamaaError(
        phase="validation",
        condition="invalid_field_type",
        requirement="REQ-0012",
        context={"type": t},
    )


def _fixed_point(x, n):
    """REQ-0747/34: exact scaling, tie away from zero, no host rounding."""
    from decimal import ROUND_HALF_UP, Decimal

    d = Decimal(x)  # exact binary64 value
    q = Decimal(10) ** n
    scaled = (d * q).to_integral_value(rounding=ROUND_HALF_UP)
    if n == 0:
        return str(scaled)
    neg = scaled < 0
    s = str(abs(scaled)).rjust(n + 1, "0")
    return ("-" if neg else "") + s[:-n] + "." + s[-n:]


def _parquet_kinds():
    """REQ-1032/REQ-1033: the closed, exact Parquet -> column_type mapping."""
    import pyarrow as pa

    return {
        pa.string(): "str",
        pa.large_string(): "str",
        pa.int64(): "int",
        pa.float64(): "float",
        pa.date32(): "date",
        pa.timestamp("us"): "datetime",
    }


def _parquet_invalid(exc, spec_path, dataset, written_path):
    return _phase_condition(
        "ingest",
        "source_parquet_invalid",
        requirement="REQ-1038",
        spec_paths=[f"{spec_path}.path"],
        context={"dataset": dataset, "path": written_path, "error": str(exc)},
    )


def parquet_field_types(
    path, spec_path="<input>", dataset="<input>", written_path=None
):
    """Each stored field and its column type, None where the closed mapping
    has none, read from the Parquet schema without reading a record."""
    import pyarrow.parquet as pq

    written_path = written_path if written_path is not None else path
    try:
        schema = pq.read_schema(path)
    except Exception as exc:  # noqa: BLE001 -- any read failure is REQ-1038
        raise _parquet_invalid(exc, spec_path, dataset, written_path)
    kinds = _parquet_kinds()
    return [(f.name, kinds.get(f.type)) for f in schema]


def read_parquet(
    path, types, spec_path="<input>", dataset="<input>", written_path=None, select=None
):
    """Read a Parquet source per the Parquet profile. Returns the fields, the
    records, and each field's type from the Parquet schema (REQ-0517).
    `select` reads only the named fields, so no other field is typed."""
    import datetime as _datetime

    import pyarrow.parquet as pq

    written_path = written_path if written_path is not None else path
    try:
        table = pq.read_table(path, columns=select)
    except Exception as exc:  # noqa: BLE001 -- any read failure is REQ-1038
        raise _parquet_invalid(exc, spec_path, dataset, written_path)
    fields = table.schema.names
    kinds = _parquet_kinds()
    ftypes = {}
    for f in table.schema:
        if f.type not in kinds:
            raise _phase_condition(
                "ingest",
                "source_field_type_unsupported",
                requirement="REQ-1040",
                spec_paths=[f"{spec_path}.path"],
                context={"dataset": dataset, "field": f.name, "type": str(f.type)},
            )
        ftypes[f.name] = kinds[f.type]
    records = []
    cols = {name: table.column(name).to_pylist() for name in fields}
    for i in range(table.num_rows):
        rec = {}
        for name in fields:
            v = cols[name][i]
            if v is None:
                rec[name] = None  # REQ-1034: null is missing
            elif isinstance(v, float) and (
                v != v  # noqa: PLR0124 -- NaN check is the intent
                or v in (float("inf"), float("-inf"))
            ):
                rec[name] = None  # REQ-1035: non-finite -> missing
            elif isinstance(v, _datetime.datetime):
                rec[name] = YDateTime(
                    v.year, v.month, v.day, v.hour, v.minute, v.second
                )
            elif isinstance(v, _datetime.date):
                rec[name] = YDate(v.year, v.month, v.day)
            else:
                rec[name] = v
        records.append(rec)
    return fields, records, ftypes


def write_parquet_bytes(columns, rows, col_types):
    """Render the parquet artifact per storage/parquet: one optional field
    per column under REQ-0734's mapping, a `datetime` on its own wall clock
    (REQ-0738), uncompressed and with no key-value metadata of its own
    (REQ-0741). rows: list of dicts."""
    import datetime as _datetime
    import io

    import pyarrow as pa
    import pyarrow.parquet as pq

    kinds = {
        "str": pa.string(),
        "int": pa.int64(),
        "float": pa.float64(),
        "date": pa.date32(),
        "datetime": pa.timestamp("us"),
    }

    def cell(v, t):
        if is_missing(v):
            return None
        if t == "datetime":
            # REQ-0738: a wall-clock reading, so no zone is attached.
            return _datetime.datetime(  # noqa: DTZ001
                v.year, v.month, v.day, v.hour, v.minute, v.second
            )
        if t == "date":
            return _datetime.date(v.year, v.month, v.day)
        return v

    fields = [pa.field(c, kinds[col_types[c]]) for c in columns]
    arrays = [
        pa.array([cell(r.get(c), col_types[c]) for r in rows], type=f.type)
        for c, f in zip(columns, fields)
    ]
    buf = io.BytesIO()
    pq.write_table(
        pa.Table.from_arrays(arrays, schema=pa.schema(fields)),
        buf,
        compression="none",
        store_schema=False,
    )
    return buf.getvalue()
