# Non-Finite Values

**Goal:** show that a non-finite number (positive infinity, negative
infinity, or Not-a-Number, NaN) is stored as missing, whichever of
three routes brings it into a column: a value written in the
specification, a value read from the source, or a value returned by a
study calculation.

**Input:** one `spec.yaml` over a subject-level dataset whose numeric
fields `POSITIVE_INFINITY`, `NEGATIVE_INFINITY`, and `NOT_A_NUMBER`
each hold one non-finite value.

**Columns:** every one of these is stored as missing.

- `YAML_PINF`, `YAML_NINF`, and `YAML_NAN` write positive infinity,
  negative infinity, and NaN directly in `spec.yaml`.
- `SOURCE_PINF`, `SOURCE_NINF`, and `SOURCE_NAN` copy them from
  `POSITIVE_INFINITY`, `NEGATIVE_INFINITY`, and `NOT_A_NUMBER`.
- `FUNCTION_PINF`, `FUNCTION_NINF`, and `FUNCTION_NAN` take them from
  the study calculation `numeric_constant`. Its contract allows a
  missing result; a calculation that may not return one fails the run
  when it returns a non-finite value.

**Note:** a quoted spelling such as `'.inf'` remains text unless
converted to a numeric type.

**Engines:** `numeric_constant` belongs to the ordinary installed packages
in `rust/tests/project-programs`. The function file
`functions/numeric_constant.yaml` declares it once, with its inline tests, and
names the Python and R implementations, which return the same three
non-finite constants.
