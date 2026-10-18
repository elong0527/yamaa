Following CDISC ADaM standards, use the provided ADEG_RAW dataset to create an
ADEG dataset with one record per subject, parameter, analysis period, nominal
day and nominal time point.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, COHORT, TRTA, PARAMCD, PARAM, APERIOD, APERDAY, ATPT, ATPTN,
AVAL, AVALU, ABLFL, BASE, CHG, BASETYPE, ACOMPFL, COMP, COMPBASE, COMPCHG,
CCOMPCHG, COMPTYPE

ACOMPFL is "Y" or has no value. COMPTYPE is "Placebo group at " followed by
the record's ATPT, or has no value. None of the comparator values is rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
