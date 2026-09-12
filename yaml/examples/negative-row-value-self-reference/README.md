# ADaM ADVS: reject a weight carried forward from a carried-forward weight

This example uses a series of collected weights to attempt one analysis record
per measurement:

- `AVAL` is the collected weight and is empty when no measurement was taken;
- `AVALF` is meant to hold the collected weight, or the most recent earlier
  weight when none was collected.

Two consecutive measurements are missing, so the second gap would have to take
its value from a record that was itself filled in, and `AVALF` is stated in
terms of its own earlier value. Filling one gap and stopping, or resolving the
rule in an order nothing states, would each give a different answer from the
same data, so the run must fail and no artifact is accepted.

## How to fix

Carry from the collected series rather than from the filled output. Search the
earlier collected values, then coalesce that result with the current value:

```yaml
- name: PRIOR
  type: float
  derivation:
    previous_non_missing:
      source: AVAL
      group_by: [STUDYID, USUBJID, PARAMCD]
      order_by: [ADT, VSSEQ]
- name: AVALF
  type: float
  derivation:
    coalesce:
      sources: [AVAL, PRIOR]
```

Keep `PRIOR` internal by omitting it from `output.columns`.
