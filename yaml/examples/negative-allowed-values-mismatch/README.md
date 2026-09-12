# ADaM ADSL: reject a recorded sex the study does not recognize

This example uses collected demographics with a `yamaa` specification to
derive one row per subject:

- `SEX` is the subject's recorded sex, either of the two study codes. A
  record carrying any other code has none the study recognizes.

## How to fix

Correct the offending record at collection, or widen the accepted codes
when the study genuinely admits a third value:

```yaml
verifications:
  - allowed_values:
      values: [M, F, U]
```
