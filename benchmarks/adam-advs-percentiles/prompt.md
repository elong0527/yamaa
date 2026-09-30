Following CDISC ADaM standards, use the provided ADVS_RAW and LMS
datasets to create an ADVS dataset with one record per subject per
visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL

Turn each collected body measurement into one growth-percentile
record; the collected records are not kept. PARAMCD is "BMIPCTL" for a
collected "BMI" measurement and "WGTPCTL" for a collected "WEIGHT"
measurement, with PARAM "BMI-for-Age Percentile" and
"Weight-for-Age Percentile" to match.

AVAL is the growth percentile for the collected result against the
reference coefficients matched on measurement code, sex, and age in
days, expressed as a percentage. A measurement with no matching
reference row, or with no collected result, leaves AVAL with no value.
A collected result exactly at the reference median gives 50.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
