# Reference solution for the yamaa benchmark adam-adeg-derived-intervals (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import math
from pathlib import Path

import polars as pl


def format_number(value: float | None) -> str | None:
    if value is None or not math.isfinite(value):
        return None
    if float(value).is_integer():
        return str(int(value))
    return repr(float(value))


base = pl.read_csv("/app/input/adeg.csv", infer_schema=False)
numeric = base.with_columns(AVALNUM=pl.col("AVAL").cast(pl.Float64, strict=False))

wide = numeric.group_by(["STUDYID", "USUBJID", "AVISIT"]).agg(
    QT=pl.col("AVALNUM").filter(pl.col("PARAMCD") == "QT").first(),
    RR=pl.col("AVALNUM").filter(pl.col("PARAMCD") == "RR").first(),
    HR=pl.col("AVALNUM").filter(pl.col("PARAMCD") == "HR").first(),
)

collected = base.select(
    "STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVISIT", "AVAL", "AVALU"
)

new_rows: list[dict] = []
for record in wide.to_dicts():
    studyid, usubjid, avisit = record["STUDYID"], record["USUBJID"], record["AVISIT"]
    qt, rr, hr = record["QT"], record["RR"], record["HR"]
    # A QTc record needs both QT and RR; RR in seconds is RR in ms over 1000.
    if qt is not None and rr is not None:
        rr_sec = rr / 1000
        new_rows.append(
            {
                "STUDYID": studyid,
                "USUBJID": usubjid,
                "PARAMCD": "QTCBR",
                "PARAM": "QTcB - Bazett's Correction Formula Rederived (ms)",
                "AVISIT": avisit,
                "AVAL": format_number(qt / math.sqrt(rr_sec)),
                "AVALU": "ms",
            }
        )
    if qt is not None and rr is not None:
        rr_sec = rr / 1000
        new_rows.append(
            {
                "STUDYID": studyid,
                "USUBJID": usubjid,
                "PARAMCD": "QTCFR",
                "PARAM": "QTcF - Fridericia's Correction Formula Rederived (ms)",
                "AVISIT": avisit,
                "AVAL": format_number(qt / (rr_sec ** (1 / 3))),
                "AVALU": "ms",
            }
        )
    # An RRR record needs a present, nonzero heart rate.
    if hr is not None and hr != 0:
        new_rows.append(
            {
                "STUDYID": studyid,
                "USUBJID": usubjid,
                "PARAMCD": "RRR",
                "PARAM": "RR Duration Rederived (ms)",
                "AVISIT": avisit,
                "AVAL": format_number(60000 / hr),
                "AVALU": "ms",
            }
        )

# Collected rows first, then QTCBR, QTCFR, and RRR per visit.
order = {"QTCBR": 0, "QTCFR": 1, "RRR": 2}
new_rows.sort(
    key=lambda r: (order[r["PARAMCD"]], r["USUBJID"], r["AVISIT"])
)
derived = pl.DataFrame(
    new_rows,
    schema={
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "PARAMCD": pl.String,
        "PARAM": pl.String,
        "AVISIT": pl.String,
        "AVAL": pl.String,
        "AVALU": pl.String,
    },
)

adeg = pl.concat([collected, derived], how="diagonal").select(
    "STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVISIT", "AVAL", "AVALU"
)

Path("/app/output").mkdir(exist_ok=True)
adeg.write_csv("/app/output/adeg.csv")
