Following CDISC ADaM standards, use the provided PP and EX datasets to
create an ADPP dataset with one record per subject per visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL, AVALU

Added records use these parameters:
- "CMAXD" with PARAM "Max Conc Norm by Dose";
- "AUCLSTD" with PARAM "AUC to Last Nonzero Conc Norm by Dose";
- "AUCIFOD" with PARAM "AUC Infinity Obs Norm by Dose".
PARAM holds no unit. AVAL is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
