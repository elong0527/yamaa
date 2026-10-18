# Reference solution for the yamaa benchmark adam-adlbc-row-window (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADLBC from LB: one record per subject per parameter per visit,
with the previous visit's result and the change since the previous visit."""

import csv
from decimal import Decimal, InvalidOperation

INPUT_CSV = "/app/input/lb.csv"
OUTPUT_CSV = "/app/output/adlbc.csv"

PARAM_BY_TEST = {"ALB": "_ALB", "BILI": "_BILI"}


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


def fmt(value):
    """Render a Decimal without exponent notation, or blank for None."""
    if value is None:
        return ""
    return format(value, "f")


def main():
    with open(INPUT_CSV, newline="") as handle:
        rows = list(csv.DictReader(handle))

    # Keep only ALB/BILI records with a numeric result.
    kept = []
    for row in rows:
        testcd = (row.get("LBTESTCD") or "").strip()
        if testcd not in PARAM_BY_TEST:
            continue
        aval = parse_number(row.get("LBSTRESN"))
        if aval is None:
            continue
        kept.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "PARAMCD": PARAM_BY_TEST[testcd],
                "AVISITN": int(row["VISITNUM"]),
                "AVAL": aval,
            }
        )

    # Order by subject, parameter, then visit number so each row's
    # previous value is the latest earlier visit for the same
    # subject and parameter.
    kept.sort(key=lambda r: (r["STUDYID"], r["USUBJID"], r["PARAMCD"], r["AVISITN"]))

    previous = {}
    out_rows = []
    for record in kept:
        key = (record["STUDYID"], record["USUBJID"], record["PARAMCD"])
        prev = previous.get(key)
        chg = record["AVAL"] - prev if prev is not None else None
        out_rows.append(
            [
                record["STUDYID"],
                record["USUBJID"],
                record["PARAMCD"],
                str(record["AVISITN"]),
                fmt(record["AVAL"]),
                fmt(prev),
                fmt(chg),
            ]
        )
        previous[key] = record["AVAL"]

    with open(OUTPUT_CSV, "w", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(
            ["STUDYID", "USUBJID", "PARAMCD", "AVISITN", "AVAL", "PREV_AVAL", "CHG"]
        )
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
