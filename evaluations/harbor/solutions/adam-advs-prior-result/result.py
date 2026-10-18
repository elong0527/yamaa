# Reference solution for the yamaa benchmark adam-advs-prior-result (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADVS from VS with PREVAVALC, the closest earlier non-blank
AVALC for the same subject and series.

Rows are grouped by subject and series (rows with no SERIES value share
one series). Within a series, rows are ordered by AVISITN with a missing
AVISITN last; rows sharing an AVISITN, or both missing one, keep their
collected order. PREVAVALC is the most recent non-blank AVALC earlier in
that order, never the row's own AVALC, and blank when no earlier row in
the series has a non-blank AVALC.
"""

import pandas as pd

INPUT_VS = "/app/input/vs.csv"
OUTPUT_ADVS = "/app/output/advs.csv"

COLUMNS = ["STUDYID", "USUBJID", "VSSEQ", "SERIES", "AVISITN", "AVALC"]


def is_blank(value):
    return value is None or str(value).strip() == ""


def visit_key(row):
    """Sort key: AVISITN ascending, missing AVISITN last."""
    avisitn = row["AVISITN"]
    if is_blank(avisitn):
        return (1, 0.0)
    return (0, float(avisitn))


def main():
    vs = pd.read_csv(INPUT_VS, dtype=str, keep_default_na=False)
    for column in COLUMNS:
        if column not in vs.columns:
            vs[column] = ""
    vs = vs[COLUMNS].copy()
    vs["_collected"] = range(len(vs))

    prev = [""] * len(vs)

    # Group by subject and series; blank SERIES values share one series.
    for _, group in vs.groupby(["STUDYID", "USUBJID", "SERIES"],
                               sort=False):
        # Stable sort keeps collected order within equal AVISITN keys.
        # groupby preserves collected order within the group, and
        # sorted() is stable, so ties on AVISITN (or both missing) keep
        # their collected order.
        ordered = sorted(group.to_dict("records"), key=visit_key)
        last_non_blank = ""
        for row in ordered:
            prev[row["_collected"]] = last_non_blank
            if not is_blank(row["AVALC"]):
                last_non_blank = row["AVALC"]

    vs["PREVAVALC"] = prev
    out = vs[COLUMNS + ["PREVAVALC"]]
    out.to_csv(OUTPUT_ADVS, index=False)


if __name__ == "__main__":
    main()
