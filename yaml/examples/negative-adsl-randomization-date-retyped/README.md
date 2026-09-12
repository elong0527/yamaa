# ADaM ADSL: reject a randomization date described twice

This example uses an ODM demographics projection to produce one record per
subject:

- `RANDDT` is the subject's randomization date.

The randomization date already carries its data type. Describing it again
beside the source would leave two authorities for the same field even when they
currently agree, so the specification must be rejected and no artifact is
accepted.

## How to fix

Keep the producing DM specification as the single type authority and remove
the inline `types` entry:

```yaml
datasets:
  DM:
    path: input/dm.csv
    schema: input/dm.schema.yaml
```
