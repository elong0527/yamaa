# Subscale Scoring

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adqs-subscale-score.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each physical functioning item response into
`PARAMCD`, `PARAM`, and `AVAL`, and add a subscale score record
coded `PFSCORE` holding the mean answered-item response multiplied
by 25 (mean divided by four, times 100).

**Input:** collected questionnaire responses for the physical
functioning scale, with study, subject, and visit (`STUDYID`,
`USUBJID`, `VISIT`), category (`QSCAT`), test code and test name
(`QSTESTCD`, `QSTEST`), and numeric result (`QSSTRESN`).

**Variables:**

- `PARAMCD`: the test code (`PF01` through `PF04`) on an item
  record, or `PFSCORE` on the added score record.
- `PARAM`: the test name on an item record, or the subscale name
  on the added score record.
- `AVAL`: on an item record, the numeric result on the zero to
  four answer scale, empty when the item was not answered; on the
  score record, the mean of the answered items at that visit
  multiplied by 25, empty when fewer than three of the four items
  were answered.

**Note:** every visit with a `PF01` item record, answered or not,
carries a score record, so a visit with too few answers to score
keeps an empty score and stays apart from a visit with no records
at all.

**Standard:** ADaM | **Domain:** ADQS
