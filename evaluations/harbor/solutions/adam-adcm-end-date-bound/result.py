# Reference solution for the yamaa benchmark adam-adcm-end-date-bound (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import calendar
from datetime import date
from pathlib import Path

import polars as pl

cm = pl.read_csv("/app/input/cm.csv", infer_schema=False).with_columns(
    pl.col("CMSEQ").cast(pl.Int64)
)
adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False)
death = {row["USUBJID"]: row["DTHDT"] for row in adsl.iter_rows(named=True)}


def end_date(collected: str | None, died: str | None) -> tuple[str | None, str | None]:
    # A full date stands as collected; a year and month runs to its last
    # day, a year alone to December 31, never past the date of death.
    if collected is None:
        return None, None
    if len(collected) == 10:
        return collected, None
    year = int(collected[:4])
    if len(collected) == 7:
        month = int(collected[5:7])
        first = date(year, month, 1)
        last = date(year, month, calendar.monthrange(year, month)[1])
        flag = "D"
    else:
        first = date(year, 1, 1)
        last = date(year, 12, 31)
        flag = "M"
    if died is not None:
        bound = date.fromisoformat(died)
        if bound < first:
            # The whole collected month or year lies after death.
            return None, None
        last = min(last, bound)
    return last.isoformat(), flag


rows = []
for record in cm.iter_rows(named=True):
    died = death.get(record["USUBJID"])
    aendt, aendtf = end_date(record["CMENDTC"], died)
    rows.append(
        {
            "STUDYID": record["STUDYID"],
            "USUBJID": record["USUBJID"],
            "CMSEQ": record["CMSEQ"],
            "CMTRT": record["CMTRT"],
            "CMENDTC": record["CMENDTC"],
            "AENDT": aendt,
            "AENDTF": aendtf,
            "DTHDT": died,
        }
    )

adcm = pl.DataFrame(
    rows,
    schema={
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "CMSEQ": pl.Int64,
        "CMTRT": pl.String,
        "CMENDTC": pl.String,
        "AENDT": pl.String,
        "AENDTF": pl.String,
        "DTHDT": pl.String,
    },
)

Path("/app/output").mkdir(exist_ok=True)
adcm.write_csv("/app/output/adcm.csv")
