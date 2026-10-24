# Reference solution for the yamaa benchmark adam-adis-log-titer (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import math

INPUT_CSV = "/app/input/is.csv"
OUTPUT_CSV = "/app/output/adis.csv"


def number(text):
    text = text.strip()
    return float(text) if text else None


def main():
    with open(INPUT_CSV, newline="") as f:
        rows = list(csv.DictReader(f))

    fieldnames = ["STUDYID", "USUBJID", "PARAMCD", "AVISIT", "AVAL"]
    with open(OUTPUT_CSV, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows:
            titer = number(row["ISSTRESN"])
            # A titer below the lower limit of quantitation gives no record;
            # a record whose titer has no value stays.
            if titer is not None and titer < number(row["ISLLOQ"]):
                continue
            writer.writerow(
                {
                    "STUDYID": row["STUDYID"],
                    "USUBJID": row["USUBJID"],
                    "PARAMCD": "NABLOG10",
                    "AVISIT": row["VISIT"],
                    "AVAL": "" if titer is None else repr(math.log10(titer)),
                }
            )


if __name__ == "__main__":
    main()
