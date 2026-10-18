# Reference solution for the yamaa benchmark adam-adlb-ordered-sum (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADLB from LB: every collected record plus one TOTAL record
per subject and visit, summed in the order the source records were
stored."""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = ["STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVISIT", "AVAL", "DTYPE"]


def main():
    with open(os.path.join(INPUT_DIR, "lb.csv"), newline="") as f:
        source = list(csv.DictReader(f))

    collected = []  # (group key, output row) in stored order
    totals = {}  # group key -> running total, None until a result is seen
    last_index = {}  # group key -> index in `collected` of its last record

    for row in source:
        key = (row["STUDYID"], row["USUBJID"], row["VISIT"])
        result = (row["LBSTRESN"] or "").strip()
        totals.setdefault(key, None)
        if result != "":
            value = float(result)
            totals[key] = value if totals[key] is None else totals[key] + value
        collected.append(
            (
                key,
                {
                    "STUDYID": row["STUDYID"],
                    "USUBJID": row["USUBJID"],
                    "PARAMCD": row["LBTESTCD"],
                    "PARAM": row["LBTEST"],
                    "AVISIT": row["VISIT"],
                    "AVAL": result,
                    "DTYPE": "",
                },
            )
        )
        last_index[key] = len(collected) - 1

    final_rows = []
    for i, (key, row) in enumerate(collected):
        final_rows.append(row)
        if last_index[key] == i:
            total = totals[key]
            final_rows.append(
                {
                    "STUDYID": key[0],
                    "USUBJID": key[1],
                    "PARAMCD": "TOTAL",
                    "PARAM": "Total of Components",
                    "AVISIT": key[2],
                    "AVAL": "" if total is None else repr(total),
                    "DTYPE": "CALCULATION",
                }
            )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "adlb.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(final_rows)


if __name__ == "__main__":
    main()
