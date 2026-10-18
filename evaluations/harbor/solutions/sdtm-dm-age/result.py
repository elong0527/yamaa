# Reference solution for the yamaa benchmark sdtm-dm-age (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM DM dataset (one record per subject) from the ODM extract.

Reads /app/input/odm.csv (long format: one row per collected item) and
writes /app/output/dm.csv with columns
DOMAIN, STUDYID, USUBJID, RFICDTC, BRTHDTC, AGE, AGEU.
"""

import csv
import os
from calendar import isleap

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

BIRTH_ITEM = "IT.DM.BRTHDT"
CONSENT_ITEM = "IT.DM.RFICDTC"


def parse_full_date(text):
    """Return (year, month, day) if text is a full ISO date, else None."""
    if not text:
        return None
    parts = text.strip().split("-")
    if len(parts) != 3:
        return None
    try:
        year, month, day = (int(p) for p in parts)
    except ValueError:
        return None
    return year, month, day


def completed_years(birth, consent):
    """Whole calendar years between birth and consent dates.

    One year fewer when the birthday falls after the consent date's
    month and day. A February 29 birthday has its anniversary on
    February 28 in a common year.
    """
    by, bm, bd = birth
    cy, cm, cd = consent
    if (bm, bd) == (2, 29) and not isleap(cy):
        anniversary = (2, 28)
    else:
        anniversary = (bm, bd)
    years = cy - by
    if anniversary > (cm, cd):
        years -= 1
    return years


def main():
    with open(os.path.join(INPUT_DIR, "odm.csv"), newline="", encoding="utf-8") as f:
        rows = list(csv.DictReader(f))

    subjects = {}
    order = []
    for row in rows:
        key = (row["StudyOID"], row["SubjectKey"])
        if key not in subjects:
            subjects[key] = {"RFICDTC": "", "BRTHDTC": ""}
            order.append(key)
        item = row["ItemOID"]
        value = (row["Value"] or "").strip()
        if item == CONSENT_ITEM and not subjects[key]["RFICDTC"]:
            subjects[key]["RFICDTC"] = value
        elif item == BIRTH_ITEM and not subjects[key]["BRTHDTC"]:
            subjects[key]["BRTHDTC"] = value

    out_rows = []
    for study_oid, subject_key in order:
        vals = subjects[(study_oid, subject_key)]
        rficdtc = vals["RFICDTC"]
        brthdtc = vals["BRTHDTC"]

        age = ""
        birth = parse_full_date(brthdtc)
        consent = parse_full_date(rficdtc)
        if birth is not None and consent is not None:
            age = str(completed_years(birth, consent))
        ageu = "YEARS" if age else ""

        out_rows.append(
            {
                "DOMAIN": "DM",
                "STUDYID": study_oid,
                "USUBJID": subject_key,
                "RFICDTC": rficdtc,
                "BRTHDTC": brthdtc,
                "AGE": age,
                "AGEU": ageu,
            }
        )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    fieldnames = ["DOMAIN", "STUDYID", "USUBJID", "RFICDTC", "BRTHDTC", "AGE", "AGEU"]
    with open(os.path.join(OUTPUT_DIR, "dm.csv"), "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
