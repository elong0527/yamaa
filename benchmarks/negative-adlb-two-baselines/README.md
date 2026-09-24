# Reject Duplicate Baselines

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-adlb-two-baselines.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry over the analysis value (`AVAL`) and the baseline
record flag (`ABLFL`) for each laboratory record.

**Input:** pre-derived laboratory records carrying study, subject,
and parameter identifiers, the analysis date (`ADT`), the analysis
value (`AVAL`), and the baseline record flag (`ABLFL`).

**Variables:**

- `ABLFL` is the input baseline flag: `Y` on the record that serves as the
  baseline for the subject and parameter, blank on every other record.

**Note:** each subject and parameter must have exactly one record flagged
`Y`; a combination with none is rejected as well. Two flagged records leave
the change from baseline undefined, and neither record can be preferred
without inventing a rule the study did not state. The run is rejected and no
artifact is accepted. The expected output records the completed rows
presented to that check.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Correct the flag in the incoming records so that one record carries
it, choosing the record the study's baseline definition selects,
ordinarily the latest result on or before the first exposure. When
the flag should be derived here instead of trusted from the input,
derive it and let a tie be reported where it arises:

```yaml
- name: ABLFL
  type: str
  label: Baseline Record Flag
  derivation:
    baseline_flag:
      window:
        group_by: [STUDYID, USUBJID, PARAMCD]
      date: ADT
      reference_date: TRTSDT
```

Do not widen the count to accept two records; a second baseline is
a defect in the data rather than a policy the analysis can adopt.
