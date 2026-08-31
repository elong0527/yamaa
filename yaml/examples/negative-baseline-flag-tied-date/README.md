# ADaM ADLB: reject a baseline chosen between two same-day results

This example uses collected lab data with ADSL to prepare one record per
subject, parameter, and collected result:

- `ADT` is the collection date and `TRTSDT` the subject's treatment start date;
- `AVAL` is the analysis value of the result;
- `ABLFL` marks the baseline record of each subject and parameter: the latest
  result on or before treatment start;
- `BASE` repeats that record's value on every result for the parameter, so each
  result can be compared with it.

A subject whose last two results before treatment were drawn on the same day
has two candidates for that mark and nothing that separates them. Either
candidate gives the parameter a different baseline and moves every comparison
against it, so the run must fail and no artifact is accepted.

## How to fix

Decide which of the two draws is the baseline before choosing how to state it.
Two results drawn on the same day are usually a sample and its repeat, and the
governed source should carry the result the study reports, or a collection time
that separates the draws. Correcting it there leaves the rule saying what it
means: the latest result before treatment.

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
