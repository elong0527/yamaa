"""CSV source reading (R023) and artifact writing (R020, csv profile)."""

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


def _scan_records(text, path, written_path, spec_path, dataset):
    """Split text into records per R023-9/22. Raises YamaaError with the
    record/field where a quoting or carriage-return failure was decided."""

    def fail(condition, record, field):
        raise _phase_condition(
            "ingest",
            condition,
            requirement="R023-22",
            spec_paths=[spec_path],
            context={
                "dataset": dataset,
                "path": written_path,
                "record": record,
                "field": field,
            },
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
                # R023: inside quotes no U+000D begins a record terminator.
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


def read_csv(path, types, spec_path="<input>", dataset="<input>", written_path=None):
    """Read a delimited source per R023. types: field -> column_type."""
    written_path = written_path if written_path is not None else path
    with open(path, "rb") as f:
        raw = f.read()
    if raw.startswith(b"\xef\xbb\xbf"):
        raise _phase_condition(
            "ingest",
            "source_byte_order_mark",
            requirement="R023-22",
            spec_paths=[spec_path],
            context={"dataset": dataset, "path": written_path},
        )
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as e:
        raise _phase_condition(
            "ingest",
            "invalid_text",
            requirement="R023-7",
            spec_paths=[spec_path],
            context={"dataset": dataset, "path": written_path, "detail": str(e)},
        )
    # R023-9: records split on \n, optional preceding \r; final record may omit terminator.
    rows = _scan_records(text, path, written_path, spec_path, dataset)
    if not rows:
        raise _phase_condition(
            "ingest",
            "source_header_absent",
            requirement="R023-22",
            spec_paths=[spec_path],
            context={"dataset": dataset, "path": written_path},
        )
    header = rows[0]
    if any(h == "" for h in header):
        raise _phase_condition(
            "ingest",
            "source_field_name_empty",
            requirement="R023-13",
            spec_paths=[spec_path],
            context={"dataset": dataset, "path": written_path},
        )
    if len(set(header)) != len(header):
        raise _phase_condition(
            "ingest",
            "source_field_name_duplicate",
            requirement="R023-14",
            spec_paths=[spec_path],
            context={"dataset": dataset, "path": written_path},
        )
    records = []
    for lineno, row in enumerate(rows[1:], start=2):
        if len(row) != len(header):
            raise _phase_condition(
                "ingest",
                "source_record_width",
                requirement="R023-15",
                spec_paths=[spec_path],
                context={"dataset": dataset, "path": written_path, "record": lineno},
            )
        rec = {}
        for name, cell in zip(header, row):
            t = types.get(name, "str")
            rec[name] = _parse_cell(
                cell, t, written_path, lineno, name, spec_path, dataset
            )
        records.append(rec)
    return header, records


def _parse_cell(cell, t, path, lineno, name, spec_path, dataset="<input>"):
    if cell == "":
        return None  # R023-16: an empty field is missing for every type
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
    except ValueError as e:
        raise _phase_condition(
            "ingest",
            "field_parse_failed",
            requirement="R014-13",
            spec_paths=[spec_path],
            context={
                "dataset": dataset,
                "path": path,
                "line": lineno,
                "field": name,
                "detail": str(e),
            },
        )
    raise _phase_condition(
        "ingest",
        "invalid_field_type",
        requirement="R006",
        spec_paths=[spec_path],
        context={"field": name, "type": t},
    )


def write_csv_text(columns, rows, col_types, decimals=None):
    """Render the csv artifact per R020. rows: list of dicts. Returns str."""
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
            # R020-14/15: quote when containing '"', ',', CR, LF.
            # R020-17: missing renders as no characters, unquoted.
            out.append('"' + f.replace('"', '""') + '"')
        else:
            out.append(f)
    return ",".join(out)


def _field_text(v, t, decimals):
    if is_missing(v):
        return ""  # R020-17: missing is no characters, unquoted
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
        phase="output", condition="invalid_field_type", context={"type": t}
    )


def _fixed_point(x, n):
    """R020-33/34: exact scaling, tie away from zero, no host rounding."""
    from decimal import ROUND_HALF_UP, Decimal

    d = Decimal(x)  # exact binary64 value
    q = Decimal(10) ** n
    scaled = (d * q).to_integral_value(rounding=ROUND_HALF_UP)
    if n == 0:
        return str(scaled)
    neg = scaled < 0
    s = str(abs(scaled)).rjust(n + 1, "0")
    return ("-" if neg else "") + s[:-n] + "." + s[-n:]


def read_parquet(
    path, types, spec_path="<input>", dataset="<input>", written_path=None
):
    """Read a Parquet source per the Parquet profile."""
    import datetime as _datetime

    import pyarrow.parquet as pq

    written_path = written_path if written_path is not None else path
    try:
        table = pq.read_table(path)
    except Exception as exc:  # noqa: BLE001 -- any read failure is source_unreadable
        raise _phase_condition(
            "ingest",
            "source_unreadable",
            requirement="R023",
            spec_paths=[spec_path],
            context={"dataset": dataset, "path": written_path, "error": str(exc)},
        )
    fields = table.schema.names
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
    return fields, records
