# Text identity and order

Status: non-normative draft for issue #606.

## Purpose

Define text independently of host locale, encoding order, and Unicode
normalization libraries.

## Scope and dependencies

This contract owns runtime string values, decoding validity, normalization,
equality, order, and parity obligations. [Types](types.md) owns missing values
and comparability between types. A storage profile chooses its encoding;
an operation chooses its missing behavior.

The source-file ASCII restriction remains in
[R019](../../rules/R019-text-values.md) until specification structure is
rewritten. Casing and case-insensitive mapping also remain in R019 until
`operations/text.md` is written. Templates and regular expressions remain
under [R012](../../rules/R012-string-templates.md) and
[R022](../../rules/R022-regular-expressions.md), respectively.

## Requirements

**REQ-0022.** A runtime `str` must be a finite sequence of Unicode scalar
values: `U+0000` through `U+D7FF` and `U+E000` through `U+10FFFF`.
Surrogate code points are invalid. Unassigned scalar values are valid and
have no special behavior. Missing is not a string.

**REQ-0023.** External text must be decoded before entering the language,
using the encoding fixed by its container contract where specified.
Implementations must reject ill-formed encoded values instead of replacing
or skipping them or choosing a machine-default decoding. Repository CSV
fixtures use UTF-8.

**REQ-0024.** No stage may implicitly apply NFC, NFD, NFKC, NFKD, or any
other normalization. Ingestion, binding, string conversion, concatenation,
interpolation, casing, mapping, grouping, comparison, key validation, and
rendering preserve scalar sequences except for explicitly defined ASCII
casing substitutions. Canonically equivalent sequences remain distinct.
Version 1.0 has no normalization operation; a specification needing
normalized data must receive it from an explicitly governed upstream process.

**REQ-0025.** Two strings are equal exactly when they contain the same
scalar values in the same positions. Identity comparisons must not perform
casing, folding, normalization, locale tailoring, or compatibility mapping.
This equality applies to predicates, case-sensitive mapping, joins and
record intermediates, group and window partitions, keys, uniqueness checks,
allowed values, and every other comparison of string identities.

**REQ-0026.** Non-missing strings must order lexicographically by numeric
scalar value. The first unequal position determines the order; when one
sequence is a prefix of another, the shorter is less. Equal sequences tie.
Compare supplementary-plane scalars as single values, never surrogate pairs.
The order must not depend on normalization, case folding, locale, collation,
character names, script properties, encoded bytes, or UTF-16 code units.
Predicates, ordering terms, `greatest`, `least`, and aggregate `MIN` and `MAX`
use this order. Their owning operations define missing placement or empty
results separately.

**REQ-0027.** R and Python implementations must produce the same scalar
sequences and order for the same inputs. UTF-8 encoding of equal results
must be byte-identical. Fixture tooling must preserve exact bytes while
reading, writing, comparing, and checking fixtures into version control.

**REQ-0028.** Repository validation checks the ASCII source boundary and
structural validity of Unicode data fixtures. Executable value and ordering
parity must be demonstrated by shared R and Python conformance where those
expressions are supported; static validation alone is not runtime evidence.

## Error conditions

**REQ-0029.** Ill-formed encoded text or a decoded surrogate code point must
fail with `invalid_text` at its entry boundary.

**REQ-0030.** An implementation unable to preserve scalar values or apply
this contract must fail before evaluation with `unsupported_text_contract`.
It must not substitute a host default.

## Conformance examples

The [portable text fixture](../../../benchmark/adam-adsl-portable-text/README.md)
preserves composed and decomposed spellings as distinct values and exercises
equality, extremes, and scalar order. Its supplementary-plane text
also exercises string operations. The
[invalid source text fixture](../../../benchmark/negative-source-invalid-text/README.md)
checks the decoding failure boundary.

For reviewing the order definition, `U+E000` is less than `U+10000` because
scalar values, not UTF-16 code units, determine order. This illustrative
comparison is not an assertion that the existing fixture covers that pair.

## Rationale

Scalar identity and order require no locale or versioned Unicode database.
Separating these value definitions from casing, patterns, and templates
lets every operation share one text model without sharing operation-specific
matching or missing behavior.
