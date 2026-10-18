# Reference solution for the yamaa benchmark sdtm-dm-race-ethnicity (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build DM and SUPPDM from the ODM extract.

Reads /app/input/odm.csv (long format: one row per collected item value)
and writes /app/output/dm.csv (one record per subject) and
/app/output/suppdm.csv (one record per distinct race marked, for
subjects whose RACE is "MULTIPLE").
"""

import csv
from collections import OrderedDict

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

RACE_MAP = {
    "White": "WHITE",
    "Asian": "ASIAN",
    "Black or African American": "BLACK OR AFRICAN AMERICAN",
    "American Indian or Alaska Native": "AMERICAN INDIAN OR ALASKA NATIVE",
    "Other, specify: Fijian": "OTHER",
    "Not reported": "NOT REPORTED",
    "Subject refused": "UNKNOWN",
}

ETHNIC_MAP = {
    "Hispanic or Latino": "HISPANIC OR LATINO",
    "Not Hispanic or Latino": "NOT HISPANIC OR LATINO",
    "Not reported": "NOT REPORTED",
    "Unknown": "UNKNOWN",
}

DM_COLUMNS = ["DOMAIN", "STUDYID", "USUBJID", "SUBJID", "RACE", "ETHNIC"]
SUPPDM_COLUMNS = [
    "STUDYID",
    "RDOMAIN",
    "USUBJID",
    "IDVAR",
    "IDVARVAL",
    "QNAM",
    "QLABEL",
    "QVAL",
    "QORIG",
    "QEVAL",
]


def main():
    with open(f"{INPUT_DIR}/odm.csv", newline="", encoding="utf-8") as f:
        rows = list(csv.DictReader(f))

    # Collect, per subject, every marked race answer and the ethnicity
    # answer, keeping subjects in order of first appearance.
    subjects = OrderedDict()
    for row in rows:
        key = (row["StudyOID"], row["SubjectKey"])
        entry = subjects.setdefault(key, {"races": [], "ethnic": ""})
        if row["ItemOID"] == "IT.DM.RACE":
            if row["Value"]:
                entry["races"].append(row["Value"])
        elif row["ItemOID"] == "IT.DM.ETHNIC":
            entry["ethnic"] = row["Value"]

    dm_rows = []
    suppdm_rows = []
    for (studyid, usubjid), entry in subjects.items():
        distinct_races = sorted(set(entry["races"]))
        if len(distinct_races) > 1:
            race = "MULTIPLE"
        elif len(distinct_races) == 1:
            race = RACE_MAP.get(distinct_races[0], "")
        else:
            race = ""
        ethnic = ETHNIC_MAP.get(entry["ethnic"], "") if entry["ethnic"] else ""

        dm_rows.append(
            {
                "DOMAIN": "DM",
                "STUDYID": studyid,
                "USUBJID": usubjid,
                "SUBJID": usubjid,
                "RACE": race,
                "ETHNIC": ethnic,
            }
        )

        if race == "MULTIPLE":
            for i, answer in enumerate(distinct_races, start=1):
                suppdm_rows.append(
                    {
                        "STUDYID": studyid,
                        "RDOMAIN": "DM",
                        "USUBJID": usubjid,
                        "IDVAR": "USUBJID",
                        "IDVARVAL": usubjid,
                        "QNAM": f"RACE{i}",
                        "QLABEL": f"Race {i}",
                        "QVAL": RACE_MAP.get(answer, ""),
                        "QORIG": "CRF",
                        "QEVAL": "",
                    }
                )

    with open(f"{OUTPUT_DIR}/dm.csv", "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=DM_COLUMNS)
        writer.writeheader()
        writer.writerows(dm_rows)

    with open(f"{OUTPUT_DIR}/suppdm.csv", "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=SUPPDM_COLUMNS)
        writer.writeheader()
        writer.writerows(suppdm_rows)


if __name__ == "__main__":
    main()
