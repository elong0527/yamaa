# Reference solution for the yamaa benchmark sdtm-lb-grading (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM LB dataset from LB_RAW.

One record per collected neutrophil (ANC) or hemoglobin (HGB) result,
with LBTOXGR the standard toxicity grade from the band set for the
test (and, for hemoglobin, the sex). Each band includes its lower
limit and excludes its upper one. Results for any other test, and
hemoglobin results with a sex other than "M" or "F", get no record.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

# (lower limit inclusive, grade) pairs in ascending order; a value
# takes the grade of the last band whose lower limit it reaches.
ANC_BANDS = [(0.0, "4"), (0.5, "3"), (1.0, "2"), (1.5, "1"), (1.8, "0")]
HGB_BANDS = {
    "M": [(0.0, "3"), (8.0, "2"), (10.0, "1"), (13.5, "0")],
    "F": [(0.0, "3"), (8.0, "2"), (10.0, "1"), (12.0, "0")],
}


def grade(value, bands):
    result = None
    for lower, band_grade in bands:
        if value >= lower:
            result = band_grade
    return result


def main():
    with open(os.path.join(INPUT_DIR, "lb_raw.csv"), newline="") as f:
        rows = list(csv.DictReader(f))

    out = []
    for row in rows:
        test = row["LBTESTCD"]
        value = float(row["LBSTRESN"])
        if test == "ANC":
            toxgr = grade(value, ANC_BANDS)
        elif test == "HGB":
            bands = HGB_BANDS.get(row["SEX"])
            if bands is None:
                continue
            toxgr = grade(value, bands)
        else:
            continue
        out.append(
            {
                "DOMAIN": "LB",
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "LBSEQ": row["LBSEQ"],
                "LBTESTCD": test,
                "LBSTRESN": row["LBSTRESN"],
                "LBTOXGR": toxgr,
            }
        )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    columns = [
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "LBSEQ",
        "LBTESTCD",
        "LBSTRESN",
        "LBTOXGR",
    ]
    with open(os.path.join(OUTPUT_DIR, "lb.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=columns)
        writer.writeheader()
        writer.writerows(out)


if __name__ == "__main__":
    main()
