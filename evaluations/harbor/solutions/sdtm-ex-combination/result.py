# Reference solution for the yamaa benchmark sdtm-ex-combination (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM EX dataset from EX_RAW.

One record per administered combination component. EXSEQ numbers each
subject's administrations from 1, ordered by start date and, on the same
start date, by treatment name.
"""

import csv
from collections import defaultdict

INPUT_PATH = "/app/input/ex_raw.csv"
OUTPUT_PATH = "/app/output/ex.csv"

COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "EXSEQ",
    "EXTRT",
    "EXDOSE",
    "EXDOSU",
    "EXSTDTC",
    "EXENDTC",
    "EXADJ",
]


def main():
    with open(INPUT_PATH, newline="", encoding="utf-8") as f:
        raw_rows = list(csv.DictReader(f))

    by_subject = defaultdict(list)
    for row in raw_rows:
        by_subject[row["USUBJID"]].append(row)

    out_rows = []
    for usubjid in sorted(by_subject):
        rows = sorted(
            by_subject[usubjid],
            key=lambda r: (r["EXSTDTC"] or "", r["EXTRT"] or ""),
        )
        for seq, row in enumerate(rows, start=1):
            out_rows.append(
                {
                    "DOMAIN": "EX",
                    "STUDYID": row["STUDYID"],
                    "USUBJID": row["USUBJID"],
                    "EXSEQ": seq,
                    "EXTRT": row["EXTRT"],
                    # A dose of 0 is an administered dose, not missing;
                    # keep the collected value exactly as reported.
                    "EXDOSE": row["EXDOSE"],
                    "EXDOSU": row["EXDOSU"],
                    "EXSTDTC": row["EXSTDTC"],
                    "EXENDTC": row["EXENDTC"],
                    "EXADJ": row["EXADJ"] or "",
                }
            )

    with open(OUTPUT_PATH, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
