# Reference solution for the yamaa benchmark sdtm-lb-findings (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM LB dataset from the ODM extract.

One record per collected lab result in the IG.LB item groups: a test
with no collected entry gives no record, while an entry of "NOT DONE"
gives a record carrying LBSTAT.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

TESTS = {
    "IT.LB.CALCIUM": ("CA", "Calcium"),
    "IT.LB.CREAT": ("CREAT", "Creatinine"),
}
DATE_ITEM = "IT.LB.LBDTC"
UNIT = "mg/dL"

COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "LBSEQ",
    "LBTESTCD",
    "LBTEST",
    "LBORRES",
    "LBORRESU",
    "LBSTRESN",
    "LBSTRESU",
    "LBSTAT",
    "LBDTC",
]


def numeric_text(value):
    """Numeric form of a collected entry, or "" when it is not a number."""
    try:
        number = float(value)
    except ValueError:
        return ""
    if number.is_integer():
        return str(int(number))
    return str(number)


def main():
    with open(os.path.join(INPUT_DIR, "odm.csv"), newline="") as handle:
        rows = list(csv.DictReader(handle))

    # Group the LB items by subject and visit group.
    groups = {}
    for row in rows:
        if row["ItemGroupOID"] != "IG.LB":
            continue
        key = (
            row["StudyOID"],
            row["SubjectKey"],
            row["StudyEventOID"],
            row["StudyEventRepeatKey"],
            row["ItemGroupOID"],
            row["ItemGroupRepeatKey"],
        )
        groups.setdefault(key, {})[row["ItemOID"]] = row["Value"]

    records = []
    for (study_id, subject, *_), items in groups.items():
        lbdtc = items.get(DATE_ITEM, "")
        for item_oid, (testcd, test) in TESTS.items():
            entry = items.get(item_oid)
            if entry is None or entry == "":
                continue  # no collected entry: no record
            record = {
                "DOMAIN": "LB",
                "STUDYID": study_id,
                "USUBJID": subject,
                "LBTESTCD": testcd,
                "LBTEST": test,
                "LBDTC": lbdtc,
            }
            if entry == "NOT DONE":
                record.update(
                    LBORRES="",
                    LBORRESU="",
                    LBSTRESN="",
                    LBSTRESU="",
                    LBSTAT="NOT DONE",
                )
            else:
                record.update(
                    LBORRES=entry,
                    LBORRESU=UNIT,
                    LBSTRESN=numeric_text(entry),
                    LBSTRESU=UNIT,
                    LBSTAT="",
                )
            records.append(record)

    # LBSEQ numbers each subject's records from 1 by collection date,
    # then by test code.
    by_subject = {}
    for record in records:
        by_subject.setdefault(record["USUBJID"], []).append(record)
    ordered = []
    for subject in sorted(by_subject):
        subject_records = sorted(
            by_subject[subject],
            key=lambda r: (r["LBDTC"], r["LBTESTCD"]),
        )
        for index, record in enumerate(subject_records, start=1):
            record["LBSEQ"] = str(index)
            ordered.append(record)

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "lb.csv"), "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(ordered)


if __name__ == "__main__":
    main()
