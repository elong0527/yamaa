# Reference solution for the yamaa benchmark sdtm-dm-race-ethnicity (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

RACE_MAP = {
    "White": "WHITE",
    "Asian": "ASIAN",
    "Black or African American": "BLACK OR AFRICAN AMERICAN",
    "American Indian or Alaska Native": "AMERICAN INDIAN OR ALASKA NATIVE",
    "Native Hawaiian or Other Pacific Islander": "NATIVE HAWAIIAN OR OTHER PACIFIC ISLANDER",
    "Other, specify: Fijian": "OTHER",
    "Subject refused": "UNKNOWN",
    "Not reported": "NOT REPORTED",
}

ETHNIC_MAP = {
    "Hispanic or Latino": "HISPANIC OR LATINO",
    "Not Hispanic or Latino": "NOT HISPANIC OR LATINO",
    "Not reported": "NOT REPORTED",
    "Unknown": "UNKNOWN",
}


def usubjid(study: str, subject: str) -> str:
    if subject.startswith(study):
        return subject
    return f"{study}-{subject}"


# Distinct collected answers per subject.
race_rows = odm.filter(pl.col("ItemOID") == "IT.DM.RACE", pl.col("Value") != "")
ethnic_rows = odm.filter(pl.col("ItemOID") == "IT.DM.ETHNIC", pl.col("Value") != "")

subjects = (
    odm.select("StudyOID", "SubjectKey")
    .unique()
    .with_columns(
        STUDYID=pl.col("StudyOID"),
        USUBJID=pl.struct(["StudyOID", "SubjectKey"]).map_elements(
            lambda s: usubjid(s["StudyOID"], s["SubjectKey"]),
            return_dtype=pl.String,
        ),
    )
)

race_distinct = (
    race_rows.select("StudyOID", "SubjectKey", "Value")
    .unique()
    .group_by("StudyOID", "SubjectKey")
    .agg(pl.col("Value").alias("answers"))
)

ethnic_one = (
    ethnic_rows.select("StudyOID", "SubjectKey", "Value")
    .group_by("StudyOID", "SubjectKey")
    .agg(pl.col("Value").first().alias("ethnic_answer"))
)

dm = (
    subjects.join(race_distinct, on=["StudyOID", "SubjectKey"], how="left")
    .join(ethnic_one, on=["StudyOID", "SubjectKey"], how="left")
    .with_columns(
        DOMAIN=pl.lit("DM"),
        SUBJID=pl.col("USUBJID"),
        RACE=pl.col("answers").map_elements(
            lambda ans: (
                None
                if ans is None or len(ans) == 0
                else (
                    "MULTIPLE"
                    if len(ans) >= 2
                    else RACE_MAP.get(ans[0])
                )
            ),
            return_dtype=pl.String,
        ),
        ETHNIC=pl.col("ethnic_answer").map_elements(
            lambda v: ETHNIC_MAP.get(v) if v is not None else None,
            return_dtype=pl.String,
        ),
    )
    .select("DOMAIN", "STUDYID", "USUBJID", "SUBJID", "RACE", "ETHNIC")
    .sort("USUBJID")
)

# SUPPDM: one record per race for MULTIPLE subjects, alphabetical by answer.
multi = (
    subjects.join(race_distinct, on=["StudyOID", "SubjectKey"], how="left")
    .with_columns(
        RACE_TMP=pl.col("answers").map_elements(
            lambda ans: (
                None
                if ans is None or len(ans) == 0
                else ("MULTIPLE" if len(ans) >= 2 else RACE_MAP.get(ans[0]))
            ),
            return_dtype=pl.String,
        )
    )
    .filter(pl.col("RACE_TMP") == "MULTIPLE")
)

supp_rows: list[dict] = []
for row in multi.to_dicts():
    studyid = row["StudyOID"]
    usubjid_val = row["USUBJID"]
    answers = sorted(row["answers"])
    for i, ans in enumerate(answers, start=1):
        supp_rows.append(
            {
                "STUDYID": studyid,
                "RDOMAIN": "DM",
                "USUBJID": usubjid_val,
                "IDVAR": "USUBJID",
                "IDVARVAL": usubjid_val,
                "QNAM": f"RACE{i}",
                "QLABEL": f"Race {i}",
                "QVAL": RACE_MAP.get(ans),
                "QORIG": "CRF",
                "QEVAL": None,
            }
        )

if supp_rows:
    suppdm = pl.DataFrame(
        supp_rows,
        schema={
            "STUDYID": pl.String,
            "RDOMAIN": pl.String,
            "USUBJID": pl.String,
            "IDVAR": pl.String,
            "IDVARVAL": pl.String,
            "QNAM": pl.String,
            "QLABEL": pl.String,
            "QVAL": pl.String,
            "QORIG": pl.String,
            "QEVAL": pl.String,
        },
    ).sort(["USUBJID", "QNAM"])
else:
    suppdm = pl.DataFrame(
        [],
        schema={
            "STUDYID": pl.String,
            "RDOMAIN": pl.String,
            "USUBJID": pl.String,
            "IDVAR": pl.String,
            "IDVARVAL": pl.String,
            "QNAM": pl.String,
            "QLABEL": pl.String,
            "QVAL": pl.String,
            "QORIG": pl.String,
            "QEVAL": pl.String,
        },
    )

Path("/app/output").mkdir(exist_ok=True)
dm.write_csv("/app/output/dm.csv")
suppdm.write_csv("/app/output/suppdm.csv")
