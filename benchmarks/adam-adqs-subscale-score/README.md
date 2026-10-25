# Subscale Scoring

**Goal:** carry each physical functioning item response into
`PARAMCD`, `PARAM`, and `AVAL`, add a subscale score record
coded `PFSCORE` holding the mean answered-item response multiplied
by 25 (mean divided by four, times 100), and say in `AREASND` why
a value is missing.

**Input:** collected questionnaire responses for the physical
functioning scale, with study, subject, and visit (`STUDYID`,
`USUBJID`, `VISIT`), category (`QSCAT`), test code and test name
(`QSTESTCD`, `QSTEST`), and numeric result (`QSSTRESN`) on the
zero to four answer scale. An unanswered item is marked not done
(`QSSTAT`), with the reason (`QSREASND`) when one was recorded.
One record on the emotional functioning scale is included and
excluded from the output, showing that only the physical
functioning records feed the dataset.

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
- `AREASND`: on an item record, the reason recorded for an
  unanswered item, empty otherwise. On a score record without a
  value, the reason the visit's items record when none was
  answered and they record one reason between them, and
  `NOT CALCULABLE` otherwise; empty on a score record with a value.

**Note:** every visit with a `PF01` item record, answered or not,
carries a score record, so a visit with too few answers to score
keeps an empty score and stays apart from a visit with no records
at all.
