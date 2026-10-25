Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per subject per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVAL, ANRLO, ANRHI

Each LB record, including a test not done, gives one output record.
PARAMCD is the lab test code (LBTESTCD) as given. AVAL is the standardized
numeric result (LBSTRESN), and has no value when the test was not done.
ANRLO and ANRHI are the lower and upper limits of the parameter's 95%
reference range, estimated from the study's own results:
- ANRLO is the mean of the parameter's results less z times their sample
  standard deviation, and ANRHI is the mean plus z times it.
- z is the standard normal distribution's 97.5th percentile at full
  precision (1.9599639845400536), not the rounded 1.96.
- The standard deviation has divisor one less than the number of results.
- A test not done takes no part in its parameter's mean, standard deviation
  or number of results, and each parameter's limits come from its own
  results alone.
- Both limits are the same on every record of the parameter, including a
  record without a result.
- Neither limit is rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
