# Reference solution for the yamaa benchmark adam-adqs-missed-visit-locf (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False)
qs = pl.read_csv("/app/input/qs.csv", infer_schema=False).with_columns(
    pl.col("AVISITN").cast(pl.Int64),
    pl.col("AVAL").cast(pl.Float64),
)

# Collected scores; an empty score is left out, so it counts as missed.
collected = (
    qs.filter(pl.col("PARAMCD") == "ACTOT", pl.col("AVAL").is_not_null())
    .join(adsl.select("USUBJID", "EFFFL"), on="USUBJID", how="left")
    .with_columns(DTYPE=pl.lit(None, dtype=pl.String))
    .select(
        "STUDYID", "USUBJID", "PARAMCD", "AVISIT", "AVISITN", "AVAL", "DTYPE", "EFFFL"
    )
)

# The protocol schedules Weeks 8, 16, and 24 for the efficacy population.
scheduled = pl.DataFrame(
    {"AVISIT": ["Week 8", "Week 16", "Week 24"], "AVISITN": [8, 16, 24]}
)
eff_subjects = adsl.filter(pl.col("EFFFL") == "Y").select(
    "STUDYID", "USUBJID", "EFFFL"
)
planned = eff_subjects.join(scheduled, how="cross")
missed_keys = planned.join(
    collected.select("USUBJID", "AVISITN"), on=["USUBJID", "AVISITN"], how="anti"
)

# Each missed visit carries the closest earlier visit with a score, where
# a collected zero carries forward and no earlier score leaves no value.
earlier_by_subject: dict[str, list[tuple[int, float]]] = {}
for row in collected.sort(["USUBJID", "AVISITN"]).iter_rows(named=True):
    earlier_by_subject.setdefault(row["USUBJID"], []).append(
        (row["AVISITN"], row["AVAL"])
    )

missed_rows = []
for row in missed_keys.sort(["USUBJID", "AVISITN"]).iter_rows(named=True):
    carried = None
    for visitn, aval in earlier_by_subject.get(row["USUBJID"], []):
        if visitn < row["AVISITN"]:
            carried = aval
        else:
            break
    missed_rows.append({**row, "PARAMCD": "ACTOT", "AVAL": carried, "DTYPE": "LOCF"})

missed = pl.DataFrame(
    missed_rows,
    schema={
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "EFFFL": pl.String,
        "AVISIT": pl.String,
        "AVISITN": pl.Int64,
        "PARAMCD": pl.String,
        "AVAL": pl.Float64,
        "DTYPE": pl.String,
    },
    strict=False,
).select(
    "STUDYID", "USUBJID", "PARAMCD", "AVISIT", "AVISITN", "AVAL", "DTYPE", "EFFFL"
)

adqs = pl.concat([collected, missed], how="diagonal").sort(["USUBJID", "AVISITN"])

Path("/app/output").mkdir(exist_ok=True)
adqs.write_csv("/app/output/adqs.csv")
