# Reference solution for the yamaa benchmark sdtm-dm-basic (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM DM dataset from the long-format ODM extract.

One record per subject, with columns:
DOMAIN, STUDYID, USUBJID, SUBJID, SEX, AGE, ARM, ACTARM, ARMNRS
"""

import csv
from collections import OrderedDict

INPUT_PATH = "/app/input/odm.csv"
OUTPUT_PATH = "/app/output/dm.csv"

ITEM_SEX = "IT.DM.SEX"
ITEM_AGE = "IT.DM.AGE"
ITEM_ARM = "IT.DM.ARM"


def main():
    subjects = OrderedDict()  # SubjectKey -> {ItemOID: Value}
    study_ids = {}  # SubjectKey -> StudyOID
    with open(INPUT_PATH, newline="", encoding="utf-8") as fh:
        for row in csv.DictReader(fh):
            key = row["SubjectKey"]
            if key not in subjects:
                subjects[key] = {}
                study_ids[key] = row["StudyOID"]
            subjects[key][row["ItemOID"]] = row["Value"]

    rows = []
    for key, items in subjects.items():
        collected_sex = items.get(ITEM_SEX, "")
        if collected_sex == "Male":
            sex = "M"
        elif collected_sex == "Female":
            sex = "F"
        else:
            sex = "U"
        age = items.get(ITEM_AGE, "") or ""
        arm = items.get(ITEM_ARM, "") or ""
        actarm = arm
        armnrs = "" if arm else "Not assigned to treatment arm"
        rows.append(
            {
                "DOMAIN": "DM",
                "STUDYID": study_ids[key],
                "USUBJID": key,
                "SUBJID": key,
                "SEX": sex,
                "AGE": age,
                "ARM": arm,
                "ACTARM": actarm,
                "ARMNRS": armnrs,
            }
        )

    fieldnames = [
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "SUBJID",
        "SEX",
        "AGE",
        "ARM",
        "ACTARM",
        "ARMNRS",
    ]
    with open(OUTPUT_PATH, "w", newline="", encoding="utf-8") as fh:
        writer = csv.DictWriter(fh, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
