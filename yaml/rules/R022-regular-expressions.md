---
id: R022
title: Regular Expressions
status: normative
applies_to: [descriptor.pattern, regex, expression.str_extract,
  column_verifications.matches]
depends_on: [R006, R007, R008, R009, R019]
---

# Regular expressions

## Intent

Pin one executable regular-expression contract so that repository validation,
R, and Python accept the same patterns, reject the same patterns, and produce
the same match for the same subject.

## Boundaries

This rule owns pattern syntax, the engine that decides it, the flag set, and
the match semantics of every regular expression in the language. R006 owns
descriptor structure and where a `pattern` keyword may be declared, R007 owns
expression dispatch, R008 owns the handler lifecycle whose results
`str_extract` returns, R009 owns when a verification runs and how a failure is
reported, and R019 owns the scalar values a pattern is applied to. R004's
`LIKE` is a predicate operator rather than a regular expression and keeps its
own matching.

## Consumers

The language admits regular expressions in exactly three places:

- the `pattern` descriptor keyword R006 declares on a schema `str`;
- `str_extract.pattern`;
- the `pattern` of R009's `matches` column verification.

The named schema type `regex` is the type of the last two. Every requirement
below applies to all three unless a section names one.

## Pinned engine

The normative syntax and semantics are the ECMA-262 `Pattern` grammar and its
matching semantics, evaluated with the Unicode flag set.

The pinned engine is the Rust `regress` crate, version `0.10.4`. A Python
consumer binds that crate version through the `regress` distribution
`2025.10.1` on PyPI. An R consumer binds the same crate version. Repository
validation uses the same binding as any other Python consumer, so no
implementation reads a pattern with a host engine such as Python `re`, POSIX
ERE, PCRE, or TRE.

The pinned engine is the decisive authority. A pattern is well formed exactly
when the pinned engine compiles it under the flag set below, and a match is
exactly the match that engine reports. Where ECMA-262 admits more than one
reading, or where a construct is newer than the pinned version, the engine
decides and its rejection is the contract rather than a defect to work around.
Changing the pin is a versioned change to this rule that re-runs the fixtures
in every consumer.

## Flags

Every pattern is evaluated with the Unicode flag `u` set and every other flag
clear. A pattern does not select its own flags: ECMA-262 has no inline flag
syntax, so `(?i)` is a syntax error and stays one, and a consumer must not
expose `i`, `m`, `s`, `g`, `y`, `d`, or `v` through a field, an environment, or
a host default.

`u` is set because it makes a pattern operate on the Unicode scalar values
R019 defines. One supplementary-plane scalar is one character to a pattern,
just as it is one unit to R006's `min_length` and R009's `max_length`; without
`u` the same scalar would be two UTF-16 code units and `.` would match half of
it. `u` also admits the `\u{...}` code point escape and makes a malformed
escape such as `\a` a syntax error rather than a silent literal.

The other flags stay clear so that each remaining behavior is fixed rather
than selectable:

- without `i`, matching is case-sensitive, and no Unicode case table enters a
  language whose casing R019 confines to ASCII;
- without `m`, `^` matches only at the start of the subject and `$` only at its
  end, so `$` does not also match before a trailing `U+000A`;
- without `s`, `.` matches every scalar except the line terminators `U+000A`,
  `U+000D`, `U+2028`, and `U+2029`;
- without `g` and `y`, a pattern carries no cursor between evaluations, and
  each consumer below states its own iteration;
- without `d` and `v`, no consumer observes match offsets and no class uses
  set notation.

Because `u` is set, `\d` is exactly `U+0030` through `U+0039` and `\w` is
exactly those, `A-Z`, `a-z`, and `U+005F`. Neither widens to a Unicode
category. `\p{...}` selects a Unicode property explicitly and is the way to
ask for one.

## The pattern value

A pattern value is the ECMA-262 `Pattern` source text alone. It is not a
`/.../flags` literal: a leading or trailing `/` is an ordinary character to
match.

A pattern is R019 text and obeys R019's source boundary, so a pattern written
in this repository is ASCII. A pattern that must match a non-ASCII scalar
spells it with the `\u{...}` escape the `u` flag admits, so `\u{00E9}` matches
`U+00E9` and `\u{1D400}` matches that one supplementary-plane scalar.

## Full match and search

Each consumer fixes whether the pattern must consume the whole subject:

- The R006 `pattern` keyword is a **full match**. The value satisfies it when
  the match starts before the first scalar and ends after the last.
  Implementations obtain this by evaluating the pattern source wrapped as
  `^(?:` and `)$`, which adds no capturing group and renumbers none.
- `matches` is a **search**. A non-missing value satisfies it when the pattern
  matches at any position. A pattern that must describe the whole value
  anchors itself.
- `str_extract` is a **search** that keeps one match: the leftmost one, and
  where several matches start at the same position, the one ECMA-262
  backtracking reaches first.

A subject is compared as R019 scalar values. Matching applies no
normalization, case folding, locale, or collation, so canonically equivalent
subjects that differ in scalars match differently.

## Capture groups

Capturing groups are numbered from `1` in the order their opening parentheses
appear in the pattern source, counting only capturing groups; `(?:...)`,
lookaround, and a character class contribute no number. A named group
`(?<name>...)` is also numbered, in the same order. Group `0` is the whole
match.

`str_extract.group` selects by that number and defaults to `0`. A `group`
greater than the number of capturing groups in its pattern is a specification
defect and fails validation; a negative `group` fails the same way.

A group that the pattern declares but the match does not enter has no value.
`str_extract` then produces missing. `no_match` does not apply, because the
pattern did match; `no_match` applies only when the pattern matched nowhere in
the subject.

## Empty matches

An empty match is a match. A pattern that can match the empty string, such as
`a*`, therefore satisfies `matches` for every non-missing value, and
`str_extract` returns the empty string rather than missing when the match it
keeps is empty. R019 keeps the empty string and missing distinct, and no
consumer converts one into the other.

A `pattern` descriptor whose pattern matches only the empty string admits only
the empty value, which `min_length` rather than the pattern is the way to
prohibit.

## Determinism and parity

R and Python implementations must accept the same patterns, reject the same
patterns, and return the same match, group, and verification outcome for the
same pattern and subject.

`conformance/regex.yaml` holds the shared fixtures. Every case names the
pattern, the subject, and the outcome for all three consumers, or records that
the pattern is rejected. Repository validation replays the fixtures against
the pinned engine, which proves the Python side and the fixtures agree.
Executable parity with R is proved by the shared conformance workflow when
that workflow exists; a fixture file alone is not runtime evidence for a
runtime that has not run it.

## Errors

- A pattern the pinned engine rejects, in any of the three consumers: fail
  validation with `invalid_regex` and report the declaring path and the
  engine's rejection. A pattern is rejected the same way whether its syntax is
  malformed or merely outside the pinned version.
- A `str_extract.group` that is negative or exceeds the capturing groups its
  pattern declares: fail validation with `regex_group_out_of_range` and report
  the path, the requested group, and the count the pattern declares.
- An implementation that cannot provide the pinned engine: fail before
  evaluation with `unsupported_regex_engine` and report the engine and version
  it expected. It must not fall back to a host regular-expression library,
  translate the pattern into another dialect, or skip the check.
