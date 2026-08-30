# ADaM ADSL: reject a treatment ordered but not chosen

This example uses collected demographics with exposure records to attempt one
record per subject:

- `TRT01A` is the treatment the subject received.

The administrations are put in order and the specification stops there, so
nothing says whether the treatment comes from the earliest or the latest of
them. The two answers differ for every subject who changed treatment, so the
run must fail and no artifact is accepted.

## How to fix

Pair `order_by` with an explicit `keep`. For the earliest treatment:

```yaml
record_lookups:
  - id: DOSING
    dataset: EX
    order_by: [EX.EXSTDTC, EX.EXSEQ]
    keep: first
```

Use `keep: last` only when the intended result is the latest treatment.
