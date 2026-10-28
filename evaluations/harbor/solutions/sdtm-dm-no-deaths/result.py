# Reference solution for the yamaa benchmark sdtm-dm-no-deaths (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the SDTM DM dataset with the death variables kept.

One record per subject (from DM_RAW). RFSTDTC is the reference start date
and DTHDTC the collected date of death, each blank when the subject has
none. DTHFL is "Y" when the subject has a date of death, and blank
otherwise. Both death variables stay in the dataset even when every value
is blank.
"""

import csv
from pathlib import Path

INPUT_DIR = Path("/app/input")
OUTPUT_DIR = Path("/app/output")
COLUMNS = ["DOMAIN", "STUDYID", "USUBJID", "RFSTDTC", "DTHDTC", "DTHFL"]


def main():
    with open(INPUT_DIR / "dm_raw.csv", newline="", encoding="utf-8") as handle:
        subjects = list(csv.DictReader(handle))

    rows = []
    for subject in subjects:
        dthdtc = subject["DTHDAT"].strip()
        rows.append(
            {
                "DOMAIN": "DM",
                "STUDYID": subject["STUDYID"],
                "USUBJID": subject["USUBJID"],
                "RFSTDTC": subject["RFSTDTC"].strip(),
                "DTHDTC": dthdtc,
                "DTHFL": "Y" if dthdtc else "",
            }
        )

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT_DIR / "dm.csv", "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
