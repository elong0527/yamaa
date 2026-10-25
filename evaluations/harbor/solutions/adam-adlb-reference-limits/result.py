# Reference solution for the yamaa benchmark adam-adlb-reference-limits (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import statistics

LB_CSV = "/app/input/lb.csv"
OUTPUT_CSV = "/app/output/adlb.csv"


def number(text):
    text = text.strip()
    return float(text) if text else None


def cell(value):
    return "" if value is None else repr(value)


def main():
    with open(LB_CSV, newline="") as f:
        lb = list(csv.DictReader(f))

    # Each parameter's results, a test not done left out.
    results = {}
    for row in lb:
        value = number(row["LBSTRESN"])
        if value is not None:
            results.setdefault(row["LBTESTCD"], []).append(value)

    z = statistics.NormalDist().inv_cdf(0.975)
    limits = {}
    for test, values in results.items():
        mean = statistics.fmean(values)
        sd = statistics.stdev(values)
        limits[test] = (mean - z * sd, mean + z * sd)

    fieldnames = ["STUDYID", "USUBJID", "PARAMCD", "AVAL", "ANRLO", "ANRHI"]
    with open(OUTPUT_CSV, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        for row in lb:
            low, high = limits.get(row["LBTESTCD"], (None, None))
            writer.writerow(
                {
                    "STUDYID": row["STUDYID"],
                    "USUBJID": row["USUBJID"],
                    "PARAMCD": row["LBTESTCD"],
                    "AVAL": cell(number(row["LBSTRESN"])),
                    "ANRLO": cell(low),
                    "ANRHI": cell(high),
                }
            )


if __name__ == "__main__":
    main()
