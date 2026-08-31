# ADaM ADSL: reject an uppercased country chosen inside the same step

This example uses collected demographics with a site-level country to attempt
one record per subject:

- `COUNTRY` is the collected country in capitals, taken from the site when the
  subject's own country was not collected.

The step that converts the text also chooses which text to convert, so the
choice is buried inside another operation and nothing names the value being
chosen. A specification states one step at a time and gives each result a name,
so the run must fail and no artifact is accepted.

## How to fix

Derive the selected source in an intermediate column, then pass that variable
to `str_upper`:

```yaml
- name: COUNTRYSRC
  type: str
  derivation:
    coalesce:
      sources: [DM.COUNTRY, DM.SITECNTY]

- name: COUNTRY
  type: str
  derivation:
    str_upper:
      source: COUNTRYSRC
```

Keep `COUNTRYSRC` internal by omitting it from `output.columns`.

Fields typed as `variable`, including `str_upper.source`, accept a variable
name rather than a nested expression.
