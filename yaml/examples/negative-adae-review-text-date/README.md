# ADaM ADAE: reject a review flag that compares a date with text

The collected adverse events produce one analysis row per reported event:

- `REVIEWFL` is rejected because its calendar date is compared with text,
  which could otherwise inherit a runtime's implicit conversion.

## How to fix

Declare the constant as a date so both operands carry the same temporal type:

```yaml
when: "ASTDT >= DATE '2025-01-01'"
```
