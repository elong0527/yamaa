Following CDISC ADaM standards, use the provided AE and EX datasets to
create an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEDECOD, ASTDT, ASTDTM, DOSE24FL, DOSENUM

DOSE24FL is Y or has no value. Times compare to the second, and a time
collected without seconds counts as second zero.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
