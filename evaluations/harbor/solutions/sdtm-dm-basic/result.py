# Reference solution for the yamaa benchmark sdtm-dm-basic (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

wide = odm.filter(pl.col("ItemGroupOID") == "IG.DM").pivot(
    index=["StudyOID", "SubjectKey"],
    on="ItemOID",
    values="Value",
    aggregate_function="first",
)

rows = []
for record in wide.to_dicts():
    sex_raw = record.get("IT.DM.SEX")
    # M for Male, F for Female; anything missing, blank, uncollected, or
    # otherwise recorded becomes U.
    sex = {"Male": "M", "Female": "F"}.get(sex_raw, "U")
    age_raw = (record.get("IT.DM.AGE") or "").strip()
    age = int(float(age_raw)) if age_raw else None
    arm = (record.get("IT.DM.ARM") or "").strip() or None
    # The actual arm always equals the planned arm.
    rows.append(
        {
            "DOMAIN": "DM",
            "STUDYID": record["StudyOID"],
            "USUBJID": record["SubjectKey"],
            "SUBJID": record["SubjectKey"],
            "SEX": sex,
            "AGE": age,
            "ARM": arm,
            "ACTARM": arm,
            "ARMNRS": "Not assigned to treatment arm" if arm is None else None,
        }
    )

dm = pl.DataFrame(
    rows,
    schema={
        "DOMAIN": pl.String,
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "SUBJID": pl.String,
        "SEX": pl.String,
        "AGE": pl.Int64,
        "ARM": pl.String,
        "ACTARM": pl.String,
        "ARMNRS": pl.String,
    },
).sort("USUBJID")

Path("/app/output").mkdir(exist_ok=True)
dm.write_csv("/app/output/dm.csv")
