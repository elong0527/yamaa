# Derive growth percentile records

**Goal:** turn each collected body measurement (`BMI` or `WEIGHT`)
into one growth-percentile record holding the percentile in `AVAL`;
the collected records are not kept.

**Input:** collected measurements carrying the measurement code
(`PARAMCD`), the collected result (`AVAL`, stored as text), sex
(`SEX`), and age in days (`AGE`); alongside a sex-and-age growth
reference carrying the coefficients `L`, `M`, and `S` for each
measurement code, sex, and age in days.

**Variables:**

- `PARAMCD`: `BMIPCTL` for a collected `BMI` measurement,
  `WGTPCTL` for a collected `WEIGHT` measurement.
- `PARAM`: `BMI-for-Age Percentile` / `Weight-for-Age Percentile`,
  matching `PARAMCD`.
- `AVAL`: the growth percentile, 100 times the standard normal
  cumulative probability of the LMS z-score: the collected result
  divided by `M`, raised to the power `L`, minus 1, all divided by
  `L` times `S`. The `L`, `M`, and `S` are the reference
  coefficients matched on measurement code, sex, and age in days.
  A measurement with no matching reference row, or with no
  collected result, leaves `AVAL` missing.

**Note:** a collected result exactly at the reference median (`M`)
has a z-score of 0 and so a percentile of exactly 50.

## Specification variants

- `spec_function.yaml` takes the standard normal probability from the
  project's `normal_cdf` function, implemented once in each host.
- `spec_norm_cdf.yaml` takes it from the formula grammar's `NORM_CDF` and
  needs no project; it writes the same bytes.
