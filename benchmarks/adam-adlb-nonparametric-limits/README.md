# Derive sex-specific reference limits from the study's percentiles

**Goal:** derive the lower (`ANRLO`) and upper (`ANRHI`) limit of each
laboratory parameter's 95% reference range for each sex as the 2.5th and
97.5th percentiles of the study's own results, which assume no distribution,
and the variance of those results (`AVALVAR`), on every record of the
parameter and sex.

**Input:** standardized laboratory results (LB), one result per subject and
test, with no result when the test was not done; and subject-level analysis
data (ADSL) giving each subject's sex.

**Variables:**

- `SEX`: the subject's sex, from the subject-level data.
- `AVAL`: the standardized numeric result; empty when the test was not done.
- `ANRLO`: the 2.5th percentile of the results of the record's parameter
  and sex, by R's default definition (type 7). With the `n` results sorted
  ascending, it sits at position `h = (n - 1) * 0.025 + 1`: the `j`-th
  smallest result, `j` the whole part of `h`, plus `h - j` times the gap to
  the next one, so a limit need not be an observed value. The same on every
  record of the parameter and sex, including one without a result. It is
  not rounded.
- `ANRHI`: the 97.5th percentile, at `h = (n - 1) * 0.975 + 1`, under the
  same terms.
- `AVALVAR`: the sample variance of the same results, their squared
  deviations from their mean added and divided by `n - 1`, under the same
  terms.

**Note:** a test not done takes no part in its group's percentiles, variance
or count, and each parameter and sex takes its values from its own results
alone. Five results put the limits at positions 1.1 and 4.9, and four at
1.075 and 3.925; the binary rounding of a position reaches no written digit
here. The results sorted, and the arithmetic of each group:

    Creatinine (umol/L), female: 61, 66, 70, 73, 80
      ANRLO   = 61 + 0.1 * (66 - 61) = 61.5
      ANRHI   = 73 + 0.9 * (80 - 73) = 79.3
      AVALVAR = (81 + 16 + 0 + 9 + 100) / 4 = 51.5, about the mean 70
    Creatinine (umol/L), male: 78, 84, 88, 95, 105
      ANRLO   = 78 + 0.1 * (84 - 78) = 78.6
      ANRHI   = 95 + 0.9 * (105 - 95) = 104
      AVALVAR = (144 + 36 + 4 + 25 + 225) / 4 = 108.5, about the mean 90
    Hemoglobin (g/L), female: 118, 126, 131, 135, 140
      ANRLO   = 118 + 0.1 * (126 - 118) = 118.8
      ANRHI   = 135 + 0.9 * (140 - 135) = 139.5
      AVALVAR = (144 + 16 + 1 + 25 + 100) / 4 = 71.5, about the mean 130
    Hemoglobin (g/L), male, one test not done: 141, 147, 153, 159
      ANRLO   = 141 + 0.075 * (147 - 141) = 141.45
      ANRHI   = 153 + 0.925 * (159 - 153) = 158.55
      AVALVAR = (81 + 9 + 9 + 81) / 3 = 60, about the mean 150
