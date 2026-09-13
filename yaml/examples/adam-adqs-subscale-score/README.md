# Score a physical functioning subscale from its items

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adqs-subscale-score.html)

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
- `AVAL`: the numeric result on an item record, on the zero to
  four answer scale, or, on the score record, the mean of the
  answered items at that visit multiplied by 25; empty on the
  score record when fewer than three of the four items were
  answered.

**Note:** in this example each administered visit carries a score
record, even when too few items were answered to score it, so an
empty score stays apart from a visit with no records at all. Item
responses are expected within zero to four, and only `PF01`
through `PF04` plus `PFSCORE` appear as parameter codes.

**Standard:** ADaM | **Domain:** ADQS
