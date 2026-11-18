Following CDISC ADaM standards, use the provided ADSL and LB datasets to create
an ADLB dataset with one record per subject per parameter per collection date.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ADT, TRT01A, ADY, AVAL, ABLFL, BASE, CHG

Use ADaMIG 1.1.
PARAM is "Alanine Aminotransferase (U/L)".
ADY is relative to the subject's first treatment date in ADSL.
BASE is the AVAL of the record with ABLFL "Y", repeated on every record for the
same study, subject, and parameter.
BASE has no value when no record is flagged "Y" for that study, subject, and
parameter.
CHG is AVAL minus BASE on a record collected after the subject's first
treatment date. CHG has no value on the baseline record, on a record collected
on or before the first treatment date, and when BASE has no value.
BASE and CHG are not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
