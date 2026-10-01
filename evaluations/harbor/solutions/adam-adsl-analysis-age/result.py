# Reference solution for the yamaa benchmark adam-adsl-analysis-age (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from datetime import date
from pathlib import Path

import polars as pl


def is_leap(year: int) -> bool:
    return year % 4 == 0 and (year % 100 != 0 or year % 400 == 0)


def whole_years(start: date | None, end: date | None) -> int | None:
    """Yearly anniversaries of start on or before end; negated when reversed."""
    if start is None or end is None:
        return None
    if end < start:
        back = whole_years(end, start)
        return None if back is None else -back
    years = end.year - start.year
    month, day = start.month, start.day
    # A February 29 birthday falls on February 28 in common years.
    if month == 2 and day == 29 and not is_leap(end.year):
        day = 28
    if (end.month, end.day) < (month, day):
        years -= 1
    return years


dm = pl.read_csv("/app/input/dm.csv", infer_schema=False).with_columns(
    pl.col("BRTHDT").str.to_date(strict=False),
    pl.col("RANDDT").str.to_date(strict=False),
)

adsl = (
    dm.with_columns(
        AAGE=pl.struct(["BRTHDT", "RANDDT"]).map_elements(
            lambda r: whole_years(r["BRTHDT"], r["RANDDT"]),
            return_dtype=pl.Int64,
        ),
        AAGEU=pl.lit("YEARS"),
    )
    .select("STUDYID", "USUBJID", "BRTHDT", "RANDDT", "AAGE", "AAGEU")
)

Path("/app/output").mkdir(exist_ok=True)
adsl.write_csv("/app/output/adsl.csv")
