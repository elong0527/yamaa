# Non-Finite Values

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-non-finite.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** show that non-finite numeric values (positive
infinity, negative infinity, and Not-a-Number (NaN)) are
stored as missing, no matter which of the three channels
carries them: a YAML literal, a source read, or a study
calculation.

**Input:** subject-level source fields `POSITIVE_INFINITY`,
`NEGATIVE_INFINITY`, and `NOT_A_NUMBER`, each holding a
non-finite value.

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
- `FUNCTION_PINF`: positive infinity from the study
  calculation; stored as missing.
- `FUNCTION_NINF`: negative infinity from the study
  calculation; stored as missing.
- `FUNCTION_NAN`: Not-a-Number (NaN) from the study
  calculation; stored as missing.

**Note:** a quoted spelling such as `.inf` remains text unless
converted to a numeric type.

**Engines:** the study calculation `numeric_constant` is
illustrated in both runtimes under the same contract
(`python/contracts.yaml`, conformance
`python/conformance/numeric_constant.yaml`): the Python
runtime in `python/runtime/projectconstants.py`, the R
runtime in `environment.R`. Both spell the same three
non-finite constants, and the engine stores each one as
missing.

**Standard:** ADaM | **Domain:** ADSL
