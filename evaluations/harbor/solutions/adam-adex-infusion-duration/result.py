# Reference solution for the yamaa benchmark adam-adex-infusion-duration (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import re
from datetime import datetime
from pathlib import Path

import polars as pl

ex = pl.read_csv("/app/input/ex.csv", infer_schema=False).with_columns(
    pl.col("EXSEQ").cast(pl.Int64)
)

DURATION = re.compile(
    r"P(?:(\d+)D)?(?:T(?:(\d+)H)?(?:(\d+)M)?(?:(\d+(?:\.\d+)?)S)?)?"
)


def collected_minutes(text: str | None) -> float | None:
    # An ISO 8601 duration of days, hours, minutes and seconds, with at
    # least one part; anything else counts as not collected.
    if text is None:
        return None
    match = DURATION.fullmatch(text)
    if match is None or not any(match.groups()) or text.endswith("T"):
        return None
    days, hours, minutes, seconds = (float(part) if part else 0.0 for part in match.groups())
    return days * 1440 + hours * 60 + minutes + seconds / 60


def moment(text: str | None) -> datetime | None:
    # A date alone has no time of day, and none is assumed.
    if text is None or len(text) <= 10:
        return None
    return datetime.fromisoformat(text if len(text) == 19 else text + ":00")


rows = []
for record in ex.iter_rows(named=True):
    start, end = moment(record["EXSTDTC"]), moment(record["EXENDTC"])
    calcdur = (end - start).total_seconds() / 60 if start and end else None
    collected = collected_minutes(record["EXDUR"])
    rows.append(
        {
            "STUDYID": record["STUDYID"],
            "USUBJID": record["USUBJID"],
            "EXSEQ": record["EXSEQ"],
            "EXSTDTC": record["EXSTDTC"],
            "EXENDTC": record["EXENDTC"],
            "EXDUR": record["EXDUR"],
            "PARAMCD": "INFDUR",
            "PARAM": "Infusion Duration (min)",
            "AVAL": collected if collected is not None else calcdur,
            "AVALU": "min",
            "CALCDUR": calcdur,
        }
    )

adex = pl.DataFrame(
    rows,
    schema={
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "EXSEQ": pl.Int64,
        "EXSTDTC": pl.String,
        "EXENDTC": pl.String,
        "EXDUR": pl.String,
        "PARAMCD": pl.String,
        "PARAM": pl.String,
        "AVAL": pl.Float64,
        "AVALU": pl.String,
        "CALCDUR": pl.Float64,
    },
)

Path("/app/output").mkdir(exist_ok=True)
adex.write_csv("/app/output/adex.csv")
