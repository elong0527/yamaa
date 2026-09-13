# Reject an unlisted severity tie-numbering method

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-rank-invalid-method.html)

**Goal:** rank each subject's adverse events (AEs) by reported
severity, carrying through `AESEV` and numbering ties with
`SEVRANK`.

**Input:** collected adverse event records carrying the reported
severity (`AESEV`).

**Variables:**

- `AESEV` would be the reported severity of the event, carried
  through unchanged; blank when no severity was reported.
- `SEVRANK` would be the number of the event among the subject's
  events ordered by severity, worst first, with equally severe
  events sharing one number so they compare as equal.

The tie-numbering choice arrives as a structured value instead of
one of the two named methods. No reader may guess which method a
structure means, so the run is rejected before any data is read
and no artifact is accepted.

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
      group_by: [STUDYID, USUBJID]
      order_by:
        - {variable: AESEV, direction: desc}
```

When no numbers may be skipped, name the dense method instead. Do not encode
the choice in a structure the vocabulary does not define.
