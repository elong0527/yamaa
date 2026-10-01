Following CDISC ADaM standards, use the provided ADVS_RAW and LMS
datasets to create an ADVS dataset with one record per subject per
visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL

PARAMCD is "BMIPCTL" for a collected "BMI" measurement and "WGTPCTL" for
a collected "WEIGHT" measurement, with PARAM "BMI-for-Age Percentile"
and "Weight-for-Age Percentile" to match.

AVAL is the growth percentile, not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
