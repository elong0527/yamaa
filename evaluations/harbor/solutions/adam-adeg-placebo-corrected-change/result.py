# Reference solution for the yamaa benchmark adam-adeg-placebo-corrected-change (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADEG with placebo-corrected change columns from ADEG_RAW.

Reads /app/input/adeg_raw.csv and writes /app/output/adeg.csv.
"""

import csv
from pathlib import Path

INPUT = Path("/app/input/adeg_raw.csv")
OUTPUT = Path("/app/output/adeg.csv")

COLUMNS = [
    "STUDYID", "USUBJID", "COHORT", "TRTA", "PARAMCD", "PARAM", "APERIOD",
    "APERDAY", "ATPT", "ATPTN", "AVAL", "AVALU", "ABLFL", "BASE", "CHG",
    "BASETYPE", "ACOMPFL", "COMP", "COMPBASE", "COMPCHG", "CCOMPCHG",
    "COMPTYPE",
]


def to_float(text):
    """Parse a CSV cell as a float, returning None for a blank cell."""
    if text is None or text.strip() == "":
        return None
    return float(text)


def fmt(value):
    """Format a computed float without rounding; blank for None."""
    if value is None:
        return ""
    return repr(value)


def mean(values):
    vals = [v for v in values if v is not None]
    if not vals:
        return None
    return sum(vals) / len(vals)


def main():
    with INPUT.open(newline="") as fh:
        rows = list(csv.DictReader(fh))

    def is_placebo(row):
        return row["TRTA"] == "Placebo"

    def is_baseline(row):
        return row["ABLFL"] == "Y"

    # Placebo comparison groups: every placebo record after baseline with
    # the same PARAMCD and the same ATPTN, from all cohorts together.
    groups = {}
    for row in rows:
        if is_placebo(row) and not is_baseline(row):
            key = (row["PARAMCD"], row["ATPTN"])
            groups.setdefault(key, []).append(row)

    stats = {}
    for key, members in groups.items():
        stats[key] = (
            mean([to_float(r["AVAL"]) for r in members]),
            mean([to_float(r["BASE"]) for r in members]),
            mean([to_float(r["CHG"]) for r in members]),
        )

    out_rows = []
    for row in rows:
        out = {col: row.get(col, "") for col in COLUMNS[:16]}
        baseline = is_baseline(row)
        placebo = is_placebo(row)

        acompfl = ""
        comp = compbase = compchg = None
        ccompchg = None
        comptype = ""
        if not baseline:
            key = (row["PARAMCD"], row["ATPTN"])
            comp, compbase, compchg = stats.get(key, (None, None, None))
            comptype = "Placebo group at " + row["ATPT"]
            if placebo:
                acompfl = "Y"
            else:
                chg = to_float(row["CHG"])
                if chg is not None and compchg is not None:
                    ccompchg = chg - compchg

        out["ACOMPFL"] = acompfl
        out["COMP"] = fmt(comp)
        out["COMPBASE"] = fmt(compbase)
        out["COMPCHG"] = fmt(compchg)
        out["CCOMPCHG"] = fmt(ccompchg)
        out["COMPTYPE"] = comptype
        out_rows.append(out)

    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    with OUTPUT.open("w", newline="") as fh:
        writer = csv.DictWriter(fh, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
