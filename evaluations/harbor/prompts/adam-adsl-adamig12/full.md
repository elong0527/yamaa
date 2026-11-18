Following CDISC ADaM standards, use the provided ADSL_RAW dataset to create an
ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01A, AGE, HEIGHTBL, TRTSDT, TRTSDTM, TRTSTMF

Use ADaMIG 1.2. TRTSDTM is the first-exposure date and time in
ADSL_RAW.TRTSDTC, shown as "YYYY-MM-DDThh:mm:ss", with seconds "00" when
the source omits seconds. TRTSDT is the calendar date part of TRTSDTM.
TRTSDT and TRTSDTM have no value when the subject was never treated.
TRTSTMF is "S" when the source omits seconds, so they were imputed as "00",
because seconds are collected for other subjects; it has no value when the
source has seconds or the subject was never treated.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
