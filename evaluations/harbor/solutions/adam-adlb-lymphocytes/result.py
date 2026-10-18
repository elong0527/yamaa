# Reference solution for the yamaa benchmark adam-adlb-lymphocytes (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADLB from LB: collected records plus calculated absolute
differential counts (WBC count times the matching differential fraction)
where the absolute parameter is not already present."""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

# differential fraction test code -> (absolute parameter code, PARAM name)
DIFFERENTIALS = [
    ("LYMLE", "LYMPH", "Lymphocytes Abs (10^9/L)"),
    ("NEUTLE", "NEUT", "Neutrophils Abs (10^9/L)"),
    ("MONOLE", "MONO", "Monocytes Abs (10^9/L)"),
    ("EOSLE", "EOS", "Eosinophils Abs (10^9/L)"),
    ("BASOLE", "BASO", "Basophils Abs (10^9/L)"),
]


def format_number(value):
    text = f"{value:.10f}".rstrip("0").rstrip(".")
    return text if text not in ("", "-0") else "0"


def main():
    with open(os.path.join(INPUT_DIR, "lb.csv"), newline="") as handle:
        lb_rows = list(csv.DictReader(handle))

    collected = []
    wbc = {}        # (subject, visit) -> WBC count (float)
    fractions = {}  # (subject, visit, fraction code) -> fraction (float)
    existing = set()  # (subject, visit, paramcd) among collected records
    group_order = []  # (subject, visit) in first-appearance order

    for row in lb_rows:
        subject = row["USUBJID"]
        visit = row["VISIT"]
        testcd = row["LBTESTCD"]
        key = (subject, visit)
        if key not in group_order:
            group_order.append(key)
        value_text = row["LBSTRESN"].strip()
        value = float(value_text) if value_text else None

        collected.append(
            {
                "USUBJID": subject,
                "PARAMCD": testcd,
                "AVAL": value_text,
                "PARAM": row["LBTEST"],
                "VISIT": visit,
                "DTYPE": "",
            }
        )
        existing.add((subject, visit, testcd))
        if value is None:
            continue
        if testcd == "WBC":
            wbc.setdefault(key, value)
        elif any(testcd == frac for frac, _, _ in DIFFERENTIALS):
            fractions.setdefault((subject, visit, testcd), value)

    added = []
    for subject, visit in group_order:
        key = (subject, visit)
        if key not in wbc:
            continue
        for frac_code, abs_code, abs_name in DIFFERENTIALS:
            frac_key = (subject, visit, frac_code)
            if frac_key not in fractions:
                continue
            if (subject, visit, abs_code) in existing:
                continue
            added.append(
                {
                    "USUBJID": subject,
                    "PARAMCD": abs_code,
                    "AVAL": format_number(wbc[key] * fractions[frac_key]),
                    "PARAM": abs_name,
                    "VISIT": visit,
                    "DTYPE": "CALCULATION",
                }
            )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    fieldnames = ["USUBJID", "PARAMCD", "AVAL", "PARAM", "VISIT", "DTYPE"]
    with open(
        os.path.join(OUTPUT_DIR, "adlb.csv"), "w", newline=""
    ) as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(collected + added)


if __name__ == "__main__":
    main()
