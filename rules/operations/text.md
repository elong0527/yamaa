---
id: operations/text
title: Text operations
status: normative
---

# Text operations

## Purpose

Apply casing, inline mapping, templates, and portable regular expressions.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Local handlers](../execution/handlers.md).
- [Execution lifecycle](../execution/lifecycle.md).
- [Verification](../execution/verification.md).
- [Expression evaluation](expressions.md).
- [Predicates](predicates.md).
- [Schema language](../reference/schema-language.md).
- [Name binding](../specification/binding.md).
- [Text values](../values/text.md).

## Requirements

### Type behavior

<a id="req-0304"></a>

**REQ-0304.** `mapping` requires a string source because dictionary keys are
strings.

<a id="req-0308"></a>

**REQ-0308.** `str_extract`, `str_concat`, `str_template`, `str_upper`, and
`str_lower` require string sources.

### Templates: Written forms

<a id="req-0446"></a>

**REQ-0446.** `str_template` accepts a bare template as [Schema language](../reference/schema-language.md) shorthand:

```yaml
str_template: "{METSTATR}|{ECOG0R}|{REGIONUSR}"
```

<a id="req-0447"></a>

**REQ-0447.** The canonical form exposes the optional `missing` handler:

```yaml
str_template:
  template: "{SITEID}:{SUBJID}"
  missing: UNKNOWN
```

<a id="req-0448"></a>

**REQ-0448.** The shorthand expands to `{template: <written value>}` and
adds no missing handler.

### Templates: Grammar

<a id="req-0449"></a>

**REQ-0449.** Scan a template from left to right under this closed
grammar:

```text
template    := part*
part        := text | placeholder | "{{" | "}}"
placeholder := "{" variable "}"
text        := one or more R019 scalar values other than "{" and "}"
```

<a id="req-0450"></a>

**REQ-0450.** `grammar/string-template.yaml` is the grammar's only source.
The grammar block above renders `grammar/string-template.yaml`. The file's
cases specify literal text and placeholders each implementation must produce
and templates each implementation must reject.

<a id="req-0451"></a>

**REQ-0451.** Repository validation and the R implementation read
`grammar/string-template.yaml`. Grammar drift fails.

<a id="req-0452"></a>

**REQ-0452.** The contents of `variable` must satisfy that variable's schema
type exactly. Whitespace is therefore not ignored inside braces.

<a id="req-0453"></a>

**REQ-0453.** `{{` emits one literal `{` and `}}` emits one literal `}`.
The brace pairs take precedence while scanning, so `{{{SITEID}}}` produces
`{UCSD}` when `SITEID` is `UCSD`.

<a id="req-0454"></a>

**REQ-0454.** Every brace must begin or end a valid placeholder.
Empty placeholders, unmatched braces, format directives, operators,
function calls, and nested placeholders are invalid. `{A + B}` is invalid
rather than an expression to evaluate.

### Templates: Binding and evaluation

<a id="req-0455"></a>

**REQ-0455.** A placeholder is a variable reference under
[Name binding](../specification/binding.md). A qualified or unqualified
placeholder name follows the `variable` field rules in
[Name binding](../specification/binding.md).

<a id="req-0456"></a>

**REQ-0456.** [Execution lifecycle](../execution/lifecycle.md) collects all placeholders as dependencies before evaluation.
Repeated placeholders add one dependency but are replaced wherever they appear.

<a id="req-0457"></a>

**REQ-0457.** When every dependency is complete, placeholders are replaced by
their string values and brace pairs are unescaped. Values stay unconverted.

<a id="req-0458"></a>

**REQ-0458.** If any value is not a string, evaluation fails under [Types and conversion](../values/types.md).

<a id="req-0459"></a>

**REQ-0459.** If a placeholder value is missing, return declared `missing`.
Without that handler, the missing value is fatal under [Local handlers](../execution/handlers.md).

<a id="req-0460"></a>

**REQ-0460.** Otherwise the result is [Text values](../values/text.md)'s exact concatenation of
literal text and replacement values, including an empty string when the
template itself is empty.

### ASCII casing

<a id="req-0706"></a>

**REQ-0706.** `str_upper` replaces each scalar from `U+0061` through `U+007A`
with the scalar 32 positions earlier, from `U+0041` through `U+005A`.
Every other scalar is unchanged.

<a id="req-0707"></a>

**REQ-0707.** `str_lower` replaces each scalar from `U+0041` through `U+005A`
with the scalar 32 positions later, from `U+0061` through `U+007A`. Every
other scalar is unchanged.

