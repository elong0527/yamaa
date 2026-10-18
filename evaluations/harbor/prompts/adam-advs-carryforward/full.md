Following CDISC ADaM standards, use the provided ADSL, PLAN, and VS datasets to
create an ADVS dataset with one record per planned measurement.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, VSSEQ, PARAMCD, ADT, AVAL, TRTSDT, HEIGHTBL

A collected record belongs to a planned measurement when it is for the
same study and subject, its test code is the planned parameter code, and
its collection date matches ADT. VSSEQ is the sequence number of that
collected record, and VSSEQ has no value when the planned measurement
was not collected. AVAL is the collected numeric result when the planned
measurement was collected. When the planned measurement was not
collected, AVAL is the most recent earlier collected value for the same
subject and parameter, and AVAL has no value before the first collected
value for that subject and parameter. TRTSDT is the subject's treatment
start date, repeated on every record of the subject. HEIGHTBL is the
latest height value collected on or before the subject's treatment
start date, repeated on every record of the subject. HEIGHTBL has no
value when no height was collected on or before the subject's treatment
start date.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
