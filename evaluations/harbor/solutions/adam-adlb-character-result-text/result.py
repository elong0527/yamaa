# Reference solution for the yamaa benchmark adam-adlb-character-result-text (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from decimal import ROUND_HALF_UP, Decimal


def display(value, places, width=0):
    """The value rounded to `places` decimals with an exact half going away
    from zero, written with exactly that many digits after the point, without
    a minus sign on zero, and right-aligned in `width` characters; a wider
    text is written in full."""
    rounded = value.quantize(Decimal(1).scaleb(-places), rounding=ROUND_HALF_UP)
    if rounded == 0:
        rounded = abs(rounded)
    return f"{rounded:.{places}f}".rjust(width)


def main():
    with open("/app/input/lb.csv", newline="") as f:
        rows = list(csv.DictReader(f))

    out_rows = []
    for row in rows:
        # The value as the source writes it, read as an exact decimal.
        text = (row["AVAL"] or "").strip()
        aval = Decimal(text) if text else None
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "PARAMCD": row["PARAMCD"],
                "AVAL": text,
                "AVALC": "NOT DONE" if aval is None else display(aval, 2, 8),
                "AVALCS": "" if aval is None else display(aval, 1),
            }
        )

    fieldnames = ["STUDYID", "USUBJID", "PARAMCD", "AVAL", "AVALC", "AVALCS"]
    with open("/app/output/adlb.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
