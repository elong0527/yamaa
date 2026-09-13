# Reject a baseline tied on one collection date

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-baseline-flag-tied-date.html)

**Goal:** mark the baseline record of each subject and parameter
with `ABLFL` and repeat its value with `BASE`.

**Input:** collected laboratory results with test code, collection
date, and numeric result (`LBTESTCD`, `LBDTC`, `LBSTRESN`), plus
subject treatment start dates from ADSL (`TRTSDT`).

**Variables:**

- `ADT` would contain the collection date, taken from `LBDTC`.
- `TRTSDT` would contain the subject's treatment start date,
  taken from ADSL `TRTSDT`.
- `AVAL` would contain the analysis value, taken from `LBSTRESN`.
- `ABLFL` would be `Y` on the baseline record for the subject
  and parameter: the latest result on or before treatment start,
  and blank on every other record.
- `BASE` would contain the baseline record's `AVAL` on every
  record for the subject and parameter; missing when no result
  falls on or before treatment start.

Results sharing one collection date can tie for latest, and either
candidate gives the parameter a different baseline. The run fails
and no dataset is accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Decide which of the two draws is the baseline before choosing how to state it.
Two results drawn on the same day are usually a sample and its repeat, and the
source data system should carry the result the study reports, or a collection
time that separates the draws. Correcting it there leaves the rule saying what
it means: the latest result before treatment.

When both draws are reportable and the study states a tie-break, choose the
record explicitly instead of by date alone. Order the eligible results and mark
the first:

```yaml
- name: ABLRANK
  type: int
  derivation:
    row_number:
      group_by: [STUDYID, USUBJID, PARAMCD]
      order_by:
        - {variable: ADT, direction: desc}
        - {variable: LBSEQ, direction: desc}
      filter: "ADT <= TRTSDT"

- name: ABLFL
  type: str
  derivation:
    case:
      branches:
        - when: "ABLRANK = 1"
          then:
            literal: Y
```

Keep `ABLRANK` internal by omitting it from `output.columns`.

Order by the term the study names; the sequence number above stands in for it
and is not itself a clinical rule. Do not average the two results into one
baseline value, which reports a measurement nobody took.
