Following CDISC ADaM standards, use the provided ADSL_RAW dataset to create an
ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01A, AGE, HEIGHTBL, TRTSDT, TRTSDTM

Use ADaMIG 1.2. TRTSDTM is shown as "YYYY-MM-DDThh:mm:ss".
TRTSDT and TRTSDTM can have no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
