Following CDISC ADaM standards, use the provided ADEG_RAW dataset to
create an ADEG dataset with one record per subject per parameter per
analysis period per nominal period day per nominal time point.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, COHORT, TRTA, PARAMCD, PARAM, APERIOD, APERDAY, ATPT,
ATPTN, AVAL, AVALU, ABLFL, BASE, CHG, BASETYPE, ACOMPFL, COMP, COMPBASE,
COMPCHG, CCOMPCHG, COMPTYPE

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
