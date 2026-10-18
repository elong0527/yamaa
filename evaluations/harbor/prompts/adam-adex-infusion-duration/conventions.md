Following CDISC ADaM standards, use the provided EX dataset to create an ADEX
dataset with one record per infusion.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EXSEQ, EXSTDTC, EXENDTC, EXDUR, PARAMCD, PARAM, AVAL, AVALU,
CALCDUR

PARAMCD is "INFDUR" and PARAM is "Infusion Duration (min)" on every record.
CALCDUR is the elapsed time in minutes from EXSTDTC to EXENDTC, with seconds
kept as a fraction of a minute and nothing added or rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adex.csv.
