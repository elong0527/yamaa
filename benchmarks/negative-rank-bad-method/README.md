# Reject Invalid Ranking

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-rank-bad-method.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** rank each subject's adverse events (AEs) by reported
severity, carrying through `AESEV` and numbering the events in
`SEVRANK`.

**Input:** collected adverse event records carrying the reported
severity (`AESEV`).

**Variables:**

- `SEVRANK` would be the number of the event among the subject's
  events ordered by severity, worst first, with equally severe
  events sharing one number so they compare as equal. An event
  with no reported severity would come after every rated one.

**Note:** the tie-numbering choice arrives as a structured value
instead of one of the two named methods. No reader may guess which
method a structure means, so the run is rejected before any data is
read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Decide how ties consume numbers, then name the method. When equally severe
events share the lowest position they occupy and the next severity continues
after the gap, write it plainly:

```yaml
- name: SEVRANK
  type: int
  derivation:
    rank:
      method: competition
      window:
        group_by: [STUDYID, USUBJID]
        order_by:
          - {variable: AESEV, direction: desc}
```

When no numbers may be skipped, name the dense method instead. Do not encode
the choice in a structure the vocabulary does not define.
