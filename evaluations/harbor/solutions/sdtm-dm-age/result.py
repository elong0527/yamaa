# Reference solution for the yamaa benchmark sdtm-dm-age (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM DM dataset (one record per subject) from the ODM extract.

Reads /app/input/odm.csv, writes /app/output/dm.csv with columns
DOMAIN, STUDYID, USUBJID, RFICDTC, BRTHDTC, AGE, AGEU.

AGE is the whole calendar years between the birth date and the informed
consent date; a February 29 birthday has its anniversary on February 28
in a common year. Birth dates collected only to the month or the year
cannot give completed years, so AGE (and AGEU) stay empty for them.
"""

import calendar

import pandas as pd

INPUT_ODM = "/app/input/odm.csv"
OUTPUT_DM = "/app/output/dm.csv"


def parse_full_date(text):
    """Return (year, month, day) for a full ISO date, else None."""
    parts = text.split("-")
    if len(parts) != 3:
        return None
    try:
        year, month, day = (int(p) for p in parts)
    except ValueError:
        return None
    return year, month, day


def completed_years(birth_text, consent_text):
    """Whole calendar years from birth date to consent date, or None.

    None when either date is missing or the birth date was collected
    only to the month or the year.
    """
    if not birth_text or not consent_text:
        return None
    birth = parse_full_date(birth_text)
    consent = parse_full_date(consent_text)
    if birth is None or consent is None:
        return None
    by, bm, bd = birth
    cy, cm, cd = consent
    # A February 29 birthday has its anniversary on February 28 in a
    # common year.
    if bm == 2 and bd == 29 and not calendar.isleap(cy):
        bm, bd = 2, 28
    years = cy - by
    if (cm, cd) < (bm, bd):
        years -= 1
    return years


def main():
    odm = pd.read_csv(INPUT_ODM, dtype=str, keep_default_na=False)

    subjects = {}
    order = []
    for row in odm.itertuples(index=False):
        key = (row.StudyOID, row.SubjectKey)
        if key not in subjects:
            subjects[key] = {"RFICDTC": "", "BRTHDTC": ""}
            order.append(key)
        if row.ItemOID == "IT.DM.RFICDTC":
            subjects[key]["RFICDTC"] = row.Value
        elif row.ItemOID == "IT.DM.BRTHDT":
            subjects[key]["BRTHDTC"] = row.Value

    records = []
    for study_oid, subject_key in order:
        values = subjects[(study_oid, subject_key)]
        age = completed_years(values["BRTHDTC"], values["RFICDTC"])
        records.append(
            {
                "DOMAIN": "DM",
                "STUDYID": study_oid,
                "USUBJID": subject_key,
                "RFICDTC": values["RFICDTC"],
                "BRTHDTC": values["BRTHDTC"],
                "AGE": "" if age is None else age,
                "AGEU": "" if age is None else "YEARS",
            }
        )

    dm = pd.DataFrame(
        records,
        columns=["DOMAIN", "STUDYID", "USUBJID", "RFICDTC", "BRTHDTC", "AGE", "AGEU"],
    )
    dm.to_csv(OUTPUT_DM, index=False)


if __name__ == "__main__":
    main()
