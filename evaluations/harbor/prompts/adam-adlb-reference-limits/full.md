Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per subject per laboratory test.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVAL, ANRLO, ANRHI

- PARAMCD is the record's test code, and AVAL its standardized numeric
  result. AVAL has no value when the test was not done.
- ANRLO and ANRHI are the lower and upper limits of the parameter's 95%
  reference range, estimated from the study's own results: the mean of the
  parameter's AVAL values, less and plus the 0.975 quantile of the standard
  normal distribution times their sample standard deviation, with divisor
  one less than the number of values. The quantile is used at full
  precision, not rounded to 1.96.
- A test not done takes no part in its parameter's mean or standard
  deviation, and each parameter's limits come from its own results alone.
- ANRLO and ANRHI are the same on every record of the parameter, including
  a record whose AVAL has no value. They are not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
