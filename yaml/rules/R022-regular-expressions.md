---
id: R022
title: Regular Expressions
status: normative
applies_to: [descriptor.pattern, regex, expression.str_extract,
  column_verifications.matches]

---

# Regular expressions

## Intent

Pin one portable regular expression contract. Repository validation, R, and
Python must accept and reject the same patterns and produce the same match for
each subject.

## Boundaries

This rule owns pattern syntax, the portable contract that decides it, the
flag set, and the match semantics of every regular expression in the
language. R006 owns
descriptor structure and where a `pattern` keyword may be declared, R007 owns
expression dispatch, R008 owns the handler lifecycle whose results
`str_extract` returns, R009 owns when a verification runs and how a failure is
reported, and R019 owns the scalar values a pattern is applied to. R004's
`LIKE` is a predicate operator rather than a regular expression and keeps its
own matching.

## Consumers

**R022-1.** The language admits regular expressions in exactly three places:

- the `pattern` descriptor keyword R006 declares on a schema `str`;
- `str_extract.pattern`;
- the `pattern` of R009's `matches` column verification.

**R022-2.** `regex` is the named schema type for `str_extract.pattern` and
R009's `matches` pattern. All requirements below apply to all three except
where a section names one.

## Portable pattern contract

**R022-3.** The normative syntax and semantics are the ECMA-262 `Pattern`
grammar and its matching semantics, with the Unicode flag set. Property
escapes of the form `\\p{...}` are not part of the grammar. Lookbehind whose
length can vary is not part of the grammar. A pattern that uses either is
invalid.

**R022-4.** Python uses the standard library `re` module. The R binding stays
unpinned until the R consumer implements R022. Repository validation uses the
same Python binding. No consumer reads a pattern with a host default that
violates the normalization below.

**R022-5.** The rule text plus the conformance fixtures are the decisive
authority. A pattern is well formed exactly when a conforming consumer accepts
it under the normalization below, and a match is exactly the match such a
consumer reports. The fixtures record the decisive cases. Changing the grammar
or the normalization versions this rule and requires a fixture rerun in every
consumer.

## Flags

**R022-6.** Every pattern uses the Unicode flag `u`. All other flags are
clear. A pattern cannot select flags because ECMA-262 has no inline flag
syntax. `(?i)` is a syntax error. A consumer must not expose `i`, `m`, `s`,
`g`, `y`, `d`, or `v` through a field, environment, or host default.

**R022-7.** The `u` flag makes a pattern operate on the Unicode scalar values
R019 defines. One supplementary-plane scalar is one character to a pattern
and one unit to R006's `min_length` and R009's `max_length`. Without `u`, the
same scalar is two UTF-16 code units and `.` matches half of the scalar.
The flag also admits `\\u{...}` code point escapes and makes a malformed
escape such as `\\a` a syntax error rather than a silent literal.

**R022-8.** Without `i`, matching is case-sensitive. No Unicode case
table applies, because R019 confines casing to ASCII.

**R022-9.** Without `m`, `^` matches only at the start of the subject and `$`
only at its end. `$` does not also match before a trailing `U+000A`.

**R022-10.** Without `s`, `.` matches every scalar except the line
terminators `U+000A`, `U+000D`, `U+2028`, and `U+2029`.

**R022-11.** Without `g` and `y`, a pattern carries no cursor between
evaluations. Each consumer below states its own iteration.

**R022-12.** Without `d` and `v`, no consumer observes match offsets. No
class uses set notation.

**R022-13.** Because `u` is set, `\\d` is exactly `U+0030` through `U+0039`
and `\\w` is exactly those, `A-Z`, `a-z`, and `U+005F`. Neither widens to a
Unicode category. `\\p{...}` is not part of the grammar. A pattern that uses
it is invalid.

## The pattern value

**R022-14.** A pattern value is the ECMA-262 `Pattern` source text alone. It
is not a `/.../flags` literal: a leading or trailing `/` is an ordinary
character to match.

**R022-15.** A pattern is R019 text and obeys R019's source boundary, so a
pattern written in this repository is ASCII. A pattern that must match a
non-ASCII scalar spells it with the `\\u{...}` escape the `u` flag admits, so
`\\u{00E9}` matches `U+00E9` and `\\u{1D400}` matches that one
supplementary-plane scalar.

## Full match and search

**R022-16.** The R006 `pattern` keyword is a **full match**. A value
satisfies the keyword when the match starts before the first scalar and ends
after the last. Implementations evaluate the pattern source wrapped as
`^(?:` and `)$`. The wrapper adds no capturing group or group number.

**R022-17.** `matches` is a **search**. A non-missing value satisfies it when
the pattern matches at any position. A pattern that must describe the whole
value anchors itself.

**R022-18.** `str_extract` is a **search** that keeps one match: the leftmost
one, and where several matches start at the same position, the one ECMA-262
backtracking reaches first.

**R022-19.** A subject is compared as R019 scalar values. Matching applies no
subject normalization, case folding, locale, or collation, so canonically
equivalent subjects that differ in scalars match differently.

## Capture groups

