Following CDISC ADaM standards, use the provided ADEG_RAW dataset to
create an ADEG dataset with one record per subject per parameter per
analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, AVALU

New records use these parameters:
- "QTCBR" with PARAM "QTcB - Bazett's Correction Formula Rederived (ms)";
- "QTCFR" with PARAM
  "QTcF - Fridericia's Correction Formula Rederived (ms)";
- "RRR" with PARAM "RR Duration Rederived (ms)".
New records use AVALU "ms", and their AVAL is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
