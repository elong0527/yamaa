Following CDISC ADaM standards, use the provided ADSL and LB datasets to create
an ADLB dataset with one record per subject per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, SEX, PARAMCD, AVAL, ANRLO, ANRHI, AVALVAR

Each LB record, including a test not done, gives one output record.
SEX is the subject's sex from ADSL. PARAMCD is the lab test code (LBTESTCD)
as given. AVAL is the standardized numeric result (LBSTRESN), and has no
value when the test was not done.
ANRLO and ANRHI are the lower and upper limits of the 95% reference range of
the record's parameter and sex, estimated from the study's own results
without assuming a distribution, and AVALVAR is the variance of those
results:
- ANRLO is the 2.5th percentile and ANRHI the 97.5th percentile of the
  results of the parameter and sex, by Hyndman and Fan's type 7 definition:
  the quantile at probability p is the value at position (n - 1) * p + 1 of
  the n results in ascending order, interpolated linearly between the two
  neighbouring results when that position is fractional, so a limit need not
  be an observed value. A definition that returns an observed value or the
  midpoint of two gives other limits.
- AVALVAR is the sample variance of the same results, with divisor one less
  than the number of results.
- A test not done takes no part in the percentiles, the variance or the
  number of results, and each parameter and sex takes its values from its
  own results alone.
- ANRLO, ANRHI and AVALVAR are the same on every record of the parameter
  and sex, including a record without a result.
- None of them is rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