<a id="req-0708"></a>

**REQ-0708.** Both operations preserve scalar count. They have no
one-to-many mapping, context rule, language tailoring, or Unicode-version
dependency. A host uppercase or lowercase routine is conforming only when
its result is exactly the ASCII transformation above for every input.

### Case-insensitive inline mapping

<a id="req-0709"></a>

**REQ-0709.** `mapping.case_sensitive: true` compares its source with
dictionary keys by the exact equality above.

<a id="req-0710"></a>

**REQ-0710.** When `case_sensitive` is `false`, fold the source and every
dictionary key by replacing `U+0061` through `U+007A` with `U+0041` through
`U+005A` and leaving every other scalar unchanged. Compare the folded
sequences by exact equality. Dictionary keys must be unique after this
fold; otherwise validation fails with `ambiguous_dictionary` and reports
the folded key and original entries. A non-ASCII value can match only the
same non-ASCII scalar sequence, apart from ASCII letters that also occur
in that sequence.

### Other text consumers

<a id="req-0711"></a>

**REQ-0711.** Identifier and keyword case behavior comes from the closed
ASCII grammars that own them and is unaffected by data casing. Predicate
`LIKE` remains case-sensitive and compares literal scalar values under
[Predicates](predicates.md). Schema patterns, `str_extract`, and `matches` match under [Text operations](text.md) and
do not select a Unicode casing mode through this contract.

<a id="req-0712"></a>

**REQ-0712.** Version 1.0 has no environment switch for non-ASCII casing.
Adding one requires a versioned contract that pins its Unicode data and
changes the environment, validation, fixtures, and conformance
requirements together. Until then, an implementation must not enable
broader casing from a host or environment default.

### Regular expressions: Consumers

<a id="req-0796"></a>

**REQ-0796.** The language admits regular expressions in exactly three places:

- the `pattern` descriptor keyword [Schema language](../reference/schema-language.md) declares on a schema `str`;
- `str_extract.pattern`;
- the `pattern` of [Verification](../execution/verification.md)'s `matches` column verification.

<a id="req-0797"></a>

**REQ-0797.** `regex` is the named schema type for `str_extract.pattern` and
[Verification](../execution/verification.md)'s `matches` pattern. All requirements below apply to all three except
where a section names one.

### Regular expressions: Portable pattern contract

<a id="req-0798"></a>

**REQ-0798.** The normative syntax and semantics are the ECMA-262 `Pattern`
grammar and its matching semantics, with the Unicode flag set. Property
escapes of the form `\\p{...}` are not part of the grammar. Lookbehind whose
length can vary is not part of the grammar. A pattern that uses either is
invalid.

<a id="req-0799"></a>

**REQ-0799.** Python uses the standard library `re` module. The R binding stays
unpinned until the R consumer implements [Text operations](text.md). Repository validation uses the
same Python binding. No consumer reads a pattern with a host default that
violates the normalization below.

<a id="req-0800"></a>

**REQ-0800.** The rule text plus the conformance fixtures are the decisive
authority. A pattern is well formed exactly when a conforming consumer accepts
it under the normalization below, and a match is exactly the match such a
consumer reports. The fixtures record the decisive cases. Changing the grammar
or the normalization versions this contract and requires a fixture rerun in every
consumer.

### Regular expressions: Flags

<a id="req-0801"></a>

**REQ-0801.** Every pattern uses the Unicode flag `u`. All other flags are
clear. A pattern cannot select flags because ECMA-262 has no inline flag
syntax. `(?i)` is a syntax error. A consumer must not expose `i`, `m`, `s`,
`g`, `y`, `d`, or `v` through a field, environment, or host default.

<a id="req-0802"></a>

**REQ-0802.** The `u` flag makes a pattern operate on the Unicode scalar values
[Text values](../values/text.md) defines. One supplementary-plane scalar is one character to a pattern
and one unit to [Schema language](../reference/schema-language.md)'s `min_length` and [Verification](../execution/verification.md)'s `max_length`. Without `u`, the
same scalar is two UTF-16 code units and `.` matches half of the scalar.
The flag also admits `\\u{...}` code point escapes and makes a malformed
escape such as `\\a` a syntax error rather than a silent literal.

<a id="req-0803"></a>

**REQ-0803.** Without `i`, matching is case-sensitive. No Unicode case
table applies, because [Text values](../values/text.md) confines casing to ASCII.

<a id="req-0804"></a>

**REQ-0804.** Without `m`, `^` matches only at the start of the subject and `$`
only at its end. `$` does not also match before a trailing `U+000A`.

