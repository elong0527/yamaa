# ADaM ADSL: reject endpoint counting beside a month count

This example uses collected demographics to record one row per subject:

- `STDT` is the start date of the exposure.
- `ENDT` is the end date of the exposure.
- `DURM` counts whole months between them, both endpoints included.

Counting endpoints has no meaning beside months: one greater than a
month count is not an age anyone recognizes, so no implementation may
accept the combination silently. The specification is rejected before
any data is read and no artifact is accepted.

## How to fix

Decide whether the study counts days or whole months, then state only
that. A month count stands on its own with no endpoint adjustment:

```yaml
- name: DURM
  type: int
  derivation:
    date_diff:
      start: STDT
      end: ENDT
      unit: month
```

When the study counts days with both endpoints included, keep
`unit: day` beside the endpoint adjustment instead.

[Rendered view](https://elong0527.github.io/yamaa/examples/negative-date-diff-bounds-unit.html)
