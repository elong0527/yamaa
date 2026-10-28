# Reference solution for the yamaa benchmark adam-adlb-nonparametric-limits
# (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import statistics

ADSL_CSV = "/app/input/adsl.csv"
LB_CSV = "/app/input/lb.csv"
OUTPUT_CSV = "/app/output/adlb.csv"


def number(text):
    text = text.strip()
    return float(text) if text else None


def cell(value):
    return "" if value is None else repr(value)


def type7(values, p):
    """Hyndman and Fan's type 7 quantile of the values at probability p."""
    x = sorted(values)
    h = (len(x) - 1) * p + 1
    j = int(h)
    if j >= len(x):
        return x[-1]
    return x[j - 1] + (h - j) * (x[j] - x[j - 1])


def main():
    with open(ADSL_CSV, newline="") as f:
        sex = {
            (r["STUDYID"], r["USUBJID"]): r["SEX"] for r in csv.DictReader(f)
        }
    with open(LB_CSV, newline="") as f:
        lb = list(csv.DictReader(f))

    # Each parameter and sex's results, a test not done left out.
    results = {}
    for row in lb:
        value = number(row["LBSTRESN"])
        subject_sex = sex.get((row["STUDYID"], row["USUBJID"]), "")
        group = (row["LBTESTCD"], subject_sex)
        results.setdefault(group, [])
        if value is not None:
            results[group].append(value)

    summary = {}
    for group, values in results.items():
        low = type7(values, 0.025) if values else None
        high = type7(values, 0.975) if values else None
        variance = statistics.variance(values) if len(values) > 1 else None
        summary[group] = (low, high, variance)

    fieldnames = [
        "STUDYID", "USUBJID", "SEX", "PARAMCD", "AVAL", "ANRLO", "ANRHI",
        "AVALVAR",
    ]
    with open(OUTPUT_CSV, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        for row in lb:
            subject_sex = sex.get((row["STUDYID"], row["USUBJID"]), "")
            low, high, variance = summary[(row["LBTESTCD"], subject_sex)]
            writer.writerow(
                {
                    "STUDYID": row["STUDYID"],
                    "USUBJID": row["USUBJID"],
                    "SEX": subject_sex,
                    "PARAMCD": row["LBTESTCD"],
                    "AVAL": cell(number(row["LBSTRESN"])),
                    "ANRLO": cell(low),
                    "ANRHI": cell(high),
                    "AVALVAR": cell(variance),
                }
            )


if __name__ == "__main__":
    main()
