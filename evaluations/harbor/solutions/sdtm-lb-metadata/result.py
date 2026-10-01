# Reference solution for the yamaa benchmark sdtm-lb-metadata (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

# One row per ItemOID within each visit group.
key_cols = [
    "StudyOID",
    "SubjectKey",
    "StudyEventOID",
    "StudyEventRepeatKey",
    "ItemGroupOID",
    "ItemGroupRepeatKey",
]
wide = (
    odm.group_by(key_cols)
    .agg(pl.struct("ItemOID", "Value").alias("items"))
    .with_columns(
        pl.col("items")
        .list.eval(pl.element().filter(pl.element().struct.field("ItemOID") == "IT.LB.LBDTC"))
        .list.get(0)
        .struct.field("Value")
        .alias("LBDTC"),
    )
    .drop("items")
)

# Keep the raw long rows for the two lab tests.
tests = odm.filter(pl.col("ItemOID").is_in(["IT.LB.GLUC", "IT.LB.CREAT"]))

code_of = {"IT.LB.GLUC": "GLUC", "IT.LB.CREAT": "CREAT"}
name_of = {"GLUC": "Glucose", "CREAT": "Creatinine"}

joined = tests.join(wide, on=key_cols, how="left")


def as_number(text):
    if text is None or text == "":
        return None
    try:
        return float(text)
    except ValueError:
        return None


rows = []
for r in joined.to_dicts():
    value = r["Value"]
    if value is None or value == "":
        continue
    code = code_of[r["ItemOID"]]
    rows.append(
        {
            "DOMAIN": "LB",
            "STUDYID": r["StudyOID"],
            "USUBJID": r["SubjectKey"],
            "LBDTC": r["LBDTC"],
            "LBTESTCD": code,
            "LBTEST": name_of[code],
            "LBORRES": value,
            "LBORRESU": "mg/dL",
            "LBSTRESN": as_number(value),
            "LBSTRESU": "mg/dL",
        }
    )

lb = pl.DataFrame(
    rows,
    schema={
        "DOMAIN": pl.String,
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "LBDTC": pl.String,
        "LBTESTCD": pl.String,
        "LBTEST": pl.String,
        "LBORRES": pl.String,
        "LBORRESU": pl.String,
        "LBSTRESN": pl.Float64,
        "LBSTRESU": pl.String,
    },
).sort(["STUDYID", "USUBJID", "LBDTC", "LBTESTCD"]).with_columns(
    pl.int_range(1, pl.len() + 1).over(["STUDYID", "USUBJID"]).alias("LBSEQ")
).select(
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "LBSEQ",
    "LBTESTCD",
    "LBTEST",
    "LBORRES",
    "LBORRESU",
    "LBSTRESN",
    "LBSTRESU",
    "LBDTC",
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
lb.write_csv("/app/output/lb.csv")
