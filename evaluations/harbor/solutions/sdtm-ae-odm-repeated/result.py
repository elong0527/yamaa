# Reference solution for the yamaa benchmark sdtm-ae-odm-repeated (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

# One event per AE item group occurrence: subject, visit, visit repeat, item
# group, and item group repeat key. An occurrence without a reported term
# gives no record; a repeat key reused at a later visit starts a new event.
events = (
    odm.filter(pl.col("ItemGroupOID") == "IG.AE")
    .pivot(
        index=[
            "StudyOID",
            "SubjectKey",
            "StudyEventOID",
            "StudyEventRepeatKey",
            "ItemGroupRepeatKey",
        ],
        on="ItemOID",
        values="Value",
        aggregate_function="first",
    )
    .with_columns(
        AETERM=pl.col("IT.AE.AETERM"),
        AESTDTC=pl.col("IT.AE.AESTDTC"),
        AEENDTC=pl.col("IT.AE.AEENDTC"),
        AESEV=pl.col("IT.AE.AESEV"),
        AESER=pl.col("IT.AE.AESER"),
    )
    .filter(pl.col("AETERM").fill_null("") != "")
    # Visit order is screening, then baseline; then visit repeat and item
    # group repeat key. The same term in two occurrences stays two records.
    .with_columns(
        VISITORD=pl.when(pl.col("StudyEventOID") == "SCREENING")
        .then(0)
        .when(pl.col("StudyEventOID") == "BASELINE")
        .then(1)
        .otherwise(2),
        VISITREP=pl.col("StudyEventRepeatKey").cast(pl.Int64),
        IGREP=pl.col("ItemGroupRepeatKey").cast(pl.Int64),
    )
    .sort(["SubjectKey", "VISITORD", "VISITREP", "IGREP"])
    .with_columns(AESEQ=pl.col("SubjectKey").cum_count().over("SubjectKey"))
    .with_columns(DOMAIN=pl.lit("AE"), STUDYID=pl.col("StudyOID"), USUBJID=pl.col("SubjectKey"))
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "AESEQ",
        "AETERM",
        "AESTDTC",
        "AEENDTC",
        "AESEV",
        "AESER",
    )
)

Path("/app/output").mkdir(exist_ok=True)
events.write_csv("/app/output/ae.csv")
