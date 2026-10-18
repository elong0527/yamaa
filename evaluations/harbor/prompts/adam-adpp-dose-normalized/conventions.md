Following CDISC ADaM standards, use the provided EX and PP datasets to create
an ADPP dataset with one record per subject, parameter, and visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL, AVALU

PARAMCD is CMAXD for a CMAX record, AUCLSTD for an AUCLST record, and
AUCIFOD for an AUCIFO record. PARAM is "Max Conc Norm by Dose" for CMAXD,
"AUC to Last Nonzero Conc Norm by Dose" for AUCLSTD, and "AUC Infinity Obs
Norm by Dose" for AUCIFOD. AVAL on an added record is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
