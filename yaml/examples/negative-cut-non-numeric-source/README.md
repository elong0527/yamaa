# ADaM ADSL: reject age bands grouped from a coded value

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-cut-non-numeric-source.html)

This example uses collected demographics with a `yamaa` specification to
derive one row per subject:

- `AGEGRP` is the age band the subject falls in, younger below 65 and
  elderly at or above. Grouping by a non-numeric value has no bands.

## How to fix

Group by the numeric age rather than the coded sex:

```yaml
cut:
  source: DM.AGE
  breaks: [65]
  labels: [younger, elderly]
```
