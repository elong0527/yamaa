# Reference solution for the yamaa benchmark adam-adsl-completion-flag (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADSL with a completion flag from ADSL_RAW and DS.

COMPLFL is "Y" when the subject has a disposition record with category
"DISPOSITION EVENT" in the "FOLLOW-UP" epoch whose standardized outcome
(DSDECOD) is "COMPLETED"; the record counts whether or not its
collection date (DSDTC) was recorded. Otherwise COMPLFL is "N".
"""

import csv

INPUT_DIR = "/app/input"
OUTPUT_PATH = "/app/output/adsl.csv"


def main():
    with open(f"{INPUT_DIR}/adsl_raw.csv", newline="") as f:
        subjects = list(csv.DictReader(f))

    completed = set()
    with open(f"{INPUT_DIR}/ds.csv", newline="") as f:
        for row in csv.DictReader(f):
            if (
                row["DSCAT"] == "DISPOSITION EVENT"
                and row["EPOCH"] == "FOLLOW-UP"
                and row["DSDECOD"] == "COMPLETED"
            ):
                completed.add((row["STUDYID"], row["USUBJID"]))

    with open(OUTPUT_PATH, "w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(["STUDYID", "USUBJID", "TRTSDT", "COMPLFL"])
        for subject in subjects:
            key = (subject["STUDYID"], subject["USUBJID"])
            writer.writerow(
                [
                    subject["STUDYID"],
                    subject["USUBJID"],
                    subject["TRTSDT"],
                    "Y" if key in completed else "N",
                ]
            )


if __name__ == "__main__":
    main()
