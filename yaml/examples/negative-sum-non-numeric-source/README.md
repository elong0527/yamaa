# Reject a severity burden totalled from severity words

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-sum-non-numeric-source.html)

**Goal:** build one analysis record per collected adverse event,
carrying `ASEV` and adding `SEVTOT`.

**Input:** collected adverse events with the severity word recorded
in `AESEV`.

**Variables:**

- `ASEV` would contain the severity word reported for the event,
  carried unchanged from the collected severity.
- `SEVTOT` would be the subject's total severity burden, totalling
  the severity across the events reported for them, but no row is
  produced.

**Note:** severity is recorded as words, and words have no total.
Ordering the words and totalling their positions would be a real
rule, but it is a different one, and the request never states the
numbers it would use. The run is rejected before any data is read,
so no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Declare the study's numeric severity scale first, then total that numeric
column:

```yaml
- name: ASEVN
  type: int
  derivation:
    mapping:
      source: ASEV
      dict:
        MILD: 1
        MODERATE: 2
        SEVERE: 3

- name: SEVTOT
  type: int
  derivation:
    aggregate:
      group_by: [STUDYID, USUBJID]
      expr: "SUM(ASEVN)"
```

Keep the intermediate numeric column out of the artifact by leaving it off
the artifact column list. The numeric assignments are analysis policy and
must be confirmed rather than inferred from the order of the words.
