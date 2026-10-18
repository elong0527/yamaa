Following CDISC ADaM standards, use the provided EX dataset to create an ADEX
dataset with one record per exposure administration.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EXSEQ, EXSTDTM, EXDOSE, DOSREDFL

DOSREDFL is Y when the current dose (EXDOSE) is lower than the previous dose
for the same subject, and has no value otherwise. Within each subject,
administrations are ordered by treatment start date/time (EXSTDTM), with
administrations that share a start date/time ordered by sequence number
(EXSEQ), and the previous dose is the dose of the administration just before
in that order. DOSREDFL has no value for the subject's first administration
and whenever the current dose or the previous dose is zero or has no value.
The previous dose is never an earlier nonzero dose: a pause in dosing, where
the dose is zero, is not flagged, and neither is the administration after it.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adex.csv.
