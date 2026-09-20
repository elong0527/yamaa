# Numeric representation and conversion

Status: non-normative draft for issue #606.

## Purpose

Define portable numeric values and their conversion without mixing those
definitions with the arithmetic expression language.

## Scope and dependencies

This first draft covers representation, parsing for conversion, widening,
integral conversion, and numeric text. [Types](types.md) owns the conversion
matrix and non-finite normalization. [R010](../../rules/R010-scalar-computation.md)
still owns arithmetic promotion, operations, overflow, and evaluation order.
Those contracts have not been rewritten in this slice.

The numeric `number` production remains owned by
[grammar/numeric.yaml](../../grammar/numeric.yaml); no second grammar is
defined here. [R020](../../rules/R020-artifact-serialization.md) owns display
precision, while [R018](../../rules/R018-project-function-environment.md)
owns temporary rounding for function conformance comparisons.

## Requirements

**REQ-0014.** `int` is a signed 64-bit integer, from `-2^63` through
`2^63 - 1`. `float` is IEEE 754 binary64. Non-finite values follow REQ-0006
before any consumer can use them.

**REQ-0015.** Parsing text as a number must accept the numeric grammar's
`number` production with an optional leading `+` or `-`, without surrounding
whitespace. It must also recognize exactly `.inf`, `.Inf`, and `.INF` with
an optional leading sign, and `.nan`, `.NaN`, and `.NAN` without a sign.
Recognized non-finite forms parse as floats and immediately become missing.
Every other spelling must fail.

**REQ-0016.** Conversion to `int` must succeed only for an exactly integral
numeric value within the signed 64-bit range. It must not round or truncate.
A specification choosing an integral part must do so explicitly with
`FLOOR`, `CEIL`, or `TRUNC` before column conversion.

**REQ-0017.** Widening `int` to `float` is exact for magnitudes below `2^53`.
Larger magnitudes use the nearest binary64 value.

**REQ-0018.** Converting `int` to `str` uses decimal text. Converting a finite
`float` to `str` must use the shortest sequence of significant decimal digits
that parses back to the same binary64 value, written in positional notation.
An integral float omits the trailing `.0`. The text must not contain an
exponent: shortest chooses the significant digits, not the character count.
Artifact serialization under R020 uses the same canonical numeric text when
no display precision is selected.

**REQ-0019.** Calculations, comparisons, verifications, and dependent
derivations must use unrounded values. Display precision under R020 applies
half-away-from-zero rounding once when writing a field. R018 conformance
comparison operates on a temporary copy; neither changes values used by
the specification.

## Error conditions

**REQ-0020.** Numeric text outside REQ-0015 must fail conversion through
REQ-0013. Non-finite spellings explicitly accepted by REQ-0015 normalize
to missing instead of becoming parse failures.

**REQ-0021.** A non-integral numeric value or a numeric value outside the
signed 64-bit range must fail conversion to `int` through REQ-0013.
Arithmetic errors remain under R010 and must not be reclassified as
conversion failures by this draft.

## Conformance examples

| Input and destination | Expected outcome |
| --- | --- |
| `"1e2"` to `int` | `100` |
| `" 100"` to `float` | conversion failure |
| `"+.INF"` to `float` | missing |
| `"+.nan"` to `float` | conversion failure |
| `72.5` to `int` | conversion failure |
| `100.0` to `str` | `"100"` |
| `0.0001` to `str` | `"0.0001"` |

The examples above illustrate the existing contract; they are not new
executable vectors. Existing executable fixtures include
[fractional conversion](../../../benchmark/negative-conversion-non-integral/README.md),
[invalid numeric text](../../../benchmark/negative-conversion-unparseable-number/README.md),
and [non-finite values](../../../benchmark/adam-adsl-non-finite-values/README.md).
The remaining arithmetic rewrite must audit boundary and rendering coverage
before this block is admitted as normative.

## Rationale

Numeric representation belongs to values because comparisons, ingestion,
functions, and serialization all use it. Arithmetic syntax and operation
semantics can then refer to one shared representation and conversion policy.
