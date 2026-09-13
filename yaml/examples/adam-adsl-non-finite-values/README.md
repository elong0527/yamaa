# Normalize non-finite numeric values to missing

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-non-finite-values.html)

**Goal:** store positive infinity, negative infinity, and
Not-a-Number (NaN) results as missing.

**Input:** subject-level source fields `POSITIVE_INFINITY`,
`NEGATIVE_INFINITY`, and `NOT_A_NUMBER`, each holding a non-finite
value; the same three values are also stated directly and returned
by a study calculation.

**Variables:**

- `YAML_PINF`: positive infinity stated directly; stored as
  missing.
- `YAML_NINF`: negative infinity stated directly; stored as
  missing.
- `YAML_NAN`: Not-a-Number (NaN) stated directly; stored as
  missing.
- `SOURCE_PINF`: positive infinity copied from
  `POSITIVE_INFINITY`; stored as missing.
- `SOURCE_NINF`: negative infinity copied from
  `NEGATIVE_INFINITY`; stored as missing.
- `SOURCE_NAN`: Not-a-Number (NaN) copied from `NOT_A_NUMBER`;
  stored as missing.
- `FUNCTION_PINF`: positive infinity from the study calculation;
  stored as missing.
- `FUNCTION_NINF`: negative infinity from the study calculation;
  stored as missing.
- `FUNCTION_NAN`: Not-a-Number (NaN) from the study calculation;
  stored as missing.

**Note:** a quoted spelling such as `.inf` remains text unless
converted to a numeric type.

**Standard:** ADaM | **Domain:** ADSL
