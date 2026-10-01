# Reference solution for the yamaa benchmark adam-adae-partial-dates (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import calendar
from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)
adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False).select("USUBJID", "TRTSDT")


def complete_row(collected: str | None, trt: str | None) -> tuple[str | None, str | None]:
    """The analysis date and its imputation flag for one collected start."""
    if collected is None:
        return None, None
    full = len(collected) == 10 and collected[4] == "-" and collected[7] == "-"
    month = len(collected) == 7 and collected[4] == "-"
    try:
        if full:
            year, mon, day = int(collected[0:4]), int(collected[5:7]), int(collected[8:10])
            last_day = calendar.monthrange(year, mon)[1]
            if day < 1 or day > last_day:
                return None, None
            completed = f"{year:04d}-{mon:02d}-{day:02d}"
            # A fully collected date stays exactly as collected.
            return completed, None
        if not month:
            return None, None
        year, mon = int(collected[0:4]), int(collected[5:7])
        completed = f"{year:04d}-{mon:02d}-15"
        if trt is None:
            return completed, "D"
        # A completed date is never placed before first exposure. When
        # the whole collected month ends before the exposure date, the
        # event is left without an analysis date.
        last_day = calendar.monthrange(year, mon)[1]
        month_end = f"{year:04d}-{mon:02d}-{last_day:02d}"
        if month_end < trt:
            return None, None
        if completed < trt:
            return trt, "D"
        return completed, "D"
    except ValueError:
        return None, None


joined = ae.join(adsl, on="USUBJID", how="left", maintain_order="left")
records = []
for row in joined.to_dicts():
    ast, flag = complete_row(row["AESTDTC"], row["TRTSDT"])
    trtemfl = (
        "Y"
        if ast is not None and row["TRTSDT"] is not None and ast >= row["TRTSDT"]
        else None
    )
    records.append(
        {
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "AESEQ": row["AESEQ"],
            "AETERM": row["AETERM"],
            "AESTDTC": row["AESTDTC"],
            "ASTDT": ast,
            "ASTDTC": ast,
            "ASTDTF": flag,
            "TRTSDT": row["TRTSDT"],
            "TRTEMFL": trtemfl,
        }
    )

adae = pl.DataFrame(
    records,
    schema={
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "AESEQ": pl.Int64,
        "AETERM": pl.String,
        "AESTDTC": pl.String,
        "ASTDT": pl.String,
        "ASTDTC": pl.String,
        "ASTDTF": pl.String,
        "TRTSDT": pl.String,
        "TRTEMFL": pl.String,
    },
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
