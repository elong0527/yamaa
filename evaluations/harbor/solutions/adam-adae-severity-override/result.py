# Reference solution for the yamaa benchmark adam-adae-severity-override (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADAE from AE with approved severity corrections.

Reads /app/input/ae.csv and writes /app/output/adae.csv with columns
STUDYID, USUBJID, AESEQ, ASEV, ASEVN.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

# Approved severity corrections, keyed by (USUBJID, AESEQ).
CORRECTIONS = {
    ("CATH-01-001", 2): "SEVERE",
    ("CATH-01-003", 1): "SEVERE",
}

RANKS = {
    "MILD": 1,
    "MODERATE": 2,
    "SEVERE": 3,
    "LIFE-THREATENING": 4,
}


def main():
    with open(os.path.join(INPUT_DIR, "ae.csv"), newline="") as f:
        rows = list(csv.DictReader(f))

    out_rows = []
    for row in rows:
        aeseq = int(row["AESEQ"])
        key = (row["USUBJID"], aeseq)
        reported = (row["AESEV"] or "").strip()
        if key in CORRECTIONS:
            asev = CORRECTIONS[key]
        elif reported:
            asev = reported.upper()
        else:
            asev = ""
        asevn = RANKS[asev] if asev else ""
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "AESEQ": row["AESEQ"],
                "ASEV": asev,
                "ASEVN": asevn,
            }
        )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "adae.csv"), "w", newline="") as f:
        writer = csv.DictWriter(
            f, fieldnames=["STUDYID", "USUBJID", "AESEQ", "ASEV", "ASEVN"]
        )
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
