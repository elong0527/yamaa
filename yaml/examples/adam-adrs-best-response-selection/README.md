# Prepare assessments for best overall response selection

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adrs-best-response-selection.html)

**Goal:** prepare each collected overall response assessment for best
overall response (BOR) selection, adding `BORCAT`, `BORPRI`, and
`BORSEQ`.

**Input:** overall response assessments carrying the analysis date
(`ADT`), the assessment day relative to randomization (`RANDDY`), and
the collected overall response (`AVALC`).

**Variables:**

- `BORCAT` is the response category the record can support in the
  best overall response decision: complete response as `CR`, partial
  response as `PR`, stable disease as `SD`, neither complete response
  nor progressive disease as `NON-CR/NON-PD`, progressive disease as
  `PD`, or not evaluable as `NE`. Stable disease and
  neither-complete-nor-progressive disease count only on or after
  day 42 after randomization, and earlier ones fall back to not
  evaluable. Any other collected value, including a missing one,
  supports no category, so `BORCAT` stays empty.
- `BORPRI` orders the supported categories as complete response
  (`1`), partial response (`2`), stable disease (`3`),
  neither-complete-nor-progressive disease (`4`), progressive
  disease (`5`), not evaluable (`6`); empty when the record
  supports no category.
- `BORSEQ` numbers the usable records of each study and subject in
  category order, then by analysis date, then by assessment
  sequence. The record numbered `1` supplies the study-subject's
  best overall response and its supporting date.

**Note:** a record that supports no category takes no priority and no
number, so numbering passes over it to the next usable record: it can
never be numbered `1`, and it never consumes a number a usable record
would otherwise take.

**Standard:** ADaM | **Domain:** ADRS
