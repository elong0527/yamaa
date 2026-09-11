# ADaM ADLB: reject writing collected text to a fixed number of places

The collected lab results produce one row per subject and test:

- `LBORRES` is the result exactly as the laboratory reported it, which may be
  a number or a remark such as a below-limit result;
- `AVALC` is rejected because a result written to a fixed number of places has
  to be written from a number, and the collected report is text.

## How to fix

Decide what the collected text means before reporting it at a precision. A
laboratory report that is always numeric is read as a number at its source and
then written:

```yaml
datasets:
  LB: {path: input/lb.csv, types: {LBORRES: float}}
```

```yaml
- name: AVALC
  type: str
  derivation:
    format_number: {source: AVAL, decimals: 2}
```

A report that also carries remarks such as `<2` cannot be read that way,
because no number stands behind the remark. Keep the collected text as it was
reported, derive the numeric result separately for the records that have one,
and write that result to its places.
