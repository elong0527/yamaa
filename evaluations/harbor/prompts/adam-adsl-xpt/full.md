Following CDISC ADaM standards, use the provided ADSL_RAW dataset to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01A, AGE, HEIGHTBL, TRTSDT, TRTSDTM

TRT01A and AGE are as collected. HEIGHTBL is the collected baseline height,
with no value when not measured. TRTSDTM is the date and time of first
exposure collected in TRTSDTC, and TRTSDT is its date part; both have no
value when the subject was never treated.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
