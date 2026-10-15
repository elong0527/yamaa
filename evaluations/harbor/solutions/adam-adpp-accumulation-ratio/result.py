# Reference solution for the yamaa benchmark adam-adpp-accumulation-ratio (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
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


COLUMNS = ["STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVISIT", "AVAL", "AVALU"]

pp = pl.read_csv("/app/input/pp.csv", infer_schema=False)
adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False)

# Every PP record, a parameter that was not calculated included.
collected = pp.select(
    "STUDYID",
    "USUBJID",
    pl.col("PPTESTCD").alias("PARAMCD"),
    pl.col("PPTEST").alias("PARAM"),
    pl.col("VISIT").alias("AVISIT"),
    pl.col("PPSTRESN").alias("AVAL"),
    pl.col("PPSTRESU").alias("AVALU"),
)

# The steady-state profile is the last planned dosing day of the arm.
steady_state_visit = {"A": "DAY 7", "B": "DAY 5"}
arms = {
    (r["STUDYID"], r["USUBJID"]): r["ARMCD"] for r in adsl.iter_rows(named=True)
}

values = {}
for r in pp.iter_rows(named=True):
    value = r["PPSTRESN"]
    key = (r["STUDYID"], r["USUBJID"], r["PPTESTCD"], r["VISIT"])
    values[key] = float(value) if value not in (None, "") else None

subjects = list(dict.fromkeys((r["STUDYID"], r["USUBJID"]) for r in pp.iter_rows(named=True)))

ratios = []
for paramcd, param, testcd in (
    ("ARAUC", "Accumulation Ratio AUCTAU", "AUCTAU"),
    ("ARCMAX", "Accumulation Ratio Cmax", "CMAX"),
):
    for studyid, usubjid in subjects:
        visit = steady_state_visit.get(arms.get((studyid, usubjid)))
        if visit is None:
            continue
        steady = values.get((studyid, usubjid, testcd, visit))
        first = values.get((studyid, usubjid, testcd, "DAY 1"))
        # A ratio needs both values.
        if steady is None or first is None or first == 0:
            continue
        ratios.append(
            {
                "STUDYID": studyid,
                "USUBJID": usubjid,
                "PARAMCD": paramcd,
                "PARAM": param,
                "AVISIT": visit,
                "AVAL": format_number(steady / first),
                "AVALU": "RATIO",
            }
        )

derived = pl.DataFrame(ratios, schema={c: pl.String for c in COLUMNS})
adpp = pl.concat([collected, derived], how="vertical").select(COLUMNS)

Path("/app/output").mkdir(exist_ok=True)
adpp.write_csv("/app/output/adpp.csv")
