# Reference solution for the yamaa benchmark adam-adsl-demographics (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADSL demographics from the DM dataset.

One record per subject. Reads /app/input/dm.csv and writes
/app/output/adsl.csv with columns:
STUDYID, USUBJID, SEX, SEXN, RACE, RACEN, AGE, AGEGR1
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

SEXN_MAP = {"M": 1, "F": 2, "U": 0}
RACEN_MAP = {
    "WHITE": 1,
    "BLACK OR AFRICAN AMERICAN": 2,
    "ASIAN": 3,
    "MULTIPLE": 4,
}


def main():
    with open(os.path.join(INPUT_DIR, "dm.csv"), newline="") as f:
        dm = list(csv.DictReader(f))

    rows = []
    for rec in dm:
        sex_raw = (rec.get("SEX") or "").strip()
        sex = sex_raw if sex_raw else "U"
        sexn = SEXN_MAP.get(sex, 0)

        race = (rec.get("RACE") or "").strip()
        if race == "":
            racen = ""
        elif race in RACEN_MAP:
            racen = RACEN_MAP[race]
        else:
            racen = 99

        age_raw = (rec.get("AGE") or "").strip()
        if age_raw == "":
            age = ""
            agegr1 = "UNKNOWN"
        else:
            age = int(float(age_raw))
            if age < 18:
                agegr1 = "<18"
            elif age < 65:
                agegr1 = "18-64"
            else:
                agegr1 = ">=65"

        rows.append(
            {
                "STUDYID": rec.get("STUDYID", ""),
                "USUBJID": rec.get("USUBJID", ""),
                "SEX": sex,
                "SEXN": sexn,
                "RACE": race,
                "RACEN": racen,
                "AGE": age,
                "AGEGR1": agegr1,
            }
        )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    fieldnames = ["STUDYID", "USUBJID", "SEX", "SEXN", "RACE", "RACEN", "AGE", "AGEGR1"]
    with open(os.path.join(OUTPUT_DIR, "adsl.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
