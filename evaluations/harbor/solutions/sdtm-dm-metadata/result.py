# Reference solution for the yamaa benchmark sdtm-dm-metadata (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM DM dataset from the collected DM_RAW dataset.

One record per subject. Reads /app/input/dm_raw.csv and writes
/app/output/dm.csv with columns DOMAIN, STUDYID, USUBJID, SUBJID,
SITEID, AGE, AGEU, SEX, COUNTRY.
"""

import csv
import os

INPUT_PATH = "/app/input/dm_raw.csv"
OUTPUT_PATH = "/app/output/dm.csv"

COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "SUBJID",
    "SITEID",
    "AGE",
    "AGEU",
    "SEX",
    "COUNTRY",
]


def build_record(row):
    studyid = (row.get("STUDYID") or "").strip()
    siteid = (row.get("SITEID") or "").strip()
    subjid = (row.get("SUBJID") or "").strip()
    age = (row.get("AGE") or "").strip()
    sex = (row.get("SEX") or "").strip()
    country = (row.get("COUNTRY") or "").strip()

    # AGE is the collected age in whole years; blank when none collected.
    age_value = ""
    if age:
        age_value = str(int(float(age)))

    return {
        "DOMAIN": "DM",
        "STUDYID": studyid,
        "USUBJID": "-".join([studyid, siteid, subjid]),
        "SUBJID": subjid,
        "SITEID": siteid,
        "AGE": age_value,
        "AGEU": "YEARS",
        "SEX": sex,
        "COUNTRY": country,
    }


def main():
    with open(INPUT_PATH, newline="", encoding="utf-8") as f:
        rows = list(csv.DictReader(f))

    records = [build_record(row) for row in rows]

    os.makedirs(os.path.dirname(OUTPUT_PATH), exist_ok=True)
    with open(OUTPUT_PATH, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
