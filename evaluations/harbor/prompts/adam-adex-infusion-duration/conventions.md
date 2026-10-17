Following CDISC ADaM standards, use the provided EX dataset to create an
ADEX dataset with one record per infusion.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EXSEQ, EXSTDTC, EXENDTC, EXDUR, PARAMCD, PARAM, AVAL, AVALU,
CALCDUR

PARAMCD is INFDUR and AVALU is min. A readable EXDUR wins over CALCDUR,
and no time is assumed for a date alone.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adex.csv.