<a id="req-0805"></a>

**REQ-0805.** Without `s`, `.` matches every scalar except the line
terminators `U+000A`, `U+000D`, `U+2028`, and `U+2029`.

<a id="req-0806"></a>

**REQ-0806.** Without `g` and `y`, a pattern carries no cursor between
evaluations. Each consumer below states its own iteration.

<a id="req-0807"></a>

**REQ-0807.** Without `d` and `v`, no consumer observes match offsets. No
class uses set notation.

<a id="req-0808"></a>

**REQ-0808.** Because `u` is set, `\\d` is exactly `U+0030` through `U+0039`
and `\\w` is exactly those, `A-Z`, `a-z`, and `U+005F`. Neither widens to a
Unicode category. `\\p{...}` is not part of the grammar. A pattern that uses
it is invalid.

### Regular expressions: The pattern value

<a id="req-0809"></a>

**REQ-0809.** A pattern value is the ECMA-262 `Pattern` source text alone. It
is not a `/.../flags` literal: a leading or trailing `/` is an ordinary
character to match.

<a id="req-0810"></a>

**REQ-0810.** A pattern is [Text values](../values/text.md) text and obeys [Text values](../values/text.md)'s source boundary, so a
pattern written in this repository is ASCII. A pattern that must match a
non-ASCII scalar spells it with the `\\u{...}` escape the `u` flag admits, so
`\\u{00E9}` matches `U+00E9` and `\\u{1D400}` matches that one
supplementary-plane scalar.

### Regular expressions: Full match and search

<a id="req-0811"></a>

**REQ-0811.** The [Schema language](../reference/schema-language.md) `pattern` keyword is a **full match**. A value
satisfies the keyword when the match starts before the first scalar and ends
after the last. Implementations evaluate the pattern source wrapped as
`^(?:` and `)$`. The wrapper adds no capturing group or group number.

<a id="req-0812"></a>

**REQ-0812.** `matches` is a **search**. A non-missing value satisfies it when
the pattern matches at any position. A pattern that must describe the whole
value anchors itself.

<a id="req-0813"></a>

**REQ-0813.** `str_extract` is a **search** that keeps one match: the leftmost
one, and where several matches start at the same position, the one ECMA-262
backtracking reaches first.

<a id="req-0814"></a>

**REQ-0814.** A subject is compared as [Text values](../values/text.md) scalar values. Matching applies no
subject normalization, case folding, locale, or collation, so canonically
equivalent subjects that differ in scalars match differently.

### Regular expressions: Capture groups

<a id="req-0815"></a>

**REQ-0815.** Capturing groups are numbered from `1` in the order their
opening parentheses appear in the pattern source, counting only capturing
groups; `(?:...)`, lookaround, and a character class contribute no number. A
named group `(?<name>...)` is also numbered, in the same order. Group `0` is
the whole match.

<a id="req-0816"></a>

**REQ-0816.** `str_extract.group` selects by that number and defaults to `0`.
A `group` above the number of capturing groups in its pattern, or a negative
`group`, is a specification defect and fails validation.

<a id="req-0817"></a>

**REQ-0817.** A group that the pattern declares but the match does not enter
has no value. `str_extract` then produces missing. `no_match` does not apply,
because the pattern did match; `no_match` applies only when the pattern
matched nowhere in the subject.

### Regular expressions: Empty matches

<a id="req-0818"></a>

**REQ-0818.** An empty match is a match. A pattern that can match the empty
string, such as `a*`, therefore satisfies `matches` for every non-missing
value. `str_extract` returns the empty string rather than missing when
the match it keeps is empty. [Text values](../values/text.md) keeps the empty string and missing
distinct, and no consumer converts one into the other.

<a id="req-0819"></a>

**REQ-0819.** A `pattern` descriptor whose pattern matches only the empty
string admits only the empty value. `min_length`, not the pattern, prohibits
the empty value.

### Regular expressions: Determinism and parity

<a id="req-0820"></a>

**REQ-0820.** R and Python implementations must accept the same patterns,
reject the same patterns, and return the same match, group, and verification
outcome for the same pattern and subject.

<a id="req-0821"></a>

**REQ-0821.** `conformance/regex.yaml` holds shared fixtures. Each case names
the pattern, subject, and outcome for all three consumers, or records a
rejected pattern. Repository validation replays the fixtures against the
Python consumer. The replay proves that the Python consumer and fixtures
agree. The shared conformance workflow proves executable parity with R when
that workflow exists. A fixture file alone is not runtime evidence for an R
runtime that has not run the workflow.

