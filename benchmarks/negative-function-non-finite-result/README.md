# Reject a Non-Finite Result From a Study Calculation

**Goal:** take each record's `VALUE` from the study's `numeric_constant`
calculation, for the constant that `KIND` names.

**Input:** one record per identifier naming a constant: one, zero or
positive infinity.

**Variables:**

- `KIND` is the name of the constant as stored.
- `VALUE` would be the constant the calculation returns.

The calculation promises a number for every kind. For positive infinity it
returns infinity, which is stored as missing, as every non-finite number is,
so the calculation has returned a missing value it promised it would not.
The run is rejected, and no artifact is accepted.

## How to fix

Decide whether a missing result is acceptable for this calculation. If it
is, say so in its function file, as `schema-non-finite` does, and add a test
for each kind that returns one:

```yaml
returns: float
may_return_missing: true
```

If it is not, correct the stored kind so that it names a finite constant.
