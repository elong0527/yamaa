# Reference solution for the yamaa benchmark adam-adsl-duration (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import calendar
from datetime import date
from pathlib import Path

import polars as pl


def add_months(day: date, n: int) -> date:
    total = day.month - 1 + n
    year = day.year + total // 12
    month = total % 12 + 1
    last = calendar.monthrange(year, month)[1]
    return date(year, month, min(day.day, last))


def whole_months(start: date | None, end: date | None) -> int | None:
    """Monthly anniversaries of start on or before end; negated when reversed."""
    if start is None or end is None:
        return None
    if end < start:
        back = whole_months(end, start)
        return None if back is None else -back
    months = (end.year - start.year) * 12 + (end.month - start.month)
    while months > 0 and add_months(start, months) > end:
        months -= 1
    while add_months(start, months + 1) <= end:
        months += 1
    return months


def whole_weeks(start: date | None, end: date | None) -> int | None:
    """Whole seven-day blocks from start to end; negated when reversed."""
    if start is None or end is None:
        return None
    if end < start:
        back = whole_weeks(end, start)
        return None if back is None else -back
    return (end - start).days // 7


dm = pl.read_csv("/app/input/dm.csv", infer_schema=False).with_columns(
    pl.col("STDT").str.to_date(strict=False),
    pl.col("ENDT").str.to_date(strict=False),
)

adsl = (
    dm.with_columns(
        DURW=pl.struct(["STDT", "ENDT"]).map_elements(
            lambda r: whole_weeks(r["STDT"], r["ENDT"]), return_dtype=pl.Int64
        ),
        DURM=pl.struct(["STDT", "ENDT"]).map_elements(
            lambda r: whole_months(r["STDT"], r["ENDT"]), return_dtype=pl.Int64
        ),
    )
    .select("STUDYID", "USUBJID", "STDT", "ENDT", "DURW", "DURM")
)

Path("/app/output").mkdir(exist_ok=True)
adsl.write_csv("/app/output/adsl.csv")
