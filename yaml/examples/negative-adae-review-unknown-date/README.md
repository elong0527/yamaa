# ADaM ADAE: reject a review flag that names an unavailable date

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-adae-review-unknown-date.html)

The collected adverse events produce one analysis row per reported event:

- `REVIEWFL` is rejected because its decision names a date that the output
  does not provide.

## How to fix

Use the declared analysis start date in the condition:

```yaml
when: "ASTDT >= DATE '2025-01-01'"
```