### Regular expressions: Normalization

<a id="req-0822"></a>

**REQ-0822.** Each consumer normalizes its host library to the portable semantics below.

Every consumer guarantees the ASCII meaning of `\\d` and `\\w`,
however its host library behaves. `\\d` is exactly `U+0030` through `U+0039`.
`\\w` is exactly those plus `A` through `Z`, `a` through `z`, and `U+005F`.
A host default that widens either class to a Unicode category stays off.

<a id="req-0823"></a>

**REQ-0823.** Every consumer guarantees `\\s` is exactly the ECMA-262
`WhiteSpace` plus `LineTerminator` set, however its host library behaves. The
set is `U+0009`, `U+000B`, `U+000C`, `U+0020`, `U+00A0`, `U+1680`, `U+2000`
through `U+200A`, `U+202F`, `U+205F`, `U+3000`, `U+FEFF`, `U+000A`, `U+000D`,
`U+2028`, and `U+2029`. `U+0085` is not in the set, even where a host library
includes it.

<a id="req-0824"></a>

**REQ-0824.** Every consumer guarantees `.` matches every scalar except
`U+000A`, `U+000D`, `U+2028`, and `U+2029`, and `$` matches only at the end of
the subject and never before a trailing `U+000A`, however its host library
behaves.

<a id="req-0825"></a>

**REQ-0825.** Every consumer expands each `\\u{...}` escape to the scalar it
names before compiling the pattern.

<a id="req-0826"></a>

**REQ-0826.** The following are syntax errors and fail with `invalid_regex`:
`(?P<name>` named group syntax, inline flag groups such as `(?i)`,
`\\p{...}` property escapes, malformed escapes such as `\\a`, and lookbehind
whose length can vary. Fixed length lookbehind stays allowed, and
`(?<name>...)` stays the way to name a group.

### Interface behavior

<a id="req-1110"></a>

**REQ-1110.** The `expressions.mapping` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.mapping.source` | String variable used as the dictionary key. |
| `expressions.mapping.dict` | Source-value to result-value dictionary. Exactly one of `dict` and `dict_yaml` is present. |
| `expressions.mapping.dict_yaml` | Path to a YAML file holding the source-value to result-value dictionary. The file is read once during workflow planning through the spec's [project resources](../storage/resources.md); its content must satisfy the `dict` contract. |
| `expressions.mapping.case_sensitive` | Compare exactly when true; use [Text values](../values/text.md) ASCII folding when false. |
| `expressions.mapping.missing` | Value returned when the source is missing or has no dictionary entry. |
| `expressions.mapping.strict` | When true, a missing source or a source with no dictionary entry is an error instead of returning `missing`. Defaults to false. |
| `Result` | Looks up a string source in a dictionary, given inline or loaded from a YAML file. Case-insensitive lookup folds ASCII a-z to A-Z under [Text values](../values/text.md); folded keys must be unique. |

<a id="req-1111"></a>

**REQ-1111.** The `expressions.str_extract` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.str_extract.source` | String variable to search. |
| `expressions.str_extract.pattern` | Regular expression searched in source under [Text operations](text.md). |
| `expressions.str_extract.group` | Match group to return; 0 is the full match. |
| `expressions.str_extract.missing` | Value returned when source is missing. |
| `expressions.str_extract.no_match` | Value returned when pattern does not match source. |
| `Result` | Extracts one regular-expression match group from a string. |

<a id="req-1112"></a>

**REQ-1112.** The `expressions.str_concat` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.str_concat.sources` | Expressions to concatenate in order. |
| `expressions.str_concat.missing` | Value returned when any source is missing. |
| `Result` | Concatenates string expression results in order. |

<a id="req-1113"></a>

**REQ-1113.** The `expressions.str_template` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.str_template` | Portable string template, concise or with missing handling. |
| `Result` | Interpolates string variables under [Text operations](text.md). |

<a id="req-1114"></a>

**REQ-1114.** The `expressions.str_upper` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.str_upper.source` | String variable whose ASCII letters are uppercased under [Text values](../values/text.md). |
| `expressions.str_upper.missing` | Value returned when source is missing. |
| `Result` | Converts ASCII a-z to A-Z and preserves every other scalar. |

<a id="req-1115"></a>

**REQ-1115.** The `expressions.str_lower` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.str_lower.source` | String variable whose ASCII letters are lowercased under [Text values](../values/text.md). |
| `expressions.str_lower.missing` | Value returned when source is missing. |
| `Result` | Converts ASCII A-Z to a-z and preserves every other scalar. |

