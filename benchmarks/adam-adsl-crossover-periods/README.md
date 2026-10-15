# Derive period-scoped treatments and dates across a washout

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-crossover-periods.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `TR01SDT`, `TR01EDT`, `TR02SDT`, `TR02EDT`,
`TRT01A`, `TRT02A`, and `WASHDUR` to capture each treatment
period and the washout between them, with one record per
subject.

**Input:** demographics listing each subject, plus exposure
records carrying the epoch (`EPOCH`), the treatment
given (`EXTRT`), start and end dates (`EXSTDTC`, `EXENDTC`), and
a sequence number (`EXSEQ`).

**Variables:**

- `TR01SDT` is the earliest `EXSTDTC` among the subject's
  exposure records in the `TREATMENT 1` epoch and a recorded start
  date; empty when the subject has no such record.
- `TR01EDT` is the latest `EXENDTC` among the subject's
  exposure records in the `TREATMENT 1` epoch and a recorded end
  date; empty when the subject has no such record.
- `TR02SDT` is the earliest `EXSTDTC` among the subject's
  exposure records in the `TREATMENT 2` epoch and a recorded start
  date; empty when the subject has no such record.
- `TR02EDT` is the latest `EXENDTC` among the subject's
  exposure records in the `TREATMENT 2` epoch and a recorded end
  date; empty when the subject has no such record.
- `TRT01A` is the `EXTRT` from the subject's earliest
  exposure record in the `TREATMENT 1` epoch, earliest by `EXSTDTC`
  with the lower `EXSEQ` breaking ties on the same day and a
  record with no start date sorting last; either `VITAMIN D3`
  or `PLACEBO`, and empty when the subject has no period-one
  exposure.
- `TRT02A` is the `EXTRT` from the subject's earliest
  exposure record in the `TREATMENT 2` epoch, chosen the same way;
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
