# Derive each parameter's reference limits from the study's results

**Goal:** derive the lower (`ANRLO`) and upper (`ANRHI`) limit of each
laboratory parameter's 95% reference range from the study's own results,
and give both on every record of the parameter.

**Input:** standardized laboratory results (LB), one result per subject and
test, with no result when the test was not done.

**Variables:**

- `AVAL`: the standardized numeric result; empty when the test was not done.
- `ANRLO`: the mean of the parameter's results less 1.96 standard
  deviations: exactly, the standard normal distribution's 97.5th percentile
  times the sample standard deviation, with divisor one less than the number
  of results. The same on every record of the parameter, including one
  without a result. It is not rounded.
- `ANRHI`: the mean plus the same multiple of the standard deviation, under
  the same terms.

**Note:** a test not done takes no part in its parameter's mean or standard
deviation, and each parameter's limits come from its own results alone. The
limits bound the central 95% of a normal distribution with that mean and
standard deviation.
