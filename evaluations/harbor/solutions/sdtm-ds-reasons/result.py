# Reference solution for the yamaa benchmark sdtm-ds-reasons (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM DS dataset from the ODM extract.

One record per subject for each of the two milestones: end of treatment
(StudyEventOID EOT, DSSEQ 1) and end of study (EOS, DSSEQ 2). The
completion flag, reason text, controlled code, and start date of a
milestone are collected as separate ODM items; the record for the
milestone carries the values of that milestone's items.
"""

import pandas as pd

INPUT_ODM = "/app/input/odm.csv"
OUTPUT_DS = "/app/output/ds.csv"

ITEM_COMPLETION = "IT.DS.COMP"
ITEM_REASON = "IT.DS.REASON"
ITEM_REASON_CODE = "IT.DS.REASONCD"
ITEM_START_DATE = "IT.DS.DTC"

# (StudyEventOID, DSSEQ, DSSCAT) in output order.
MILESTONES = [
    ("EOT", 1, "STUDY TREATMENT"),
    ("EOS", 2, "STUDY"),
]

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
    odm = pd.read_csv(INPUT_ODM, dtype=str, keep_default_na=False)

    records = []
    for subject_key in sorted(odm["SubjectKey"].unique()):
        subject_rows = odm[odm["SubjectKey"] == subject_key]
        study_id = subject_rows["StudyOID"].iloc[0]
        for event_oid, dsseq, dsscat in MILESTONES:
            milestone_rows = subject_rows[
                subject_rows["StudyEventOID"] == event_oid
            ]
            items = dict(
                zip(milestone_rows["ItemOID"], milestone_rows["Value"])
            )
            completion = items.get(ITEM_COMPLETION, "")
            if completion == "COMPLETED":
                dsterm = "COMPLETED"
                dsdecod = "COMPLETED"
            else:
                dsterm = items.get(ITEM_REASON, "")
                dsdecod = items.get(ITEM_REASON_CODE, "")
            records.append(
                {
                    "DOMAIN": "DS",
                    "STUDYID": study_id,
                    "USUBJID": subject_key,
                    "DSSEQ": dsseq,
                    "DSCAT": "DISPOSITION EVENT",
                    "DSSCAT": dsscat,
                    "DSTERM": dsterm,
                    "DSDECOD": dsdecod,
                    "DSSTDTC": items.get(ITEM_START_DATE, ""),
                }
            )

    ds = pd.DataFrame.from_records(records, columns=COLUMNS)
    ds.to_csv(OUTPUT_DS, index=False)


if __name__ == "__main__":
    main()
