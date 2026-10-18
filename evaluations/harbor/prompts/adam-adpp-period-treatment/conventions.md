Following CDISC ADaM standards, use the provided EX and PP datasets to create
an ADPP dataset with one record per subject, analyte, parameter and
reference date and time.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARCAT1, PARAMCD, PARAM, PPRFTDTC, APERIOD, TRTA, DOSEA,
DOSEU, AVAL, AVALU

APERIOD is 1 for "TREATMENT 1" and 2 for "TREATMENT 2".

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
