# ADaM ADAE: reject a severity burden totalled from severity words

This example uses collected adverse events to attempt one record per event:

- `ASEV` is the reported severity of the event;
- `SEVTOT` is meant to be a subject's total severity burden across the events
  reported for them.

Severity is recorded as a word, and words have no total. Ordering the words and
totalling their positions is a real rule, but it is a different one, and the
specification never states the numbers it would use. Choosing them here would
put the study's severity scale outside the specification, so the run must fail
and no artifact is accepted.

## How to fix

Declare the study's numeric severity scale first, then aggregate that numeric
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

Keep `ASEVN` internal by omitting it from `output.columns`.

The numeric assignments are analysis policy and must be confirmed rather than
inferred from the order of the words.
