# Derive analysis period start and end dates in a two-period study

**Goal:** derive `AP01SDT`, `AP01EDT`, `AP02SDT`, and `AP02EDT`, the
start and end dates of the two analysis periods, with one record per
subject: period one ends the day before period two starts, and the last
period a subject entered ends 30 days after last exposure.

**Input:** demographics listing each subject, plus exposure records
carrying the epoch (`EPOCH`, either `TREATMENT 1` or `TREATMENT 2`), the
start and end dates (`EXSTDTC`, `EXENDTC`), and a sequence number
(`EXSEQ`).

**Variables:**

- `AP01SDT` is the earliest `EXSTDTC` among the subject's exposure
  records in the `TREATMENT 1` epoch, skipping a record with no start
  date; empty when no such record has a start date.
- `AP01EDT` is the day before `AP02SDT` when the subject entered period
  two. Otherwise it is 30 days after the subject's last exposure end
  date, so a subject who stopped after period one is followed up within
  period one. Empty when the subject has no exposure end date.
- `AP02SDT` is the earliest `EXSTDTC` among the subject's exposure
  records in the `TREATMENT 2` epoch, skipping a record with no start
  date; empty when the subject never entered period two, that is, has no
  such record with a start date.
- `AP02EDT` is 30 days after the subject's last exposure end date; empty
  when the subject never entered period two.

**Note:** the last exposure end date is the latest `EXENDTC` over all of
the subject's exposure records, including one with no start date. Each
date moves by whole calendar days across month ends, year ends and
29 February, and a subject with no exposure records has all four dates
empty.
