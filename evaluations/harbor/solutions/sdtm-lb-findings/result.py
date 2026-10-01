# Reference solution for the yamaa benchmark sdtm-lb-findings (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

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
        .list.eval(
            pl.element().filter(
                pl.element().struct.field("ItemOID") == "IT.LB.LBDTC"
            )
        )
        .list.get(0)
        .struct.field("Value")
        .alias("LBDTC"),
    )
    .drop("items")
)

tests = odm.filter(pl.col("ItemOID").is_in(["IT.LB.CALCIUM", "IT.LB.CREAT"]))

code_of = {"IT.LB.CALCIUM": "CA", "IT.LB.CREAT": "CREAT"}
name_of = {"CA": "Calcium", "CREAT": "Creatinine"}

joined = tests.join(wide, on=key_cols, how="left")


def as_number(text):
    if text is None or text == "" or text == "NOT DONE":
        return None
    try:
        return float(text)
    except ValueError:
        return None


rows = []
for r in joined.to_dicts():
    value = r["Value"]
    # A test with no collected entry produces no record.
    if value is None or value == "":
        continue
    code = code_of[r["ItemOID"]]
    not_done = value == "NOT DONE"
    rows.append(
        {
            "DOMAIN": "LB",
            "STUDYID": r["StudyOID"],
            "USUBJID": r["SubjectKey"],
            "LBDTC": r["LBDTC"],
            "LBTESTCD": code,
            "LBTEST": name_of[code],
            "LBORRES": None if not_done else value,
            "LBORRESU": None if not_done else "mg/dL",
            "LBSTRESN": None if not_done else as_number(value),
            "LBSTRESU": None if not_done else "mg/dL",
            "LBSTAT": "NOT DONE" if not_done else None,
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
        "LBSTAT": pl.String,
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
    "LBSTAT",
    "LBDTC",
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
lb.write_csv("/app/output/lb.csv")
