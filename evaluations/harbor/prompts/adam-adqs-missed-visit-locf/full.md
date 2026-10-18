Following CDISC ADaM standards, use the provided ADSL and QS datasets to create
an ADQS dataset with one record per subject per parameter per analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISIT, AVISITN, AVAL, DTYPE, EFFFL

Each collected total score in QS is a record of the output, and a
collected record with an empty score is left out. The scheduled visits
are "Week 8" (AVISITN 8), "Week 16" (AVISITN 16), and "Week 24"
(AVISITN 24). Baseline is not a scheduled visit, so a missing baseline
adds no record. A scheduled visit is missed when the subject has no
collected score there. For each scheduled visit missed by a subject
whose EFFFL in ADSL is "Y", the output adds a record with PARAMCD
"ACTOT", that visit's AVISIT and AVISITN, and DTYPE "LOCF". AVAL is
the collected score on a collected record. On an added record, AVAL is
the subject's score from the closest earlier visit that has one, and
AVAL has no value when the subject has no earlier score. A collected
score of zero counts as a score. DTYPE has no value on a collected
record. EFFFL is the subject's efficacy population flag from ADSL,
repeated on every record. A subject outside the efficacy population
keeps the collected records and gains no added record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
