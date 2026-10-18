# Reference solution for the yamaa benchmark sdtm-ds-reasons (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM DS dataset from the ODM extract.

One record per subject for each of the two milestones: end of
treatment (EOT) and end of study (EOS). Each milestone's completion
flag, reason text, controlled code, and start date are collected as
separate items in the ODM rows; the record for a milestone carries
the values of that milestone's items, in whatever order the input
rows hold them.
"""

import csv
from pathlib import Path

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")

# (StudyEventOID, DSSEQ, DSSCAT) for each milestone, in DSSEQ order.
MILESTONES = [
    ("EOT", 1, "STUDY TREATMENT"),
    ("EOS", 2, "STUDY"),
]

ITEM_COMPLETION = "IT.DS.COMP"
ITEM_REASON = "IT.DS.REASON"
ITEM_REASON_CODE = "IT.DS.REASONCD"
ITEM_DATE = "IT.DS.DTC"

COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "DSSEQ",
    "DSCAT",
    "DSSCAT",
    "DSTERM",
    "DSDECOD",
    "DSSTDTC",
]


def main() -> None:
    with open(INPUT / "odm.csv", newline="") as handle:
        odm_rows = list(csv.DictReader(handle))

    # Collect each milestone's items per subject:
    # items[(subject, event)][item oid] = value.
    items: dict[tuple[str, str], dict[str, str]] = {}
    study_by_subject: dict[str, str] = {}
    for row in odm_rows:
        subject = row["SubjectKey"]
        study_by_subject.setdefault(subject, row["StudyOID"])
        key = (subject, row["StudyEventOID"])
        items.setdefault(key, {})[row["ItemOID"]] = row["Value"]

    records = []
    for subject in sorted(study_by_subject):
        for event_oid, dsseq, dsscat in MILESTONES:
            collected = items.get((subject, event_oid), {})
            if collected.get(ITEM_COMPLETION) == "COMPLETED":
                dsterm = "COMPLETED"
                dsdecod = "COMPLETED"
            else:
                dsterm = collected.get(ITEM_REASON, "")
                dsdecod = collected.get(ITEM_REASON_CODE, "")
            records.append(
                {
                    "DOMAIN": "DS",
                    "STUDYID": study_by_subject[subject],
                    "USUBJID": subject,
                    "DSSEQ": str(dsseq),
                    "DSCAT": "DISPOSITION EVENT",
                    "DSSCAT": dsscat,
                    "DSTERM": dsterm,
                    "DSDECOD": dsdecod,
                    "DSSTDTC": collected.get(ITEM_DATE, ""),
                }
            )

    OUTPUT.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT / "ds.csv", "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
