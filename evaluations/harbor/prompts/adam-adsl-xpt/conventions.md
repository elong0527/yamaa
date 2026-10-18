Following CDISC ADaM standards, use the provided ADSL_RAW dataset to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01A, AGE, HEIGHTBL, TRTSDT, TRTSDTM

TRTSDTM comes from TRTSDTC.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
