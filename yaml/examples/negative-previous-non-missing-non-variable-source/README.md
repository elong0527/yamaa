# ADaM ADVS: reject carrying forward a fixed value

This example uses collected vital signs to record one row per
measurement:

- `SYSBP` is the systolic blood pressure, missing when the measurement
  was not taken.
- `PRIORBP` carries the closest earlier taken pressure forward across
  any number of missed visits.

The carried value arrives as a fixed number instead of a variable to
search. No implementation may invent earlier rows holding that number,
so the specification is rejected before any data is read and no
artifact is accepted.

## How to fix

Decide which earlier measurement the study carries, then name the
variable that holds it. When the closest earlier taken pressure is the
rule, search the pressure variable itself:

```yaml
- name: PRIORBP
  type: int
  derivation:
    previous_non_missing:
      source: SYSBP
      group_by: [STUDYID, USUBJID]
      order_by:
        - {variable: VSSEQ, direction: asc}
```

When every gap should instead read a fixed fallback, state that as a
separate rule over the carried result rather than inside the search.
