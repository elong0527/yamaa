# ADaM ADSL: reject a site name folded from a number

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

[Rendered view](https://elong0527.github.io/yamaa/examples/negative-str-lower-non-string-source.html)
