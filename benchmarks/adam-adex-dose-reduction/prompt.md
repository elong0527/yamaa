Following CDISC ADaM standards, use the provided EX dataset to create
an ADEX dataset with one record per exposure administration.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EXSEQ, EXSTDTM, EXDOSE, DOSREDFL

DOSREDFL is Y when the current dose is lower than the previous dose for
the same subject, and has no value otherwise: for the first record, and
whenever the current or the previous dose is zero or missing. Compare
in chronological treatment-start order within each subject, breaking
timestamp ties by sequence number. The previous dose is always the
administration just before, never an earlier nonzero dose, so a pause
in dosing is not flagged and neither is the administration after it.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adex.csv.
