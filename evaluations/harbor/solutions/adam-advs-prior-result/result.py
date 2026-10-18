# Reference solution for the yamaa benchmark adam-advs-prior-result (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADVS from the VS dataset: one record per collected row, with
PREVAVALC set to the closest earlier non-blank AVALC for the same
subject and series.

Rows with no SERIES value share one series. Within a subject and
series, rows are ordered by AVISITN with a missing AVISITN last, and
rows that tie keep their collected order. Blank results never serve as
a previous value, and a previous value never crosses a series or a
subject.
"""

import csv

INPUT_VS = "/app/input/vs.csv"
OUTPUT_ADVS = "/app/output/advs.csv"

COLUMNS = ["STUDYID", "USUBJID", "VSSEQ", "SERIES", "AVISITN", "AVALC",
           "PREVAVALC"]


def visit_key(row):
    """Sort key: AVISITN ascending, a missing AVISITN last.

    The sort is stable, so rows sharing an AVISITN, or both missing
    one, keep their collected order.
    """
    text = row["AVISITN"].strip()
    if text == "":
        return (1, 0.0)
    return (0, float(text))


def main():
    with open(INPUT_VS, newline="") as handle:
        rows = list(csv.DictReader(handle))

    groups = {}
    for index, row in enumerate(rows):
        key = (row["USUBJID"], row["SERIES"])
        groups.setdefault(key, []).append(index)

    previous = {}
    for indexes in groups.values():
        last_nonblank = ""
        for index in sorted(indexes, key=lambda i: visit_key(rows[i])):
            previous[index] = last_nonblank
            avalc = rows[index]["AVALC"]
            if avalc.strip() != "":
                last_nonblank = avalc

    with open(OUTPUT_ADVS, "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        for index, row in enumerate(rows):
            out = {name: row[name] for name in COLUMNS if name != "PREVAVALC"}
            out["PREVAVALC"] = previous[index]
            writer.writerow(out)


if __name__ == "__main__":
    main()
