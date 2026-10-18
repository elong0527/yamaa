# Reference solution for the yamaa benchmark sdtm-relrec-dataset-level (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = ["STUDYID", "USUBJID", "RDOMAIN", "IDVAR", "IDVARVAL",
           "RELTYPE", "RELID"]


def read_rows(name):
    with open(os.path.join(INPUT_DIR, name), newline="") as f:
        return list(csv.DictReader(f))


def clean(value):
    return "" if value is None else value.strip()


def main():
    ae = read_rows("ae.csv")
    cm = read_rows("cm.csv")
    tr = read_rows("tr.csv")
    tu = read_rows("tu.csv")

    rows = []

    # Dataset-level rows: the TU and TR datasets relate as a whole.
    # One tumor identification (TU) relates to many tumor results (TR).
    studyid = ""
    if tu:
        studyid = tu[0]["STUDYID"]
    elif tr:
        studyid = tr[0]["STUDYID"]
    rows.append({
        "STUDYID": studyid,
        "USUBJID": "",
        "RDOMAIN": "TU",
        "IDVAR": "TULNKID",
        "IDVARVAL": "",
        "RELTYPE": "ONE",
        "RELID": "1",
    })
    rows.append({
        "STUDYID": studyid,
        "USUBJID": "",
        "RDOMAIN": "TR",
        "IDVAR": "TRLNKID",
        "IDVARVAL": "",
        "RELTYPE": "MANY",
        "RELID": "1",
    })

    # Record-level rows: one per AE record carrying a link identifier.
    for rec in ae:
        lnkid = clean(rec.get("AELNKID"))
        if not lnkid:
            continue
        rows.append({
            "STUDYID": rec["STUDYID"],
            "USUBJID": rec["USUBJID"],
            "RDOMAIN": "AE",
            "IDVAR": "AESEQ",
            "IDVARVAL": clean(rec["AESEQ"]),
            "RELTYPE": "",
            "RELID": lnkid,
        })

    # Record-level rows: one per CM record carrying a link identifier.
    for rec in cm:
        lnkid = clean(rec.get("CMLNKID"))
        if not lnkid:
            continue
        rows.append({
            "STUDYID": rec["STUDYID"],
            "USUBJID": rec["USUBJID"],
            "RDOMAIN": "CM",
            "IDVAR": "CMSEQ",
            "IDVARVAL": clean(rec["CMSEQ"]),
            "RELTYPE": "",
            "RELID": lnkid,
        })

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "relrec.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
