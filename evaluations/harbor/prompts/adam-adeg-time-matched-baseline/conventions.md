Following CDISC ADaM standards, use the provided ADEG_RAW dataset to create an
ADEG dataset with one record per subject, parameter, analysis period,
nominal day within the period, and nominal time point.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, APERIOD, APERDAY, ATPT, ATPTN, AVAL, AVALU,
ABLFL, BASE, CHG, BASETYPE

ABLFL is "Y" or has no value. CHG is not rounded. BASETYPE is "Period "
followed by the period number, " Baseline Day " and ATPT.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
