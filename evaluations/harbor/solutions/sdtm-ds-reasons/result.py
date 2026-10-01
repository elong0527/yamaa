# Reference solution for the yamaa benchmark sdtm-ds-reasons (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

wide = (
    odm.group_by("StudyOID", "SubjectKey", "StudyEventOID")
    .agg(
        pl.col("Value").filter(pl.col("ItemOID") == "IT.DS.DTC").first().alias("DTC"),
        pl.col("Value").filter(pl.col("ItemOID") == "IT.DS.COMP").first().alias("COMP"),
        pl.col("Value").filter(pl.col("ItemOID") == "IT.DS.REASON").first().alias("REASON"),
        pl.col("Value")
        .filter(pl.col("ItemOID") == "IT.DS.REASONCD")
        .first()
        .alias("REASONCD"),
    )
    .filter(pl.col("StudyEventOID").is_in(["EOT", "EOS"]))
)

MILESTONE = {"EOT": ("STUDY TREATMENT", 1), "EOS": ("STUDY", 2)}

records: list[dict] = []
for row in wide.to_dicts():
    study = row["StudyOID"]
    subj = row["SubjectKey"]
    event = row["StudyEventOID"]
    sscat, seq = MILESTONE[event]
    comp = row["COMP"] or ""
    reason = row["REASON"] or ""
    reasoncd = row["REASONCD"] or ""
    dtc = row["DTC"] or ""
    if comp == "COMPLETED":
        term, decod = "COMPLETED", "COMPLETED"
    else:
        term, decod = (reason or None), (reasoncd or None)
    usubjid = subj
    records.append(
        {
            "DOMAIN": "DS",
            "STUDYID": study,
            "USUBJID": usubjid,
            "DSSEQ": seq,
            "DSCAT": "DISPOSITION EVENT",
            "DSSCAT": sscat,
            "DSTERM": term,
            "DSDECOD": decod,
            "DSSTDTC": dtc or None,
        }
    )

ds = pl.DataFrame(
    records,
    schema={
        "DOMAIN": pl.String,
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "DSSEQ": pl.Int64,
        "DSCAT": pl.String,
        "DSSCAT": pl.String,
        "DSTERM": pl.String,
        "DSDECOD": pl.String,
        "DSSTDTC": pl.String,
    },
).sort(["USUBJID", "DSSEQ"])

Path("/app/output").mkdir(exist_ok=True)
ds.write_csv("/app/output/ds.csv")
