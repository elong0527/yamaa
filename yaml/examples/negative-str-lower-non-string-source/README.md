# ADaM ADSL: reject a site name folded from a number

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-str-lower-non-string-source.html)

This example uses collected demographics with a `yamaa` specification to
derive one row per subject:

- `SITE` is the site name in lowercase. Folding a numeric site number has
  no lowercase form.

## How to fix

Fold the collected site name rather than its number:

```yaml
str_lower:
  source: DM.SITENM
```
