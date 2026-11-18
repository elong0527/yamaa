Following CDISC ADaM standards, use the provided ADSL_RAW dataset to create an
ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01A, AGE, HEIGHTBL, TRTSDT, TRTSDTM, TRTSTMF

TRTSDTM is the first exposure date and time from ADSL_RAW.TRTSDTC,
expressed as YYYY-MM-DDThh:mm:ss with seconds 00 when the collected value
ends at the minute. TRTSDT is the calendar date of TRTSDTM. TRTSDT and
TRTSDTM have no value for a subject who was never treated. TRTSTMF is "S"
when the collected time ends at the minute, so its seconds were imputed as
00, because seconds are collected for other subjects; it has no value when
the seconds were collected or the subject was never treated. HEIGHTBL has no
value when baseline height was not measured.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
