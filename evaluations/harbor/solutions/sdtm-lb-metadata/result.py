# Reference solution for the yamaa benchmark sdtm-lb-metadata (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM LB dataset from the ODM extract.

One record per collected glucose or creatinine result. Reads
/app/input/odm.csv and writes /app/output/lb.csv.
"""

import csv
from collections import OrderedDict

INPUT_CSV = "/app/input/odm.csv"
OUTPUT_CSV = "/app/output/lb.csv"

TESTS = OrderedDict(
    [
        ("IT.LB.GLUC", ("GLUC", "Glucose")),
        ("IT.LB.CREAT", ("CREAT", "Creatinine")),
    ]
)
DATE_ITEM = "IT.LB.LBDTC"

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
    "LBDTC",
]


def numeric_form(text):
    """Return the numeric rendering of text, or '' when not a number."""
    try:
        value = float(text)
    except (TypeError, ValueError):
        return ""
    if value == int(value):
        return str(int(value))
    return repr(value)


def main():
    with open(INPUT_CSV, newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))

    # Group items by subject and visit group (one collection event).
    groups = OrderedDict()
    for row in rows:
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
    for (study_oid, subject_key, *_), items in groups.items():
        lbdtc = items.get(DATE_ITEM, "")
        for item_oid, (testcd, test) in TESTS.items():
            if item_oid not in items:
                continue
            entry = items[item_oid]
            records.append(
                {
                    "DOMAIN": "LB",
                    "STUDYID": study_oid,
                    "USUBJID": subject_key,
                    "LBTESTCD": testcd,
                    "LBTEST": test,
                    "LBORRES": entry,
                    "LBORRESU": "mg/dL",
                    "LBSTRESN": numeric_form(entry),
                    "LBSTRESU": "mg/dL",
                    "LBDTC": lbdtc,
                }
            )

    # LBSEQ numbers each subject's records from 1 by collection date,
    # then by test code.
    by_subject = {}
    for record in records:
        by_subject.setdefault(record["USUBJID"], []).append(record)
    ordered = []
    for subject in by_subject:
        subject_records = sorted(
            by_subject[subject],
            key=lambda r: (r["LBDTC"], r["LBTESTCD"]),
        )
        for index, record in enumerate(subject_records, start=1):
            record["LBSEQ"] = index
        ordered.extend(subject_records)

    with open(OUTPUT_CSV, "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        for record in ordered:
            writer.writerow({column: record[column] for column in COLUMNS})


if __name__ == "__main__":
    main()
