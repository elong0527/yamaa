# Reference solution for the yamaa benchmark adam-adpp-carryover-exclusion (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADPP from PC and PP: pre-dose carryover exclusion flag.

One record per subject, period, and pharmacokinetic parameter (from PP).
PRECONC is the concentration of the period's pre-dose PC sample (planned
"PREDOSE", matched to the parameter by reference dose time); a character
result beginning with "<" counts as 0. PREPCMAX is PRECONC as a
percentage of the period's Cmax. ANL01FL is blank on every parameter of
a period whose PREPCMAX is greater than 5, and "Y" otherwise.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_csv(name):
    with open(os.path.join(INPUT_DIR, name), newline="") as f:
        return list(csv.DictReader(f))


def fmt(value):
    """Format a float for CSV: integers without a decimal point."""
    if value is None:
        return ""
    if value == int(value):
        return str(int(value))
    return repr(value)


def main():
    pc = read_csv("pc.csv")
    pp = read_csv("pp.csv")

    # Pre-dose concentrations keyed by (subject, reference dose time).
    preconc = {}
    for row in pc:
        if row["PCTPT"] != "PREDOSE":
            continue
        key = (row["USUBJID"], row["PCRFTDTC"])
        if row["PCSTAT"] == "NOT DONE":
            preconc[key] = None
        elif row["PCSTRESC"].startswith("<"):
            preconc[key] = 0.0
        elif row["PCSTRESN"]:
            preconc[key] = float(row["PCSTRESN"])
        else:
            preconc[key] = None

    # Cmax (AVAL of the period's CMAX parameter) per (subject, visit).
    cmax = {}
    for row in pp:
        if row["PPTESTCD"] == "CMAX":
            cmax[(row["USUBJID"], row["VISITNUM"])] = float(row["PPSTRESN"])

    # Period-level values: PRECONC, PREPCMAX, ANL01FL per (subject, visit).
    period = {}
    for row in pp:
        key = (row["USUBJID"], row["VISITNUM"])
        if key in period:
            continue
        pre = preconc.get((row["USUBJID"], row["PPRFTDTC"]))
        c = cmax.get(key)
        if pre is None or c is None or c == 0:
            prepct = None
        else:
            prepct = 100.0 * pre / c
        if prepct is not None and prepct > 5:
            anl01fl = ""
        else:
            anl01fl = "Y"
        period[key] = (pre, prepct, anl01fl)

    out_rows = []
    for row in pp:
        pre, prepct, anl01fl = period[(row["USUBJID"], row["VISITNUM"])]
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "VISITNUM": row["VISITNUM"],
                "VISIT": row["VISIT"],
                "PARAMCD": row["PPTESTCD"],
                "PARAM": row["PPTEST"],
                "AVAL": fmt(float(row["PPSTRESN"])),
                "PRECONC": fmt(pre),
                "PREPCMAX": fmt(prepct),
                "ANL01FL": anl01fl,
            }
        )

    fieldnames = [
        "STUDYID",
        "USUBJID",
        "VISITNUM",
        "VISIT",
        "PARAMCD",
        "PARAM",
        "AVAL",
        "PRECONC",
        "PREPCMAX",
        "ANL01FL",
    ]
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "adpp.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
