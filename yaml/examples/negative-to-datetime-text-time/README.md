# ADaM ADPC: reject a sample moment composed from unchecked text

This example uses collected pharmacokinetic samples to attempt one analysis row
per specimen:

- `ADTM` is meant to combine the sample date with a valid local time of day.

The time is still unchecked text, so combining it would bypass the time ranges
and canonical form that make the resulting moment portable. The run must fail
before reading data.

## How to fix

Declare the collected field as a time before composing the moment:

```yaml
datasets:
  PC:
    path: input/pc.csv
    types: {PCSEQ: int, PCDAT: date, PCTIM: time}
```
