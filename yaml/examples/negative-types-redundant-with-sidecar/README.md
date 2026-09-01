# ADaM ADSL: reject a source type repeated beside its schema

This example uses demographics to produce one record per subject:

- `RANDDT` is the subject's randomization date.

The source schema already identifies the randomization date as a date. A
second description beside the source would leave two authorities for the same
field even when they currently agree, so the specification must be rejected
and no artifact is accepted.

## How to fix

Keep the source schema as the single type authority and remove the inline
`types` entry:

```yaml
datasets:
  DM:
    path: input/dm.csv
    schema: input/dm.schema.yaml
```
