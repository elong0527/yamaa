Following CDISC ADaM standards, use the provided ADEG dataset to create an ADEG
dataset with one record per subject, parameter, and analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, AVALU

PARAMCD QTCBR has PARAM "QTcB - Bazett's Correction Formula Rederived
(ms)", PARAMCD QTCFR has PARAM "QTcF - Fridericia's Correction Formula
Rederived (ms)", and PARAMCD RRR has PARAM "RR Duration Rederived
(ms)". AVAL on the new records is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
