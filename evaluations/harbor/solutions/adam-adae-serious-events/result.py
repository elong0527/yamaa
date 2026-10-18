# Reference solution for the yamaa benchmark adam-adae-serious-events (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADAE: one record per serious adverse event (AESER == "Y").

Reads AE from /app/input/ae.csv and writes the serious events, in the
order collected, to /app/output/adae.csv with columns
STUDYID, USUBJID, AESEQ, AEDECOD, AESER.
"""

import csv
import os

INPUT_PATH = "/app/input/ae.csv"
OUTPUT_PATH = "/app/output/adae.csv"
COLUMNS = ["STUDYID", "USUBJID", "AESEQ", "AEDECOD", "AESER"]


def main():
    with open(INPUT_PATH, newline="", encoding="utf-8") as f:
        rows = list(csv.DictReader(f))

    serious = [row for row in rows if (row.get("AESER") or "") == "Y"]

    os.makedirs(os.path.dirname(OUTPUT_PATH), exist_ok=True)
    with open(OUTPUT_PATH, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        for row in serious:
            writer.writerow({col: row.get(col, "") for col in COLUMNS})


if __name__ == "__main__":
    main()