<a id="req-1240"></a>

**REQ-1240.** The `expressions.str_sentence` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.str_sentence.source` | String variable converted to sentence case under [Text values](../values/text.md). |
| `expressions.str_sentence.missing` | Value returned when source is missing. |
| `Result` | Uppercases the first scalar and lowercases every later scalar, ASCII-only: the first scalar gets the REQ-0708 upward substitution and every later scalar gets the downward substitution. Non-ASCII scalars pass through unchanged, so the scalar count is preserved. A host `capitalize` routine must not be used: host Unicode behavior can expand or alter non-ASCII scalars (e.g. U+00DF or U+0130). |

<a id="req-1241"></a>

**REQ-1241.** The `expressions.str_title` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.str_title.source` | String variable converted to title case under [Text values](../values/text.md). |
| `expressions.str_title.missing` | Value returned when source is missing. |
| `Result` | Title-cases each maximal run of ASCII letters `[A-Za-z]+`: the first letter of the run is uppercased and the remaining letters of the run are lowercased, both via the REQ-0708 ASCII substitutions. Every other scalar -- including non-ASCII letters -- passes through unchanged, so the scalar count is preserved. Word detection is ASCII-only: no locale, no Unicode word-break rule, and no Unicode-version dependency. |

<a id="req-1116"></a>

**REQ-1116.** The `string_template` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `string_template` | Template string defined by [Text operations](text.md). |

<a id="req-1117"></a>

**REQ-1117.** The `str_template_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `str_template_class.template` | Text containing literal content and braced variables. |
| `str_template_class.missing` | Value returned when any placeholder value is missing. |

## Error conditions

<a id="req-0336"></a>

**REQ-0336.** A `str_template` expression that violates [Text operations](text.md): fail.

### Templates: Errors

<a id="req-0461"></a>

**REQ-0461.** A template that does not parse under the grammar: fail
validation and report its specification path and the invalid placeholder
or unmatched brace.

<a id="req-0462"></a>

**REQ-0462.** A placeholder that does not bind: fail under [Execution lifecycle](../execution/lifecycle.md) and [Name binding](../specification/binding.md).

<a id="req-0463"></a>

**REQ-0463.** A non-string placeholder value: fail under [Types and conversion](../values/types.md).

<a id="req-0464"></a>

**REQ-0464.** A missing placeholder value without `missing`: fail under
[Local handlers](../execution/handlers.md).

### Errors

<a id="req-0714"></a>

**REQ-0714.** Dictionary keys that collide after the ASCII fold: fail
validation with `ambiguous_dictionary`.

### Regular expressions: Errors

<a id="req-0827"></a>

**REQ-0827.** A pattern the grammar or the normalization rejects, in any of
  the three consumers: fail validation with `invalid_regex` and report the
  declaring path and the rejection. A pattern is rejected the same way
  whether its syntax is malformed or merely outside the portable grammar.

<a id="req-0828"></a>

**REQ-0828.** A `str_extract.group` that is negative or exceeds the
  capturing groups its pattern declares: fail validation with
  `regex_group_out_of_range` and report the path, the requested group, and
  the count the pattern declares.

<a id="req-0829"></a>

**REQ-0829.** A consumer that cannot implement the normalization contract:
  fail before evaluation with `unsupported_regex_engine` and report what it
  cannot provide. It must not read patterns with a host default that violates
  the normalization, translate the pattern into another dialect, or skip the
  check.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [adam-adsl-text](../../benchmarks/schema-text-functions/README.md).
- [negative-subject-reference](../../benchmarks/negative-subject-reference/README.md).
- [negative-mapping-case-collision](../../benchmarks/negative-mapping-case-collision/README.md).
- [negative-matches-bad-pattern](../../benchmarks/negative-matches-bad-pattern/README.md).
- [negative-str-uncaptured-group](../../benchmarks/negative-str-uncaptured-group/README.md).

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

This contract covers casing, mapping, templates, and regular expressions.

### Current implementation limits

Linear-time matching is not required. Python's backtracking `re` can make
pathological patterns slow. `re` matches still follow this contract.

Outside a character class, the Python consumer normalizes `\S` to the exact
negation of the ECMA-262 whitespace set. Inside a character class, `[\S]`
keeps the host `re` behavior under `re.ASCII`: it excludes only ASCII
whitespace, so it still matches non-ASCII whitespace scalars such as U+00A0
that the contract counts as whitespace. This is a known consumer edge of the
normalization, not a second dialect.
