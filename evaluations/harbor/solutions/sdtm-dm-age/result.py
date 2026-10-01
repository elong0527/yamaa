# Reference solution for the yamaa benchmark sdtm-dm-age (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import re
from datetime import date
from pathlib import Path

import polars as pl

odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

FULL = re.compile(r"^\d{4}-\d{2}-\d{2}$")


def is_leap(year: int) -> bool:
    return year % 4 == 0 and (year % 100 != 0 or year % 400 == 0)


def whole_years(birth: str, consent: str) -> int | None:
    # Whole calendar years between birth and consent; a year-month or
    # year-only birth date cannot give completed years.
    if not FULL.match(birth or "") or not FULL.match(consent or ""):
        return None
    by, bm, bd = int(birth[0:4]), int(birth[5:7]), int(birth[8:10])
    cy, cm, cd = int(consent[0:4]), int(consent[5:7]), int(consent[8:10])
    try:
        date(by, bm, bd)
        date(cy, cm, cd)
    except ValueError:
        return None
    # A February 29 birthday has its anniversary on February 28 in a
    # common year.
    if (bm, bd) == (2, 29) and not is_leap(cy):
        anniversary = (2, 28)
    else:
        anniversary = (bm, bd)
    return cy - by - (1 if (cm, cd) < anniversary else 0)


wide = odm.filter(pl.col("ItemGroupOID") == "IG.DM").pivot(
    index=["StudyOID", "SubjectKey"],
    on="ItemOID",
    values="Value",
    aggregate_function="first",
)

rows = []
for record in wide.to_dicts():
    rfic = record.get("IT.DM.RFICDTC") or None
    brth = record.get("IT.DM.BRTHDT") or None
    # A missing birth date leaves the birth date, age, and units empty.
    if not brth:
        brth, age, ageu = None, None, None
    else:
        age = whole_years(brth, rfic or "")
        ageu = "YEARS" if age is not None else None
    rows.append(
        {
            "DOMAIN": "DM",
            "STUDYID": record["StudyOID"],
            "USUBJID": record["SubjectKey"],
            "RFICDTC": rfic,
            "BRTHDTC": brth,
            "AGE": age,
            "AGEU": ageu,
        }
    )

dm = pl.DataFrame(
    rows,
    schema={
        "DOMAIN": pl.String,
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "RFICDTC": pl.String,
        "BRTHDTC": pl.String,
        "AGE": pl.Int64,
        "AGEU": pl.String,
    },
).sort("USUBJID")

Path("/app/output").mkdir(exist_ok=True)
dm.write_csv("/app/output/dm.csv")
