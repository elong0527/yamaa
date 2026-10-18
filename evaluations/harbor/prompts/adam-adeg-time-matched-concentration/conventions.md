Following CDISC ADaM standards, use the provided ADEG_RAW and ADPC datasets to
create an ADEG dataset with one record per subject, parameter, analysis
period, nominal day within the period, and nominal time point.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRTA, PARAMCD, PARAM, APERIOD, APERDAY, ATPT, ATPTN, AVAL,
AVALU, ABLFL, BASE, CHG, BASETYPE, ECGPCFL, PKCONC, CQTANLFL

ECGPCFL is Y or has no value. PKCONC is as recorded and not rounded.
CQTANLFL is Y or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
