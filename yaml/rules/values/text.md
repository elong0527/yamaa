---
id: values/text
title: Text values
status: normative
---

# Text values

## Purpose

Define Unicode scalar identity, preservation, equality, and order.

## Scope and dependencies

This contract owns runtime Unicode scalar values, identity, and order.
[Types](types.md) owns missingness and comparability. [Text operations](../operations/text.md)
own casing, templates, and patterns; storage profiles own encodings.
[Specification structure](../specification/structure.md) owns the ASCII
source-file boundary.

## Requirements

<a id="req-0022"></a>

**REQ-0022.** A runtime `str` must be a finite sequence of Unicode scalar
values: `U+0000` through `U+D7FF` and `U+E000` through `U+10FFFF`.
Surrogate code points are invalid. Unassigned scalar values are valid and
have no special behavior. Missing is not a string.

<a id="req-0023"></a>

**REQ-0023.** External text must be decoded before entering the language,
using the encoding fixed by its container contract where specified.
Implementations must reject ill-formed encoded values instead of replacing
or skipping them or choosing a machine-default decoding. Repository CSV
fixtures use UTF-8.

<a id="req-0024"></a>

**REQ-0024.** No stage may implicitly apply NFC, NFD, NFKC, NFKD, or any
other normalization. Ingestion, binding, string conversion, concatenation,
interpolation, casing, mapping, grouping, comparison, key validation, and
rendering preserve scalar sequences except for explicitly defined ASCII
casing substitutions. Canonically equivalent sequences remain distinct.
Version 1.0 has no normalization operation; a specification needing
normalized data must receive it from an explicitly governed upstream process.

<a id="req-0025"></a>

**REQ-0025.** Two strings are equal exactly when they contain the same
scalar values in the same positions. Identity comparisons must not perform
casing, folding, normalization, locale tailoring, or compatibility mapping.
This equality applies to predicates, case-sensitive mapping, joins and
record intermediates, group and window partitions, keys, uniqueness checks,
allowed values, and every other comparison of string identities.

<a id="req-0026"></a>

**REQ-0026.** Non-missing strings must order lexicographically by numeric
scalar value. The first unequal position determines the order; when one
sequence is a prefix of another, the shorter is less. Equal sequences tie.
Compare supplementary-plane scalars as single values, never surrogate pairs.
The order must not depend on normalization, case folding, locale, collation,
character names, script properties, encoded bytes, or UTF-16 code units.
Predicates, ordering terms, `greatest`, `least`, and aggregate `MIN` and `MAX`
use this order. Their owning operations define missing placement or empty
results separately.

<a id="req-0027"></a>

**REQ-0027.** R and Python implementations must produce the same scalar
sequences and order for the same inputs. UTF-8 encoding of equal results
must be byte-identical. Fixture tooling must preserve exact bytes while
reading, writing, comparing, and checking fixtures into version control.

<a id="req-0028"></a>

**REQ-0028.** Repository validation checks the ASCII source boundary and
structural validity of Unicode data fixtures. Executable value and ordering
parity must be demonstrated by shared R and Python conformance where those
expressions are supported; static validation alone is not runtime evidence.

## Error conditions

<a id="req-0029"></a>

**REQ-0029.** Ill-formed encoded text or a decoded surrogate code point must
fail with `invalid_text` at its entry boundary.

<a id="req-0030"></a>

**REQ-0030.** An implementation unable to preserve scalar values or apply
this contract must fail before evaluation with `unsupported_text_contract`.
It must not substitute a host default.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [adam-adsl-text](../../../benchmark/adam-adsl-text/README.md).
- [negative-source-bad-encoding](../../../benchmark/negative-source-bad-encoding/README.md).

The [execution manifest](../../../benchmark/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Define Unicode scalar identity, preservation, equality, and order. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
