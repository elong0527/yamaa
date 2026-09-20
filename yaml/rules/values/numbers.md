---
id: values/numbers
title: Numeric values
status: normative
---

# Numeric values

## Purpose

Define numeric representation, promotion, conversion, and overflow.

## Scope and dependencies

This contract owns numeric representation, promotion, overflow, and
conversion details. [Types](types.md) owns the conversion matrix and missing
normalization. [Computation](../operations/computation.md) owns arithmetic
syntax, functions, and evaluation order. [CSV](../storage/csv.md) owns display
precision, and [functions](../operations/functions.md) own conformance
comparison. The numeric grammar remains in [grammar/numeric.yaml](../../grammar/numeric.yaml).

## Requirements

<a id="req-0014"></a>

**REQ-0014.** `int` is a signed 64-bit integer, from `-2^63` through
`2^63 - 1`. `float` is IEEE 754 binary64. Non-finite values follow [REQ-0006](types.md#req-0006)
before any consumer can use them.

<a id="req-0015"></a>

**REQ-0015.** Parsing text as a number must accept the numeric grammar's
`number` production with an optional leading `+` or `-`, without surrounding
whitespace. It must also recognize exactly `.inf`, `.Inf`, and `.INF` with
an optional leading sign, and `.nan`, `.NaN`, and `.NAN` without a sign.
Recognized non-finite forms parse as floats and immediately become missing.
Every other spelling must fail.

<a id="req-0016"></a>

**REQ-0016.** Conversion to `int` must succeed only for an exactly integral
numeric value within the signed 64-bit range. It must not round or truncate.
A specification choosing an integral part must do so explicitly with
`FLOOR`, `CEIL`, or `TRUNC` before column conversion.

<a id="req-0017"></a>

**REQ-0017.** Widening `int` to `float` is exact for magnitudes below `2^53`.
Larger magnitudes use the nearest binary64 value.

<a id="req-0018"></a>

**REQ-0018.** Converting `int` to `str` uses decimal text. Converting a finite
`float` to `str` must use the shortest sequence of significant decimal digits
that parses back to the same binary64 value, written in positional notation.
An integral float omits the trailing `.0`. The text must not contain an
exponent: shortest chooses the significant digits, not the character count.
Artifact serialization under [CSV profile](../storage/csv.md) uses the same canonical numeric text when
no display precision is selected.

<a id="req-0019"></a>

**REQ-0019.** Calculations, comparisons, verifications, and dependent
derivations must use unrounded values. Display precision under [CSV profile](../storage/csv.md) applies
half-away-from-zero rounding once when writing a field. [Project functions](../operations/functions.md) conformance
comparison operates on a temporary copy; neither changes values used by
the specification.

### Types

<a id="req-0420"></a>

**REQ-0420.** `+`, `-`, `*` return `int` for `int` operands. A `float`
operand returns `float`.

<a id="req-0421"></a>

**REQ-0421.** `/` always returns `float`. There is no integer division. Write
`FLOOR(a / b)` for a floor-divided integer.

<a id="req-0422"></a>

**REQ-0422.** `SQRT`, `POWER`, `EXP`, and `LN` return `float`.

<a id="req-0423"></a>

**REQ-0423.** `CEIL`, `FLOOR`, and `TRUNC` return `float`. Declare the column
`type: int` to get an integer. [Execution lifecycle](../execution/lifecycle.md) converts the completed result
and [Types and conversion](types.md) defines that conversion.

<a id="req-0424"></a>

**REQ-0424.** `ABS`, `GREATEST`, `LEAST`, `MOD`, `NULLIF`, and `COALESCE`
return the promoted type of their arguments: `int` when every argument is
`int`, otherwise `float`.

### Overflow and precision

<a id="req-0434"></a>

**REQ-0434.** Integer overflow of `+`, `-`, or `*` under `int` promotion must fail
arithmetic evaluation.

<a id="req-0435"></a>

**REQ-0435.** Floating-point results are not exact decimals. `POWER(x, 2)`
and `x * x` may differ in the last place. A specification
cannot round the difference away. A derivation that needs a stable decimal
must be written as a formula producing a stable decimal.

## Error conditions

<a id="req-0020"></a>

**REQ-0020.** Numeric text outside [REQ-0015](numbers.md#req-0015) must fail conversion through
[REQ-0013](types.md#req-0013). Non-finite spellings explicitly accepted by [REQ-0015](numbers.md#req-0015) normalize
to missing instead of becoming parse failures.

<a id="req-0021"></a>

**REQ-0021.** A non-integral numeric value or a numeric value outside the
signed 64-bit range must fail conversion to `int` through [REQ-0013](types.md#req-0013).
Arithmetic errors remain distinct from conversion failures.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [adam-adsl-non-finite](../../../benchmark/adam-adsl-non-finite/README.md).
- [negative-integer-overflow](../../../benchmark/negative-integer-overflow/README.md).
- [negative-conversion-fractional](../../../benchmark/negative-conversion-fractional/README.md).
- [negative-number-below-limit](../../../benchmark/negative-number-below-limit/README.md).

The [execution manifest](../../../benchmark/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Define numeric representation, promotion, conversion, and overflow. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
