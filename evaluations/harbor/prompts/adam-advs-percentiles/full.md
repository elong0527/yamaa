Following CDISC ADaM standards, use the provided ADVS_RAW and LMS_REF datasets
to create an ADVS dataset with one record per subject per visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL

PARAMCD is "BMIPCTL" for a collected BMI measurement and "WGTPCTL" for
a collected WEIGHT measurement. PARAM is "BMI-for-Age Percentile" when
PARAMCD is "BMIPCTL" and "Weight-for-Age Percentile" when PARAMCD is
"WGTPCTL". AVAL is the growth percentile of the collected result: 100
times the standard normal cumulative probability of its LMS z-score,
where the z-score is the collected result divided by M, raised to the
power L, minus 1, all divided by L times S. The L, M, and S are the
reference coefficients matched on the measurement code, sex, and age in
days. AVAL has no value when no reference row matches the measurement
code, sex, and age in days, or when the collected result has no value.
A collected result equal to M has a percentile of exactly 50. AVAL is
not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
