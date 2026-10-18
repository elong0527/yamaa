# Reference solution for the yamaa benchmark sdtm-ds-multi-form-disposition (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM DS dataset from the collected ODM extract.

One record per subject per collected form (study event). Reads
/app/input/odm.csv (long EAV form: one row per collected item) and
writes /app/output/ds.csv.
"""

import csv
import os
from collections import OrderedDict

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

# StudyEventOID -> (DSCAT, DSSCAT, controlled term for a milestone)
EVENT_INFO = {
    "CONSENT": ("PROTOCOL MILESTONE", "INFORMED CONSENT",
                "INFORMED CONSENT OBTAINED"),
    "RAND": ("PROTOCOL MILESTONE", "RANDOMIZATION", "RANDOMIZED"),
    "EOT": ("DISPOSITION EVENT", "END OF TREATMENT", None),
    "EOS": ("DISPOSITION EVENT", "END OF STUDY", None),
}

COLUMNS = ["DOMAIN", "STUDYID", "USUBJID", "DSSEQ", "DSTERM", "DSDECOD",
           "DSCAT", "DSSCAT", "DSSTDTC"]


def main():
    with open(os.path.join(INPUT_DIR, "odm.csv"), newline="") as f:
        rows = list(csv.DictReader(f))

    # Group the EAV rows into forms: one form per subject, study event
    # and repeat keys. A study event with no form gives no record.
    forms = OrderedDict()
    for row in rows:
        key = (row["StudyOID"], row["SubjectKey"], row["StudyEventOID"],
               row["StudyEventRepeatKey"], row["ItemGroupOID"],
               row["ItemGroupRepeatKey"])
        forms.setdefault(key, {})[row["ItemOID"]] = row["Value"] or ""

    records = []
    for (study_oid, subject_key, event_oid, *_), items in forms.items():
        dscat, dsscat, milestone_term = EVENT_INFO[event_oid]
        completion = items.get("IT.DS.COMP", "")
        if milestone_term is not None:
            dsterm = dsdecod = milestone_term
        elif completion == "COMPLETED":
            dsterm = dsdecod = "COMPLETED"
        elif event_oid == "EOS" and completion == "SCREEN FAILURE":
            dsterm = dsdecod = "SCREEN FAILURE"
        else:
            dsterm = items.get("IT.DS.REASON", "")
            dsdecod = items.get("IT.DS.REASONCD", "")
        records.append({
            "DOMAIN": "DS",
            "STUDYID": study_oid,
            "USUBJID": "{}-{}".format(study_oid, subject_key),
            "DSTERM": dsterm,
            "DSDECOD": dsdecod,
            "DSCAT": dscat,
            "DSSCAT": dsscat,
            "DSSTDTC": items.get("IT.DS.DTC", ""),
        })

    # DSSEQ numbers each subject's records from 1 in the order they
    # happened, by DSSTDTC and then by DSTERM.
    by_subject = OrderedDict()
    for record in records:
        by_subject.setdefault(record["USUBJID"], []).append(record)

    out_rows = []
    for subject_records in by_subject.values():
        subject_records.sort(key=lambda r: (r["DSSTDTC"], r["DSTERM"]))
        for seq, record in enumerate(subject_records, start=1):
            record["DSSEQ"] = seq
            out_rows.append(record)

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "ds.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        for record in out_rows:
            writer.writerow({col: record[col] for col in COLUMNS})


if __name__ == "__main__":
    main()
