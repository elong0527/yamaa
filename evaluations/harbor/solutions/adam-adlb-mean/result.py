# Reference solution for the yamaa benchmark adam-adlb-mean (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADLB from the collected LB dataset.

One record per collected laboratory record. AVAL is the collected
numeric result (LBSTRESN); AVALMEAN is the mean of the subject's
non-missing AVAL values for the record's parameter, repeated on every
record for that subject and parameter.
"""

import csv
from decimal import Decimal, InvalidOperation
from pathlib import Path

INPUT_DIR = Path("/app/input")
OUTPUT_DIR = Path("/app/output")


def parse_number(text):
    """Return the Decimal value of a collected result, or None."""
    if text is None:
        return None
    text = text.strip()
    if not text:
        return None
    try:
        return Decimal(text)
    except InvalidOperation:
        return None


def format_number(value):
    """Format a Decimal without rounding, dropping trailing zeros."""
    if value is None:
        return ""
    text = format(value.normalize(), "f")
    return text


def main():
    with open(INPUT_DIR / "lb.csv", newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))

    records = []
    totals = {}
    counts = {}
    for row in rows:
        aval = parse_number(row.get("LBSTRESN"))
        key = (row["USUBJID"], row["LBTESTCD"])
        if aval is not None:
            totals[key] = totals.get(key, Decimal(0)) + aval
            counts[key] = counts.get(key, 0) + 1
        records.append((row, aval))

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    fieldnames = ["STUDYID", "USUBJID", "LBSEQ", "PARAMCD", "AVAL", "AVALMEAN"]
    with open(OUTPUT_DIR / "adlb.csv", "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for row, aval in records:
            key = (row["USUBJID"], row["LBTESTCD"])
            mean = None
            if counts.get(key):
                mean = totals[key] / counts[key]
            writer.writerow(
                {
                    "STUDYID": row["STUDYID"],
                    "USUBJID": row["USUBJID"],
                    "LBSEQ": row["LBSEQ"],
                    "PARAMCD": row["LBTESTCD"],
                    "AVAL": format_number(aval),
                    "AVALMEAN": format_number(mean),
                }
            )


if __name__ == "__main__":
    main()
