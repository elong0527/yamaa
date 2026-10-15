# Multi-Scale Instrument Scoring

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adqs-multi-scale-scoring.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each questionnaire item response into `PARAMCD`, `PARAM`, and
`AVAL`, and add one scale score record per scale per visit, coded `F1SCORE`,
`F2SCORE`, `SSCORE`, and `GSCORE`, holding the mean answered-item response
transformed to a zero to one hundred scale.

**Input:** collected questionnaire responses for a ten-item instrument, with
study, subject, and visit (`STUDYID`, `USUBJID`, `VISIT`), category
(`QSCAT`), test code and test name (`QSTESTCD`, `QSTEST`), and numeric result
(`QSSTRESN`): four physical functioning items and two role functioning items
on the one to four answer scale, three fatigue and sleep symptom items on the
one to four answer scale, and two global health items on the one to seven
answer scale.

**Variables:**

- `PARAMCD`: the test code (`F101` through `F104`, `F201`, `F202`, `S01`,
  `S02`, `G01`, `G02`) on an item record, or the scale code (`F1SCORE`,
  `F2SCORE`, `SSCORE`, `GSCORE`) on an added score record.
- `PARAM`: the test name on an item record, or the scale name on the added
  score record.
- `AVAL`: on an item record, the numeric result on its answer scale, empty
  when the item was not answered; on a score record, the scale's mean
  answered-item response on a zero to one hundred scale, where the
  functioning scales read higher for better functioning, the symptom scale
  reads higher for worse symptoms, and the global scale reads higher for
  better health; empty when too few of the scale's items were answered.

**Note:** every visit with an anchor item record (`F101`, `F201`, `S01`, or
`G01`), answered or not, carries that scale's score record, so a visit with
too few answers to score keeps an empty score and stays apart from a visit
with no records at all. One item, `F104`, belongs to two scales and feeds
both of their scores.

**Standard:** ADaM | **Domain:** ADQS
