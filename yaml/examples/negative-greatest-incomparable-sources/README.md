# ADaM ADSL: reject a last-known-alive date taken from a day number

This example uses collected demographics to attempt one record per subject:

- `DTHDT` is the collected date of death and is empty for a subject who is
  alive;
- `LSTVSDY` is the study day of the last visit, counted from the first dose;
- `LSTALVDT` is meant to be the latest date on which the subject was known to
  be alive.

A calendar date and a day number are not on the same scale, so asking which of
the two is later has no answer. Comparing the day number with the year, or
reading it as a date, would each invent a rule the specification never stated,
so the run must fail and no artifact is accepted.

## How to fix

Put both candidates on the same scale before comparing them. For example,
carry the actual last-visit date in the source and compare two date columns:

```yaml
- name: LSTVSDT
  type: date
  derivation:
    source: DM.LSTVSDTC

- name: LSTALVDT
  type: date
  derivation:
    greatest:
      sources: [DTHDT, LSTVSDT]
```

If only `LSTVSDY` is available, derive its calendar date from the study-day
reference under a separately stated rule before using `greatest`.
