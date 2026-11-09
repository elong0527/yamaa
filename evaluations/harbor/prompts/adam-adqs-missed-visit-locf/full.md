Following CDISC ADaM standards, use the provided ADSL and QS datasets to create
an ADQS dataset with one record per collected QS record and per
carried-forward visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISIT, AVISITN, AVAL, DTYPE, QSSEQ, EFFFL

Each QS record is a record of the output, including a record whose
QSSTRESN is empty. On a collected record, PARAMCD is QSTESTCD, AVISIT is
VISIT, AVISITN is VISITNUM, AVAL is QSSTRESN, and QSSEQ is the record's
QSSEQ. The scheduled visits are "Week 8" (AVISITN 8), "Week 16" (AVISITN
16), and "Week 24" (AVISITN 24). A scheduled visit is missed when the
subject has no QS record with a QSSTRESN value there. For each scheduled
visit missed by a subject whose EFFFL in ADSL is "Y", the output adds a
record with PARAMCD "ACTOT", that visit's AVISIT and AVISITN, and DTYPE
"LOCF", when the subject has a QSSTRESN value at an earlier visit after
baseline (VISITNUM greater than 0). Its AVAL is the QSSTRESN of the
closest such earlier visit, and its QSSEQ is the QSSEQ of that record. A
baseline score is never carried forward, so a missed visit with no
earlier post-baseline score adds no record. A score of zero counts as a
score. DTYPE has no value on a collected record. EFFFL is the subject's
efficacy population flag from ADSL, repeated on every record. A subject
outside the efficacy population keeps the collected records and gains no
added record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
