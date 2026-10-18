# Reference solution for the yamaa benchmark sdtm-cm-whodrug-coding (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build an SDTM CM dataset from ODM reported medications, the coder's
WHODrug coding output, and the WHODrug extract.

One record per reported medication (the IT.CM.CMTRT item of each IG.CM
item group in ODM). The coded variables come from the WHODrug extract row
for the drug record and ATC code the coder assigned to that record; they
stay blank when the reported name is not yet coded.
"""

import polars as pl

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "CMSEQ",
    "CMTRT",
    "CMDECOD",
    "CMCLAS",
    "CMCLASCD",
]


def read_csv(path):
    # Read every column as text so values are kept exactly as written.
    return pl.read_csv(path, infer_schema_length=0)


def main():
    odm = read_csv(f"{INPUT_DIR}/odm.csv")
    coding = read_csv(f"{INPUT_DIR}/coding_output.csv")
    whodrug = read_csv(f"{INPUT_DIR}/whodrug_extract.csv")

    # One record per reported medication: the CMTRT item of each CM item
    # group. CMSEQ is the item group's repeat key within the subject.
    meds = odm.filter(pl.col("ItemOID") == "IT.CM.CMTRT")
    cm = meds.select(
        pl.lit("CM").alias("DOMAIN"),
        pl.col("StudyOID").alias("STUDYID"),
        pl.col("SubjectKey").alias("USUBJID"),
        pl.col("ItemGroupRepeatKey").cast(pl.Int64).alias("CMSEQ"),
        pl.col("Value").alias("CMTRT"),
    ).sort(["USUBJID", "CMSEQ"], maintain_order=True)

    # The drug record and ATC code the coder assigned to each record.
    cm = cm.join(
        coding.select("STUDYID", "USUBJID", "CMSEQ", "DRUG_RECORD_NO", "ATC_CODE")
        .with_columns(pl.col("CMSEQ").cast(pl.Int64)),
        on=["STUDYID", "USUBJID", "CMSEQ"],
        how="left",
    )

    # Preferred name and ATC class for that drug record together with the
    # assigned ATC code; the same drug record can carry a different class
    # for a different assigned ATC code.
    cm = cm.join(
        whodrug.select(
            "DRUG_RECORD_NO",
            "ATC_CODE",
            "PREFERRED_NAME",
            "ATC_CLASS_NAME",
            "ATC_CLASS_CODE",
        ),
        on=["DRUG_RECORD_NO", "ATC_CODE"],
        how="left",
    )

    cm = cm.rename(
        {
            "PREFERRED_NAME": "CMDECOD",
            "ATC_CLASS_NAME": "CMCLAS",
            "ATC_CLASS_CODE": "CMCLASCD",
        }
    )

    # Uncoded values stay null and are written as empty fields.
    cm.select(COLUMNS).write_csv(f"{OUTPUT_DIR}/cm.csv", null_value="")


if __name__ == "__main__":
    main()
