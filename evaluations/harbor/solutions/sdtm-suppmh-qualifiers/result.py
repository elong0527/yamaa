# Reference solution for the yamaa benchmark sdtm-suppmh-qualifiers (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SUPPMH dataset from MH_RAW.

One SUPPMH record per collected qualifier on each parent MH record.
The two qualifiers are family history (MHFAMHX) and confirmation by
medical records (MHCONF). A qualifier that was not collected on the
parent record (empty value) gives no SUPPMH record.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

QUALIFIERS = [
    ("MHFAMHX", "MHFAMHX", "Family History"),
    ("MHCONF", "MHCONF", "Confirmed by Medical Records"),
]

COLUMNS = [
    "STUDYID",
    "RDOMAIN",
    "USUBJID",
    "IDVAR",
    "IDVARVAL",
    "QNAM",
    "QLABEL",
    "QVAL",
    "QORIG",
    "QEVAL",
]


def main():
    with open(os.path.join(INPUT_DIR, "mh_raw.csv"), newline="") as f:
        parents = list(csv.DictReader(f))

    rows = []
    for parent in parents:
        for source_col, qnam, qlabel in QUALIFIERS:
            qval = (parent.get(source_col) or "").strip()
            if qval == "":
                continue
            rows.append(
                {
                    "STUDYID": parent["STUDYID"],
                    "RDOMAIN": "MH",
                    "USUBJID": parent["USUBJID"],
                    "IDVAR": "MHSEQ",
                    "IDVARVAL": str(parent["MHSEQ"]),
                    "QNAM": qnam,
                    "QLABEL": qlabel,
                    "QVAL": qval,
                    "QORIG": "Collected",
                    "QEVAL": "",
                }
            )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "suppmh.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
