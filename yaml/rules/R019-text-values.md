---
id: R019
title: Text Values
status: normative
applies_to: [str, expression.str_upper, expression.str_lower,
  expression.mapping, order_by_term]
depends_on: []
---

# Text values

## Intent

Define one portable text model for source notation, runtime strings, casing,
equality, and total order without consulting a host locale or Unicode library.

## Boundaries

This rule owns the language-wide meaning of a string. R002 owns how a string
source binds, R004 owns predicate grammar and `LIKE`, R006 owns schema
structure, R007 owns expression dispatch and order terms, R008 owns missing
handlers, R012 owns template grammar, R013 owns aggregate grammar, and R014
owns source-format ingestion. Regular expressions retain the common
ECMAScript contract of their owning operations; this rule does not reinterpret
their pattern syntax or matching units.

## Source and data boundary

Language source is ASCII. Schema documents, specifications, inheritance
layers, project environments, conformance documents, rules, documentation,
and implementation source contain only bytes from `0x00` through `0x7F`.
Names and examples in source use forms such as `U+00E9` rather than embedding
the character. A format's ASCII escape notation may denote a non-ASCII value;
the decoded value is then an ordinary string under this rule.

Data may contain Unicode. A runtime `str` is a finite sequence of Unicode
scalar values: code points `U+0000` through `U+D7FF` and `U+E000` through
`U+10FFFF`. Surrogate code points are not scalar values and cannot occur.
Unassigned scalar values are valid and have no special behavior.

An external encoding is decoded before its values enter the language. An
ill-formed encoded value fails rather than being replaced, skipped, or decoded
under a machine default. A container contract may fix an encoding; the CSV
fixtures in this repository are UTF-8. Missing is not a string and continues
to follow the missing-value rules of each consumer.

## No implicit normalization

No stage implicitly applies NFC, NFD, NFKC, NFKD, or any other normalization.
Ingestion, source binding, string conversion, concatenation, interpolation,
casing, mapping, grouping, comparison, key validation, and artifact rendering
preserve the scalar sequence except for the explicit ASCII substitutions below.

Canonically equivalent sequences therefore remain distinct values. A
specification that needs normalized source data must receive it from an
explicitly governed upstream process; version 1.0 has no normalization
operation.

## Equality

Two strings are equal exactly when they contain the same scalar values in the
same positions. Equality performs no casing, folding, normalization, locale
tailoring, or compatibility mapping.

This equality is used everywhere the language compares identities: predicate
equality, case-sensitive inline mapping, dataset joins and record lookups,
group and window partitions, key and uniqueness checks, allowed values, and
any other operation that asks whether two `str` values are equal.

## Total order

Non-missing strings are ordered lexicographically by scalar value. Compare the
numeric code point at each position from left to right; the first unequal
position decides the result. If one sequence is a prefix of the other, the
shorter sequence is less. Equal sequences tie.

This is the one string order used by predicate comparisons, every
`order_by` term, `greatest`, `least`, and aggregate `MIN` and `MAX`. Missing
placement is not part of string order; the operation that admits missing
values owns that placement or empty-result behavior.

The order performs no normalization or case folding and uses no locale,
collator, character name, script property, encoded byte order, or UTF-16 code
unit order. In particular, an implementation must compare a supplementary-
plane scalar as one value rather than as a surrogate pair.

## ASCII casing

`str_upper` replaces each scalar from `U+0061` through `U+007A` with the scalar
32 positions earlier, from `U+0041` through `U+005A`. Every other scalar is
unchanged.

`str_lower` replaces each scalar from `U+0041` through `U+005A` with the scalar
32 positions later, from `U+0061` through `U+007A`. Every other scalar is
unchanged.

Both operations preserve scalar count. They have no one-to-many mapping,
context rule, language tailoring, or Unicode-version dependency. A host
uppercase or lowercase routine is conforming only when its result is exactly
the ASCII transformation above for every input.

## Case-insensitive inline mapping

`mapping.case_sensitive: true` compares its source with dictionary keys by the
exact equality above.

When `case_sensitive` is `false`, fold the source and every dictionary key by
replacing `U+0061` through `U+007A` with `U+0041` through `U+005A` and leaving
every other scalar unchanged. Compare the folded sequences by exact equality.
Dictionary keys must be unique after this fold; otherwise validation fails
with `ambiguous_dictionary` and reports the folded key and original entries.

This fold is deliberately not Unicode case folding. A non-ASCII value can
match only the same non-ASCII scalar sequence, apart from ASCII letters that
also occur in that sequence.

## Other text consumers

Identifier and keyword case behavior comes from the closed ASCII grammars that
own them and is unaffected by data casing. Predicate `LIKE` remains
case-sensitive and compares literal scalar values under R004. Schema patterns,
`str_extract`, and `matches` retain their common ECMAScript behavior and do
not select a Unicode casing mode through this rule.

Version 1.0 has no environment switch for non-ASCII casing. Adding one requires
a versioned contract that pins its Unicode data and changes the environment,
validation, fixtures, and conformance requirements together. Until then, an
implementation must not enable broader casing from a host or environment
default.

## Determinism and parity

R and Python implementations must return the same scalar sequences and order
for the same inputs. Encoding equal results as UTF-8 produces byte-identical
text. Fixtures must retain their exact bytes; tooling must not normalize them
while reading, writing, comparing, or checking them into version control.

Repository validation proves the ASCII source boundary and the structural
validity of the Unicode data fixtures. Executable value and ordering parity is
proved by the shared R and Python conformance workflow when that workflow
supports these expressions; static validation alone is not runtime evidence.

## Errors

- A non-ASCII byte in language or repository source: fail validation with
  `non_ascii_source` and report the file and position.
- Ill-formed encoded text or a decoded surrogate code point: fail with
  `invalid_text` at the boundary where it enters.
- Dictionary keys that collide after the ASCII fold: fail validation with
  `ambiguous_dictionary`.
- An implementation that cannot preserve scalar values or apply this exact
  contract: fail before evaluation with `unsupported_text_contract`; it must
  not substitute a host default.
