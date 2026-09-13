# Crossover period treatments, dates, and washout

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-crossover-periods.html)

**Goal:** derive `TR01SDT`, `TR01EDT`, `TR02SDT`, `TR02EDT`,
`TRT01A`, `TRT02A`, and `WASHDUR` to capture each treatment
period and the washout between them, with one record per
subject.

**Input:** demographics listing each subject, plus exposure
records carrying `APERIOD`, `EXTRT`, `EXSTDTC`, `EXENDTC`, and
`EXSEQ`.

**Variables:**

- `TR01SDT` is the earliest `EXSTDTC` among the subject's
  exposure records with `APERIOD` of `1` and a recorded start
  date; empty when the subject has no such record.
- `TR01EDT` is the latest `EXENDTC` among the subject's
  exposure records with `APERIOD` of `1` and a recorded end
  date; empty when the subject has no such record.
- `TR02SDT` is the earliest `EXSTDTC` among the subject's
  exposure records with `APERIOD` of `2` and a recorded start
  date; empty when the subject has no such record.
- `TR02EDT` is the latest `EXENDTC` among the subject's
  exposure records with `APERIOD` of `2` and a recorded end
  date; empty when the subject has no such record.
- `TRT01A` is the `EXTRT` from the subject's earliest
  exposure record with `APERIOD` of `1`, earliest by `EXSTDTC`
  with the lower `EXSEQ` breaking ties on the same day;
  either `VITAMIN D3` or `PLACEBO`, and empty when the subject
  has no period-one exposure.
- `TRT02A` is the `EXTRT` from the subject's earliest
  exposure record with `APERIOD` of `2`, chosen the same way;
  either `VITAMIN D3` or `PLACEBO`, and empty when the
  subject has no period-two exposure.
- `WASHDUR` is the number of days strictly between `TR01EDT`
  and `TR02SDT`, counting neither endpoint, so periods that
  touch give zero; empty when either date is missing.

**Note:** every value comes only from exposure records in its
own period, so period-one dates and treatment never mix with
period-two records. A subject with no period-two exposure
leaves `TR02SDT`, `TR02EDT`, `TRT02A`, and `WASHDUR` empty
together.

**Standard:** ADaM | **Domain:** ADSL
