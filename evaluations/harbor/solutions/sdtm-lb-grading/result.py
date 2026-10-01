# Reference solution for the yamaa benchmark sdtm-lb-grading (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

raw = pl.read_csv("/app/input/lb_raw.csv", infer_schema=False).with_columns(
    pl.col("LBSEQ").cast(pl.Int64),
    pl.col("LBSTRESN").cast(pl.Float64, strict=False),
)


def toxgrade(test, sex, value):
    if value is None:
        return None
    if test == "ANC":
        if value < 0.5:
            return "4"
        if value < 1.0:
            return "3"
        if value < 1.5:
            return "2"
        if value < 1.8:
            return "1"
        return "0"
    if test == "HGB":
        if sex not in ("M", "F"):
            return None
        if value < 8.0:
            return "3"
        if value < 10.0:
            return "2"
        limit = 13.5 if sex == "M" else 12.0
        if value < limit:
            return "1"
        return "0"
    return None


rows = []
for r in raw.to_dicts():
    grade = toxgrade(r["LBTESTCD"], r["SEX"], r["LBSTRESN"])
    if grade is None and r["LBTESTCD"] not in ("ANC", "HGB"):
        continue
    if r["LBTESTCD"] == "HGB" and r["SEX"] not in ("M", "F"):
        continue
    rows.append(
        {
            "DOMAIN": "LB",
            "STUDYID": r["STUDYID"],
            "USUBJID": r["USUBJID"],
            "LBSEQ": r["LBSEQ"],
            "LBTESTCD": r["LBTESTCD"],
            "LBSTRESN": r["LBSTRESN"],
            "LBTOXGR": grade,
        }
    )

lb = pl.DataFrame(
    rows,
    schema={
        "DOMAIN": pl.String,
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "LBSEQ": pl.Int64,
        "LBTESTCD": pl.String,
        "LBSTRESN": pl.Float64,
        "LBTOXGR": pl.String,
    },
).sort(["STUDYID", "USUBJID", "LBSEQ"])

Path("/app/output").mkdir(parents=True, exist_ok=True)
lb.write_csv("/app/output/lb.csv")
