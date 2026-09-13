# ADaM ADVS: reject a severity carried from its own column

This example uses vital-sign records with a `yamaa` specification to derive
one row per measurement:

- `SEVAL` is the severity recorded at the visit, or the severity carried
  forward from the earlier rows when the visit has none. A severity that
  falls back to its own column has no earlier value to carry.

## How to fix

Carry forward from the collected source instead of the column being
derived. Search the collected values on the earlier rows:

```yaml
previous_non_missing:
  source: VS.SEVAL
```

and, when the first row may itself be missing, keep the default for that
row only.

[Rendered view](https://elong0527.github.io/yamaa/examples/negative-coalesce-self-reference.html)
