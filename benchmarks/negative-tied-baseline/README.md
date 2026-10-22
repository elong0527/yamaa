# Reject Tied Baseline

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

**Note:** results sharing one collection date can tie for latest,
and either candidate gives the parameter a different baseline. The
run fails and no dataset is accepted.

## How to fix

Decide which of the two draws is the baseline before choosing how to state it.
Two results drawn on the same day are usually a sample and its repeat, and the
source data system should carry the result the study reports, or a collection
time that separates the draws. Correcting it there leaves the rule saying what
it means: the latest result on or before treatment start.

When both draws are reportable and the study states a tie-break, declare it.
The baseline is then the last of the same-day results in that order:

```yaml
- name: ABLFL
  type: str
  derivation:
    baseline_flag:
      window:
        group_by: [STUDYID, USUBJID, PARAMCD]
        order_by: [ADT, LBSEQ]
      date: ADT
      reference: TRTSDT
      source: AVAL
```

Order by the term the study names, such as a collection time; the sequence
number above stands in for it and is not itself a clinical rule. Results still
equal on every declared term fail the same way. Do not average the two results
into one baseline value, which reports a measurement nobody took.