**R022-20.** Capturing groups are numbered from `1` in the order their
opening parentheses appear in the pattern source, counting only capturing
groups; `(?:...)`, lookaround, and a character class contribute no number. A
named group `(?<name>...)` is also numbered, in the same order. Group `0` is
the whole match.

**R022-21.** `str_extract.group` selects by that number and defaults to `0`.
A `group` above the number of capturing groups in its pattern, or a negative
`group`, is a specification defect and fails validation.

**R022-22.** A group that the pattern declares but the match does not enter
has no value. `str_extract` then produces missing. `no_match` does not apply,
because the pattern did match; `no_match` applies only when the pattern
matched nowhere in the subject.

## Empty matches

**R022-23.** An empty match is a match. A pattern that can match the empty
string, such as `a*`, therefore satisfies `matches` for every non-missing
value. `str_extract` returns the empty string rather than missing when
the match it keeps is empty. R019 keeps the empty string and missing
distinct, and no consumer converts one into the other.

**R022-24.** A `pattern` descriptor whose pattern matches only the empty
string admits only the empty value. `min_length`, not the pattern, prohibits
the empty value.

## Determinism and parity

**R022-25.** R and Python implementations must accept the same patterns,
reject the same patterns, and return the same match, group, and verification
outcome for the same pattern and subject.

**R022-26.** `conformance/regex.yaml` holds shared fixtures. Each case names
the pattern, subject, and outcome for all three consumers, or records a
rejected pattern. Repository validation replays the fixtures against the
Python consumer. The replay proves that the Python consumer and fixtures
agree. The shared conformance workflow proves executable parity with R when
that workflow exists. A fixture file alone is not runtime evidence for an R
runtime that has not run the workflow.

## Rationale

Host regular-expression libraries differ in syntax, flags, and match choice,
so any behavior that depends on one host library cannot satisfy the parity
requirement. The portable grammar plus the normalization make the verdict the
contract even where the standard admits more than one reading. The `u` flag
equivalent keeps a pattern on the same scalar values every other text rule
counts, and the remaining flags stay clear so anchoring, dot, case, and
iteration behavior are fixed rather than selectable. Property escapes and
lookbehind of variable length are excluded because consumers cannot implement
them the same way. Repository fixtures replayed against the Python consumer
prove the Python side agrees with them, while executable R parity needs the
shared conformance workflow rather than the fixture file alone.

## Known limitation

The contract no longer requires linear time matching. The Python consumer
uses `re`, which backtracks, so a pathological pattern can take long. The
match it reports is still the match this rule defines.

Outside a character class, the Python consumer normalizes `\S` to the exact
negation of the ECMA-262 whitespace set. Inside a character class, `[\S]`
keeps the host `re` behavior under `re.ASCII`: it excludes only ASCII
whitespace, so it still matches non-ASCII whitespace scalars such as U+00A0
that the contract counts as whitespace. This is a known consumer edge of the
normalization, not a second dialect.

## Errors

- **R022-27.** A pattern the grammar or the normalization rejects, in any of
  the three consumers: fail validation with `invalid_regex` and report the
  declaring path and the rejection. A pattern is rejected the same way
  whether its syntax is malformed or merely outside the portable grammar.
- **R022-28.** A `str_extract.group` that is negative or exceeds the
  capturing groups its pattern declares: fail validation with
  `regex_group_out_of_range` and report the path, the requested group, and
  the count the pattern declares.
- **R022-29.** A consumer that cannot implement the normalization contract:
  fail before evaluation with `unsupported_regex_engine` and report what it
  cannot provide. It must not read patterns with a host default that violates
  the normalization, translate the pattern into another dialect, or skip the
  check.

## Normalization

Each consumer normalizes its host library to the portable semantics below.

**R022-30.** Every consumer guarantees the ASCII meaning of `\\d` and `\\w`,
however its host library behaves. `\\d` is exactly `U+0030` through `U+0039`.
`\\w` is exactly those plus `A` through `Z`, `a` through `z`, and `U+005F`.
A host default that widens either class to a Unicode category stays off.

**R022-31.** Every consumer guarantees `\\s` is exactly the ECMA-262
`WhiteSpace` plus `LineTerminator` set, however its host library behaves. The
set is `U+0009`, `U+000B`, `U+000C`, `U+0020`, `U+00A0`, `U+1680`, `U+2000`
through `U+200A`, `U+202F`, `U+205F`, `U+3000`, `U+FEFF`, `U+000A`, `U+000D`,
`U+2028`, and `U+2029`. `U+0085` is not in the set, even where a host library
includes it.

**R022-32.** Every consumer guarantees `.` matches every scalar except
`U+000A`, `U+000D`, `U+2028`, and `U+2029`, and `$` matches only at the end of
the subject and never before a trailing `U+000A`, however its host library
behaves.

**R022-33.** Every consumer expands each `\\u{...}` escape to the scalar it
names before compiling the pattern.

**R022-34.** The following are syntax errors and fail with `invalid_regex`:
`(?P<name>` named group syntax, inline flag groups such as `(?i)`,
`\\p{...}` property escapes, malformed escapes such as `\\a`, and lookbehind
whose length can vary. Fixed length lookbehind stays allowed, and
`(?<name>...)` stays the way to name a group.
