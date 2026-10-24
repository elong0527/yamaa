# Reference solution for the yamaa benchmark adam-adsl-bmi-quartile-group (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import math

INPUT_CSV = "/app/input/adsl.csv"
OUTPUT_CSV = "/app/output/adsl.csv"


def quantile(values, p):
    """R's default quantile (type 7) of the values at proportion p."""
    xs = sorted(values)
    h = (len(xs) - 1) * p + 1
    j = math.floor(h)
    g = h - j
    if g == 0 or j == len(xs):
        return xs[j - 1]
    return xs[j - 1] + g * (xs[j] - xs[j - 1])


def group(bmi, q1, med, q3):
    if bmi is None:
        return ""
    if bmi <= q1:
        return "Quartile 1"
    if bmi <= med:
        return "Quartile 2"
    if bmi <= q3:
        return "Quartile 3"
    return "Quartile 4"


def cell(value):
    return "" if value is None else repr(value)


def main():
    with open(INPUT_CSV, newline="") as f:
        rows = list(csv.DictReader(f))
    for row in rows:
        text = row["BMIBL"].strip()
        row["value"] = float(text) if text else None

    cutpoints = {}
    for study in {row["STUDYID"] for row in rows}:
        values = [r["value"] for r in rows if r["STUDYID"] == study and r["value"] is not None]
        cutpoints[study] = tuple(quantile(values, p) for p in (0.25, 0.5, 0.75))

    fieldnames = ["STUDYID", "USUBJID", "BMIBL", "BMIBLQ1", "BMIBLMED", "BMIBLQ3", "BMIBLGR1"]
    with open(OUTPUT_CSV, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows:
            q1, med, q3 = cutpoints[row["STUDYID"]]
            writer.writerow(
                {
                    "STUDYID": row["STUDYID"],
                    "USUBJID": row["USUBJID"],
                    "BMIBL": cell(row["value"]),
                    "BMIBLQ1": cell(q1),
                    "BMIBLMED": cell(med),
                    "BMIBLQ3": cell(q3),
                    "BMIBLGR1": group(row["value"], q1, med, q3),
                }
            )


if __name__ == "__main__":
    main()
