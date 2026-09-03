# ADaM ADSL: normalize non-finite numeric values to missing

This example uses one subject record to show that every non-finite numeric
value is stored as missing:

- `YAML_PINF`, `YAML_NINF`, and `YAML_NAN` receive positive infinity,
  negative infinity, and NaN from values written directly in the plan;
- `SOURCE_PINF`, `SOURCE_NINF`, and `SOURCE_NAN` receive the same three values
  from numeric source fields.

All six derived numeric values therefore have the same missing value in the
artifact. A quoted YAML spelling such as `".inf"` would remain text unless it
were converted to a numeric type.
