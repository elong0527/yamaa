Following CDISC ADaM standards, use the provided EX dataset to create an ADPPK
dataset with one record per EX record with a dose (EXDOSE) above zero.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EXSEQ, ADTM, ATMF, EVID, AMT, II, ADDL

- ADTM is in the form YYYY-MM-DDThh:mm:ss.
- ATMF is "H" or has no value.
- EVID is 1.
- II is 24, 12, or 0.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adppk.csv.
