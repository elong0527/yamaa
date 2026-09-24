# Non-Finite Values

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-non-finite.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

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

**Engines:** `numeric_constant` is implemented in both runtimes under
one contract (`python/contracts.yaml`, checked against
`python/conformance/numeric_constant.yaml`): Python in
`python/runtime/projectconstants.py` and R in `environment.R`. Both
return the same three non-finite constants.

**Standard:** ADaM | **Domain:** ADSL
