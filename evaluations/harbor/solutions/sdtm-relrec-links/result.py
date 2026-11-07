# Reference solution for the yamaa benchmark sdtm-relrec-links (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build a RELREC dataset from the AE numbers entered on CM records.

Each event number in CMAENO1/CMAENO2 names the same subject's AE record
with that AESPID. Every such medication-event pair gives two RELREC rows,
one for the CM record and one for the AE record, sharing a RELID that
numbers the subject's pairs by AESEQ and then CMSEQ.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = ["STUDYID", "USUBJID", "RDOMAIN", "IDVAR", "IDVARVAL",
           "RELTYPE", "RELID"]


def read_rows(name):
    with open(os.path.join(INPUT_DIR, name), newline="",
              encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def clean(value):
    return "" if value is None else value.strip()


def main():
    ae = read_rows("ae.csv")
    cm = read_rows("cm.csv")

    aeseq_by_number = {}
    for record in ae:
        key = (record["STUDYID"], record["USUBJID"], clean(record["AESPID"]))
        aeseq_by_number[key] = int(record["AESEQ"])

    pairs = {}
    for record in cm:
        subject = (record["STUDYID"], record["USUBJID"])
        cmseq = int(record["CMSEQ"])
        for var in ("CMAENO1", "CMAENO2"):
            number = clean(record.get(var))
            if not number:
                continue
            aeseq = aeseq_by_number[subject + (number,)]
            pairs.setdefault(subject, []).append((aeseq, cmseq))

    rows = []
    for subject in sorted(pairs):
        studyid, usubjid = subject
        for relid, (aeseq, cmseq) in enumerate(sorted(pairs[subject]), 1):
            for rdomain, idvar, seq in (("AE", "AESEQ", aeseq),
                                        ("CM", "CMSEQ", cmseq)):
                rows.append({
                    "STUDYID": studyid,
                    "USUBJID": usubjid,
                    "RDOMAIN": rdomain,
                    "IDVAR": idvar,
                    "IDVARVAL": str(seq),
                    "RELTYPE": "",
                    "RELID": str(relid),
                })

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    out_path = os.path.join(OUTPUT_DIR, "relrec.csv")
    with open(out_path, "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
