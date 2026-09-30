Following CDISC ADaM standards, use the provided ADSL and QS datasets
to create an ADQS dataset with one record per subject per parameter
per analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISIT, AVISITN, AVAL, DTYPE, EFFFL

Use PARAMCD "ACTOT". The protocol schedules Weeks 8, 16, and 24 for
every subject in the efficacy population; baseline is not a scheduled
visit, so a missing baseline adds nothing.

Keep every collected score with DTYPE empty. A collected record with
an empty score is left out, so at a scheduled visit it counts as
missed.

Add one LOCF record for each missed scheduled visit of a subject in
the efficacy population. Its value is the score from the closest
earlier visit that has one for the same subject, and a collected zero
is a real score that carries forward; it has no value when the subject
has no earlier score. Subjects outside the efficacy population keep
their collected records and gain none.

EFFFL is the subject's efficacy population flag, repeated on every
record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
