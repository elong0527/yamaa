# Reference solution for the yamaa benchmark sdtm-ds-multi-form-disposition (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
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
)


def build_row(event: str, dtc, comp, reason, reasoncd) -> dict | None:
    if event == "CONSENT":
        return {
            "DSTERM": "INFORMED CONSENT OBTAINED",
            "DSDECOD": "INFORMED CONSENT OBTAINED",
            "DSCAT": "PROTOCOL MILESTONE",
            "DSSCAT": "INFORMED CONSENT",
        }
    if event == "RAND":
        return {
            "DSTERM": "RANDOMIZED",
            "DSDECOD": "RANDOMIZED",
            "DSCAT": "PROTOCOL MILESTONE",
            "DSSCAT": "RANDOMIZATION",
        }
    if event == "EOT":
        if comp == "COMPLETED":
            term, decod = "COMPLETED", "COMPLETED"
        else:
            term, decod = reason, reasoncd
        return {
            "DSTERM": term,
            "DSDECOD": decod,
            "DSCAT": "DISPOSITION EVENT",
            "DSSCAT": "END OF TREATMENT",
        }
    if event == "EOS":
        if comp == "COMPLETED":
            term, decod = "COMPLETED", "COMPLETED"
        elif comp == "SCREEN FAILURE":
            term, decod = "SCREEN FAILURE", "SCREEN FAILURE"
        else:
            term, decod = reason, reasoncd
        return {
            "DSTERM": term,
            "DSDECOD": decod,
            "DSCAT": "DISPOSITION EVENT",
            "DSSCAT": "END OF STUDY",
        }
    return None


records: list[dict] = []
for row in wide.to_dicts():
    study = row["StudyOID"]
    subj = row["SubjectKey"]
    event = row["StudyEventOID"]
    out = build_row(event, row["DTC"], row["COMP"], row["REASON"], row["REASONCD"])
    if out is None:
        continue
    usubjid = f"{study}-{subj}" if not subj.startswith(study) else subj
    records.append(
        {
            "STUDYID": study,
            "USUBJID": usubjid,
            "DSTERM": out["DSTERM"] if out["DSTERM"] != "" else None,
            "DSDECOD": out["DSDECOD"] if out["DSDECOD"] != "" else None,
            "DSCAT": out["DSCAT"],
            "DSSCAT": out["DSSCAT"],
            "DSSTDTC": row["DTC"] if row["DTC"] != "" else None,
        }
    )

ds = pl.DataFrame(
    records,
    schema={
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "DSTERM": pl.String,
        "DSDECOD": pl.String,
        "DSCAT": pl.String,
        "DSSCAT": pl.String,
        "DSSTDTC": pl.String,
    },
).with_columns(
    DOMAIN=pl.lit("DS"),
    DSSTDTC_DT=pl.col("DSSTDTC").str.to_date(strict=False),
)

ds = (
    ds.sort(["USUBJID", "DSSTDTC_DT", "DSTERM"])
    .with_columns(
        DSSEQ=pl.col("USUBJID").cum_count().over("USUBJID"),
    )
    .with_columns(DSSEQ=pl.col("DSSEQ").cast(pl.Int64))
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "DSSEQ",
        "DSTERM",
        "DSDECOD",
        "DSCAT",
        "DSSCAT",
        "DSSTDTC",
    )
)

Path("/app/output").mkdir(exist_ok=True)
ds.write_csv("/app/output/ds.csv")
