# Reference solution for the yamaa benchmark adam-adeg-time-matched-baseline (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADEG from ADEG_RAW with time-matched period baselines.

For each subject, parameter and analysis period, the baseline day is
APERDAY -1. BASE is the AVAL recorded at the same nominal time point on
that period's baseline day; CHG is AVAL - BASE (blank on the baseline
day itself and wherever no baseline value exists).
"""

import csv
from decimal import Decimal

INPUT_PATH = "/app/input/adeg_raw.csv"
OUTPUT_PATH = "/app/output/adeg.csv"

FIELDS = [
    "STUDYID", "USUBJID", "PARAMCD", "PARAM", "APERIOD", "APERDAY",
    "ATPT", "ATPTN", "AVAL", "AVALU", "ABLFL", "BASE", "CHG", "BASETYPE",
]


def main():
    with open(INPUT_PATH, newline="") as f:
        rows = list(csv.DictReader(f))

    # AVAL on each period's baseline day, by subject/parameter/period/time point.
    baseline = {}
    for row in rows:
        if row["APERDAY"] == "-1":
            key = (row["USUBJID"], row["PARAMCD"], row["APERIOD"], row["ATPT"])
            baseline[key] = row["AVAL"]

    out_rows = []
    for row in rows:
        is_baseline_day = row["APERDAY"] == "-1"
        key = (row["USUBJID"], row["PARAMCD"], row["APERIOD"], row["ATPT"])
        base = baseline.get(key, "")
        if is_baseline_day or base == "":
            chg = ""
        else:
            chg = str(Decimal(row["AVAL"]) - Decimal(base))
        out = {name: row[name] for name in FIELDS[:10]}
        out["ABLFL"] = "Y" if is_baseline_day else ""
        out["BASE"] = base
        out["CHG"] = chg
        out["BASETYPE"] = (
            "Period " + row["APERIOD"] + " Baseline Day " + row["ATPT"]
        )
        out_rows.append(out)

    with open(OUTPUT_PATH, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=FIELDS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
