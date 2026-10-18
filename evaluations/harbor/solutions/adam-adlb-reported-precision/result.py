# Reference solution for the yamaa benchmark adam-adlb-reported-precision (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from decimal import Decimal, ROUND_HALF_UP, getcontext

getcontext().prec = 50

FOUR_PLACES = Decimal("0.0001")


def parse_number(text):
    text = (text or "").strip()
    if text == "":
        return None
    return Decimal(text)


def report(value):
    """Format a Decimal to exactly four decimal places, rounding an exact
    half away from zero. A missing value is reported as absent."""
    if value is None:
        return ""
    return str(value.quantize(FOUR_PLACES, rounding=ROUND_HALF_UP))


def main():
    with open("/app/input/lb.csv", newline="") as f:
        rows = list(csv.DictReader(f))

    out_rows = []
    for row in rows:
        aval = parse_number(row["AVAL"])
        anrlo = parse_number(row["ANRLO"])
        if aval is None or anrlo is None or anrlo == 0:
            ratio = None
        else:
            # Nothing is rounded before the report: the ratio keeps every
            # digit it was calculated with and is rounded once, in report().
            ratio = aval / anrlo
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "PARAMCD": row["PARAMCD"],
                "AVAL": report(aval),
                "ANRLO": report(anrlo),
                "R2ANRLO": report(ratio),
            }
        )

    fieldnames = ["STUDYID", "USUBJID", "PARAMCD", "AVAL", "ANRLO", "R2ANRLO"]
    with open("/app/output/adlb.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
