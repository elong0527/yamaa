# ADaM ADAE: reject a review flag whose condition performs arithmetic

The collected adverse events produce one analysis row per reported event:

- `REVIEWFL` is rejected because its decision changes the sequence number
  while testing it instead of comparing a named value.

## How to fix

State the equivalent comparison directly when no intermediate value is needed:

```yaml
when: "AE.AESEQ > 0"
```
