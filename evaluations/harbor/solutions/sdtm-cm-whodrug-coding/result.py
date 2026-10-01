# Reference solution for the yamaa benchmark sdtm-cm-whodrug-coding (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)
coding = pl.read_csv("/app/input/coding_output.csv", infer_schema=False).with_columns(
    pl.col("CMSEQ").cast(pl.Int64)
)
whodrug = pl.read_csv("/app/input/whodrug_extract.csv", infer_schema=False)

# One record per reported medication; the repeat number is the sequence
# number. The preferred name comes from the drug record the coder chose,
# and the class from the ATC code assigned for this use, so the same drug
# record can give a different class on different records. No coding, or a
# code not in the extract, leaves the coded variables empty.
reported = (
    odm.filter(pl.col("ItemGroupOID") == "IG.CM")
    .pivot(
        index=["StudyOID", "SubjectKey", "ItemGroupRepeatKey"],
        on="ItemOID",
        values="Value",
        aggregate_function="first",
    )
    .with_columns(
        STUDYID=pl.col("StudyOID"),
        USUBJID=pl.col("SubjectKey"),
        CMSEQ=pl.col("ItemGroupRepeatKey").cast(pl.Int64),
        CMTRT=pl.col("IT.CM.CMTRT"),
    )
    .select("STUDYID", "USUBJID", "CMSEQ", "CMTRT")
)

cm = (
    reported.join(coding, on=["STUDYID", "USUBJID", "CMSEQ"], how="left")
    .join(
        whodrug,
        left_on=["DRUG_RECORD_NO", "ATC_CODE"],
        right_on=["DRUG_RECORD_NO", "ATC_CODE"],
        how="left",
    )
    .with_columns(
        DOMAIN=pl.lit("CM"),
        CMDECOD=pl.when(pl.col("PREFERRED_NAME").is_null())
        .then(None)
        .otherwise(pl.col("PREFERRED_NAME")),
        CMCLAS=pl.col("ATC_CLASS_NAME"),
        CMCLASCD=pl.col("ATC_CLASS_CODE"),
    )
    .with_columns(
        CMDECOD=pl.when(
            pl.col("DRUG_RECORD_NO").fill_null("") == ""
        )
        .then(None)
        .otherwise(pl.col("CMDECOD")),
        CMCLAS=pl.when(pl.col("CMDECOD").is_null())
        .then(None)
        .otherwise(pl.col("CMCLAS")),
        CMCLASCD=pl.when(pl.col("CMDECOD").is_null())
        .then(None)
        .otherwise(pl.col("CMCLASCD")),
    )
    .sort(["USUBJID", "CMSEQ"])
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "CMSEQ",
        "CMTRT",
        "CMDECOD",
        "CMCLAS",
        "CMCLASCD",
    )
)

Path("/app/output").mkdir(exist_ok=True)
cm.write_csv("/app/output/cm.csv")
