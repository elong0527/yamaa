# Reference solution for the yamaa benchmark adam-advs-first-observed-carry (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADVS from VS: carry each subject/parameter's first observed
result forward as BASEVAL from the first visit with a collected result
onward."""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def main():
    with open(os.path.join(INPUT_DIR, "vs.csv"), newline="") as f:
        rows = list(csv.DictReader(f))

    # For each subject and parameter, find the first visit (by visit
    # number) with a collected result, and that result.
    first = {}
    for row in rows:
        key = (row["USUBJID"], row["PARAMCD"])
        if row["AVAL"] == "":
            continue
        visit = int(row["AVISITN"])
        if key not in first or visit < first[key][0]:
            first[key] = (visit, row["AVAL"])

    out_rows = []
    for row in rows:
        key = (row["USUBJID"], row["PARAMCD"])
        baseval = ""
        if key in first and int(row["AVISITN"]) >= first[key][0]:
            baseval = first[key][1]
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "PARAMCD": row["PARAMCD"],
                "AVISITN": row["AVISITN"],
                "AVAL": row["AVAL"],
                "BASEVAL": baseval,
            }
        )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "advs.csv"), "w", newline="") as f:
        writer = csv.DictWriter(
            f,
            fieldnames=[
                "STUDYID",
                "USUBJID",
                "PARAMCD",
                "AVISITN",
                "AVAL",
                "BASEVAL",
            ],
        )
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
