# Reference solution for the yamaa benchmark sdtm-ae-seriousness (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM AE dataset from AE_RAW.

One record per adverse event. AESER is "Y" when any seriousness
criterion (AESDTH, AESLIFE, AESHOSP, AESDISAB, AESCONG, AESMIE) is "Y",
and "N" when no criterion is "Y", including when every criterion is
blank. A criterion flag with no value stays empty.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

CRITERIA = ["AESDTH", "AESLIFE", "AESHOSP", "AESDISAB", "AESCONG", "AESMIE"]

OUTPUT_COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "AESEQ",
    "AETERM",
    "AESER",
    "AESDTH",
    "AESLIFE",
    "AESHOSP",
    "AESDISAB",
    "AESCONG",
    "AESMIE",
]


def main():
    with open(os.path.join(INPUT_DIR, "ae_raw.csv"), newline="") as f:
        raw_rows = list(csv.DictReader(f))

    out_rows = []
    for row in raw_rows:
        serious = any((row.get(col) or "") == "Y" for col in CRITERIA)
        out = {
            "DOMAIN": "AE",
            "STUDYID": row.get("STUDYID", ""),
            "USUBJID": row.get("USUBJID", ""),
            "AESEQ": row.get("AESEQ", ""),
            "AETERM": row.get("AETERM", ""),
            "AESER": "Y" if serious else "N",
        }
        for col in CRITERIA:
            out[col] = row.get(col) or ""
        out_rows.append(out)

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "ae.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=OUTPUT_COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
