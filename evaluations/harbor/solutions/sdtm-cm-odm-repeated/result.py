# Reference solution for the yamaa benchmark sdtm-cm-odm-repeated (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

# One course per form repeat within a visit repeat: subject, visit, visit
# repeat, and form repeat. A course without a reported treatment name gives
# no record; a repeat number reused at a later visit starts a new course.
courses = (
    odm.filter(pl.col("ItemGroupOID") == "IG.CM")
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
        CMTRT=pl.col("IT.CM.CMTRT"),
        CMSTDTC=pl.col("IT.CM.CMSTDTC"),
        CMENDTC=pl.col("IT.CM.CMENDTC"),
        CMROUTE=pl.col("IT.CM.CMROUTE"),
        CMINDC=pl.col("IT.CM.CMINDC"),
    )
    .filter(pl.col("CMTRT").fill_null("") != "")
    # Courses run by visit (screening before baseline), visit repeat, and
    # form repeat; the same medication in two courses stays two records.
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
    .with_columns(CMSEQ=pl.col("SubjectKey").cum_count().over("SubjectKey"))
    .with_columns(DOMAIN=pl.lit("CM"), STUDYID=pl.col("StudyOID"), USUBJID=pl.col("SubjectKey"))
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "CMSEQ",
        "CMTRT",
        "CMSTDTC",
        "CMENDTC",
        "CMROUTE",
        "CMINDC",
    )
)

Path("/app/output").mkdir(exist_ok=True)
courses.write_csv("/app/output/cm.csv")
