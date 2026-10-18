# Reference solution for the yamaa benchmark sdtm-suppmh-linkage (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build SUPPMH from MH and MH_SUPP_RAW.

One SUPPMH record per collected qualifier on a condition. The parent MH
record is found by subject and condition term; its sequence number is
carried as text in IDVARVAL.
"""

import csv
import os

INPUT = "/app/input"
OUTPUT = "/app/output"

QUALIFIERS = [
    ("MHFAMHX", "MHFAMHX", "Family History"),
    ("MHCONF", "MHCONF", "Confirmed by Medical Records"),
]

COLUMNS = [
    "STUDYID", "RDOMAIN", "USUBJID", "IDVAR", "IDVARVAL",
    "QNAM", "QLABEL", "QVAL", "QORIG", "QEVAL",
]


def read_csv(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def main():
    mh = read_csv(os.path.join(INPUT, "mh.csv"))
    raw = read_csv(os.path.join(INPUT, "mh_supp_raw.csv"))

    mhseq_by_key = {}
    for row in mh:
        key = (row["STUDYID"], row["USUBJID"], row["MHTERM"])
        mhseq_by_key[key] = row["MHSEQ"]

    records = []
    for row in raw:
        key = (row["STUDYID"], row["USUBJID"], row["MHTERM"])
        mhseq = mhseq_by_key.get(key)
        if mhseq is None:
            continue
        for source, qnam, qlabel in QUALIFIERS:
            answer = (row.get(source) or "").strip()
            if not answer:
                continue
            records.append({
                "STUDYID": row["STUDYID"],
                "RDOMAIN": "MH",
                "USUBJID": row["USUBJID"],
                "IDVAR": "MHSEQ",
                "IDVARVAL": str(mhseq),
                "QNAM": qnam,
                "QLABEL": qlabel,
                "QVAL": answer,
                "QORIG": "Collected",
                "QEVAL": "",
            })

    os.makedirs(OUTPUT, exist_ok=True)
    with open(os.path.join(OUTPUT, "suppmh.csv"), "w", newline="",
              encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
