Following CDISC ADaM standards, use the provided ADVS_RAW and LMS_REF datasets
to create an ADVS dataset with one record per subject per visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL

PARAMCD is "BMIPCTL" for a collected BMI measurement and "WGTPCTL" for
a collected WEIGHT measurement. PARAM is "BMI-for-Age Percentile" when
PARAMCD is "BMIPCTL" and "Weight-for-Age Percentile" when PARAMCD is
"WGTPCTL". AVAL is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
