# Text

The text verbs pad, change case, interpolate templates, and extract and
replace by pattern. They operate on the string values of
[VALUE-11](values.md#value-11) and preserve every scalar they do not change.
One portable regular-expression language serves every pattern in a
specification.

## String verbs

<a id="text-1"></a>

### TEXT-1. Text verbs take string sources and fail rather than convert another type

`str_extract`, `str_replace`, `str_template` and `str_case` require string
sources; a source of another runtime type fails rather than being converted.

Fails: `incompatible_input_type`.
Evidence: `negative-str-case-number`, `rust/crates/yamaa-core/tests/support/diagnostic/text_case.rs`.
Replaces: REQ-0308.

A source of another type fails under [VALUE-2](values.md#value-2).

<a id="text-2"></a>

### TEXT-2. str_pad left-pads a value's canonical text with ASCII spaces to a minimum width

`str_pad` takes `source` and a positive integer `width`. It converts a present
scalar source to its canonical text under [VALUE-3](values.md#value-3), then
adds ASCII spaces to the left until the text has at least `width` characters;
it does not truncate longer text. A missing source yields missing. A width
below one fails as `invalid_field_type`.

Fails: `invalid_field_type`.
Evidence: `negative-str-pad-width`, `adam-adlb-supplb-padded-key`, `rust/crates/yamaa-adapters/tests/fixtures/text_pad_reference.json`.
Replaces: REQ-1261.

An unknown source or a failed conversion reports its existing condition.

## String templates

<a id="text-3"></a>

### TEXT-3. A string template is literal text, braced placeholders and doubled braces, nothing else

A template is scanned from left to right under the closed grammar below. `{{`
emits one literal `{` and `}}` one literal `}`, and the brace pairs take
precedence while scanning. A placeholder's contents must satisfy its
variable's schema type exactly, so whitespace inside braces is not ignored.
Every other brace must begin or end a valid placeholder; a template that does
not parse fails validation with `invalid_string_template`.

Fails: `invalid_string_template`.
Evidence: `negative-subject-reference`, `yaml/grammar/string-template.yaml`, `rust/crates/yamaa-adapters/tests/fixtures/text_template_reference.json`.
Replaces: REQ-0336, REQ-0449, REQ-0450, REQ-0451, REQ-0452, REQ-0453, REQ-0454, REQ-0461.

<!-- grammar: string-template -->

```text
template    := part*
part        := text | placeholder | "{{" | "}}"
placeholder := "{" variable "}"
text        := one or more Unicode scalar values other than "{" and "}"
```

`yaml/grammar/string-template.yaml` is the grammar's only source, and the
block above renders it. Its cases specify the literal text and placeholders a
template produces and the templates that are rejected.

`{{{SITEID}}}` produces `{UCSD}` when `SITEID` is `UCSD`. Empty placeholders,
unmatched braces, format directives, operators, function calls and nested
placeholders are invalid: `{A + B}` is invalid rather than an expression to
evaluate. The failure reports the template's specification path and the
invalid placeholder or unmatched brace. Any other `str_template` expression
that violates these rules also fails.

<a id="text-4"></a>

### TEXT-4. str_template replaces each placeholder with its string value, or answers a missing one

A placeholder is a variable reference, collected as a dependency before
evaluation; a repeated placeholder is one dependency, replaced wherever it
appears. When every dependency is complete, placeholders are replaced by their
string values, unconverted, and brace pairs are unescaped. A value that is not
a string fails. A missing value returns the declared `missing` value. Otherwise
the result is the exact concatenation of literal text and values, the empty
string when the template is empty.

Fails: `incompatible_input_type`.
Evidence: `schema-shorthand`, `sdtm-dm-race-ethnicity`, `schema-expressions`, `rust/crates/yamaa-adapters/tests/fixtures/text_template_reference.json`.
Replaces: REQ-0446, REQ-0447, REQ-0448, REQ-0455, REQ-0456, REQ-0457, REQ-0458, REQ-0459, REQ-0460, REQ-0462, REQ-1112, REQ-1113, REQ-1116, REQ-1117.

`str_template` accepts a bare template as
[Schema language](reference/schema-language.md) shorthand. The shorthand
expands to `{template: <written value>}` and adds no missing handler:

```yaml
str_template: "{METSTATR}|{ECOG0R}|{REGIONUSR}"
```

The canonical form exposes the optional `missing` handler:

```yaml
str_template:
  template: "{SITEID}:{SUBJID}"
  missing: UNKNOWN
```

A qualified or unqualified placeholder name follows the `variable` field rules
of [Name binding](names.md), and the
[Column derivation](columns.md) collects the placeholders as
dependencies. A placeholder that does not bind fails under the
[Column derivation](columns.md) and
[Name binding](names.md). A value that is not a string fails
under [VALUE-2](values.md#value-2). `missing` answers when any placeholder
value is missing; without it, the missing value is fatal under
[Column derivation](columns.md). The concatenation is exact under
[VALUE-12](values.md#value-12).

Text joined from several parts is written as one template: literal text, with
braces written `{{` and `}}`, sits beside the placeholders, and a value that
another expression computes is bound to a column first and then named as a
placeholder.

## Case

<a id="text-5"></a>

### TEXT-5. str_case changes the case of ASCII letters only, one scalar for one

`str_case` with `to: upper` replaces each of `U+0061` through `U+007A` with the
scalar 32 positions earlier, and `to: lower` replaces each of `U+0041` through
`U+005A` with the scalar 32 positions later. `sentence` applies the upward
substitution to the first scalar and the downward one to every later scalar;
`title` applies the upward one to the first letter of each maximal run of ASCII
letters and the downward one to the rest of the run. Every other scalar is
unchanged.

Evidence: `schema-sentence-case`, `schema-text-functions`, `rust/crates/yamaa-adapters/tests/fixtures/text_case_reference.json`.
Replaces: REQ-0706, REQ-0707, REQ-0708, REQ-0711, REQ-0712, REQ-1114, REQ-1240, REQ-1241.

Every substitution preserves scalar count. None has a one-to-many mapping, a
context rule, language tailoring or a Unicode-version dependency, and word
detection for `title` is ASCII-only: a run is `[A-Za-z]+`, with no locale and
no Unicode word-break rule. Non-ASCII scalars, including non-ASCII letters such
as `U+00DF` and `U+0130`, pass through unchanged. `missing` is the value
returned when the source is missing.

Identifier and keyword case behavior comes from the closed ASCII grammars that
own them and is unaffected by data casing. Predicate `LIKE` remains
case-sensitive and compares literal scalar values under
[Formulas](formulas.md). Schema patterns, `str_extract`,
`str_replace` and `matches` match under [TEXT-6](#text-6) to
[TEXT-12](#text-12) and select no Unicode casing mode through this rule.

There is no environment switch for non-ASCII casing, and an implementation
must not enable broader casing from a host or environment default.

## Regular expressions

<a id="text-6"></a>

### TEXT-6. Every pattern is an ECMA-262 Pattern read with the Unicode flag, minus a few constructs

A regular expression is admitted in a schema `pattern` descriptor,
`str_extract.pattern`, `str_replace.pattern`, a `matches` verification and the
`str_contains` predicate call. Each is ECMA-262 `Pattern` source text with its
matching semantics and the Unicode flag set. Property escapes `\p{...}`,
variable-length lookbehind, `(?P<name>` groups, inline flag groups such as
`(?i)` and malformed escapes such as `\a` are invalid. An invalid pattern fails
validation with `invalid_regex`, or with `invalid_predicate` in `str_contains`.

Fails: `invalid_regex`, `invalid_predicate`.
Evidence: `negative-matches-bad-pattern`, `negative-str-replace-invalid-regex`, `negative-str-contains-invalid-regex`.
Replaces: REQ-0796, REQ-0797, REQ-0798, REQ-0799, REQ-0800, REQ-0809, REQ-0820, REQ-0821, REQ-0826, REQ-0827, REQ-0829.

The consumers are:

- the `pattern` descriptor keyword
  [Schema language](reference/schema-language.md) declares on a schema `str`;
- `str_extract.pattern`;
- `str_replace.pattern` ([TEXT-11](#text-11));
- the `pattern` of [Verification](verification.md)'s `matches`
  column verification;
- the pattern literal in the predicate call `str_contains(source, pattern)`
  ([FORMULA-8](formulas.md#formula-8)).

`regex` is the named schema type of `str_extract.pattern`,
`str_replace.pattern` and the `matches` pattern. The schema descriptor and the
predicate pattern literal follow the same portable pattern rules, and every
rule below applies to every consumer except where it names one.

A pattern value is the ECMA-262 `Pattern` source text alone. It is not a
`/.../flags` literal: a leading or trailing `/` is an ordinary character to
match. Fixed-length lookbehind stays allowed, and `(?<name>...)` stays the way
to name a group.

Every consumer reads patterns through the one shared portable matcher; no host
regex library or host default defines what a pattern means, and no consumer
translates a pattern into another dialect or skips the check. Every consumer
therefore accepts and rejects the same patterns and returns the same match,
group and verification outcome for the same pattern and subject. A pattern is
well formed exactly when that matcher accepts it under the flags and classes of
[TEXT-7](#text-7) and [TEXT-8](#text-8), and a match is exactly the match it
reports.

These rules and `yaml/conformance/regex.yaml` are the decisive authority. Each
fixture case names a pattern and either records that it is rejected or gives a
subject with its full-match, search and extraction outcomes, and optionally a
replacement and its outcome; the search outcome also governs `str_contains`.
Changing the grammar or the classes versions this contract and its fixtures.

A pattern the grammar or the classes reject, in a schema descriptor,
`str_extract`, `str_replace` or a `matches` verification, fails validation with
`invalid_regex` and reports the declaring path and the rejection. The
`str_contains` predicate call instead fails with `invalid_predicate` at parse
time under [FORMULA-8](formulas.md#formula-8). Each consumer
rejects a pattern with its stated condition whether its syntax is malformed or
merely outside the portable grammar.

<a id="text-7"></a>

### TEXT-7. A pattern carries the Unicode flag only: case-sensitive, anchored to the whole subject, stateless

Every pattern uses the Unicode flag `u`, all other flags are clear, and a
pattern cannot select flags; no field, environment or host default exposes
`i`, `m`, `s`, `g`, `y`, `d` or `v`. Matching is case-sensitive with no Unicode
case table. `^` matches only at the start of the subject and `$` only at its
end, never before a trailing `U+000A`. `.` matches every scalar except
`U+000A`, `U+000D`, `U+2028` and `U+2029`. A pattern keeps no cursor between
evaluations, and no consumer observes match offsets.

Evidence: `yaml/conformance/regex.yaml`, `rust/crates/yamaa-adapters/tests/declaration_contract_vectors.rs::unchanged_regex_documents_and_authored_capture_controls_run_directly`.
Replaces: REQ-0801, REQ-0802, REQ-0803, REQ-0804, REQ-0805, REQ-0806, REQ-0807, REQ-0824.

ECMA-262 has no inline flag syntax, so `(?i)` is a syntax error. The `u` flag
makes a pattern operate on the Unicode scalar values of
[VALUE-11](values.md#value-11): one supplementary-plane scalar is one character
to a pattern and one unit to [Schema language](reference/schema-language.md)'s
`min_length` and [Verification](verification.md)'s `max_length`. The
flag also admits `\u{...}` code point escapes and makes a malformed escape such
as `\a` a syntax error rather than a silent literal. Casing is confined to
ASCII under [TEXT-5](#text-5). Each consumer states its own iteration
([TEXT-9](#text-9), [TEXT-11](#text-11)), and no character class uses set
notation.

<a id="text-8"></a>

### TEXT-8. Character classes are fixed ASCII and ECMA-262 sets, and a non-ASCII scalar is an escape

`\d` is exactly `U+0030` through `U+0039`, and `\w` is exactly those, `A`
through `Z`, `a` through `z` and `U+005F`; neither widens to a Unicode
category. `\s` is exactly the ECMA-262 `WhiteSpace` plus `LineTerminator` set,
without `U+0085`. Every `\u{...}` escape expands to the scalar it names before
the pattern compiles. A pattern is ASCII language source, so it matches a
non-ASCII scalar by spelling it with that escape.

Evidence: `yaml/conformance/regex.yaml`, `rust/crates/yamaa-adapters/tests/fixtures/regex_contract.tsv`.
Replaces: REQ-0808, REQ-0810, REQ-0822, REQ-0823, REQ-0825.

The `\s` set is `U+0009`, `U+000B`, `U+000C`, `U+0020`, `U+00A0`, `U+1680`,
`U+2000` through `U+200A`, `U+202F`, `U+205F`, `U+3000`, `U+FEFF`, `U+000A`,
`U+000D`, `U+2028` and `U+2029`. These sets hold whatever any host library
would do; a default that widens a class to a Unicode category stays off.

A pattern is text under [VALUE-11](values.md#value-11) and obeys the ASCII
source boundary of [SPEC-8](specification.md#spec-8), so
`\u{00E9}` matches `U+00E9` and `\u{1D400}` matches that one supplementary-plane
scalar. `\p{...}` is not part of the grammar ([TEXT-6](#text-6)).

<a id="text-9"></a>

### TEXT-9. A schema pattern is a full match, while matches and str_extract search the subject

The schema `pattern` keyword is a full match: a value satisfies it when the
match starts before the first scalar and ends after the last, as if the source
were wrapped in `^(?:` and `)$`. `matches` is a search: a non-missing value
satisfies it when the pattern matches at any position. `str_extract` is a
search that keeps the leftmost match and, among matches starting there, the one
ECMA-262 backtracking reaches first. An empty match is a match.

Evidence: `yaml/conformance/regex.yaml`, `adam-adsl-demographics`, `schema-str-contains`.
Replaces: REQ-0811, REQ-0812, REQ-0813, REQ-0814, REQ-0818, REQ-0819.

The full-match wrapper adds no capturing group or group number. A `matches`
pattern that must describe the whole value anchors itself.

A subject is compared as the scalar values of [VALUE-11](values.md#value-11).
Matching applies no subject normalization, case folding, locale or collation,
so canonically equivalent subjects that differ in scalars match differently.

A pattern that can match the empty string, such as `a*`, therefore satisfies
`matches` for every non-missing value, and `str_extract` returns the empty
string rather than missing when the match it keeps is empty. The empty string
and missing stay distinct, and no consumer converts one into the other. A
`pattern` descriptor whose pattern matches only the empty string admits only
the empty value; `min_length`, not the pattern, prohibits the empty value.

<a id="text-10"></a>

### TEXT-10. Capture groups are numbered by opening parenthesis, and str_extract returns one group

Capturing groups are numbered from `1` in the order their opening parentheses
appear in the pattern source, counting only capturing groups, named groups
included; group `0` is the whole match. `str_extract.group` selects by that
number and defaults to `0`; a negative group, or one above the pattern's number
of capturing groups, fails validation with `regex_group_out_of_range`. A group
the match does not enter yields missing. A source the pattern matches nowhere
returns the declared `no_match` value.

Fails: `regex_group_out_of_range`.
Evidence: `negative-str-uncaptured-group`, `adam-adsl-site-parse`, `rust/crates/yamaa-adapters/tests/fixtures/text_extract_reference.json`.
Replaces: REQ-0815, REQ-0816, REQ-0817, REQ-1111.

`(?:...)`, lookaround and a character class contribute no number. The failure
reports the path, the requested group and the number of capturing groups.

A group that the pattern declares but the match does not enter has no value,
so `str_extract` produces missing and `no_match` does not apply: the pattern
did match, and `no_match` applies only when the pattern matched nowhere in the
subject. A missing source returns the declared `missing` value. The handlers
are those of [Column derivation](columns.md).

<a id="text-11"></a>

### TEXT-11. str_replace replaces every non-overlapping match, left to right, with literal text

`str_replace` is a search that replaces every match. The scan starts before the
source's first scalar; each step keeps the match [TEXT-9](#text-9) would keep
among those starting at or after the scan position, and the next step starts
where that match ends, so matches never overlap. After an empty match the next
step starts one scalar later, copying that scalar unchanged. Each match is
replaced by the literal `replacement` text, scalars outside matches are kept in
order, and an empty result is the empty string.

Evidence: `schema-str-replace`, `rust/crates/yamaa-core/tests/execution_declarative_text_replace.rs`, `yaml/conformance/regex.yaml`.
Replaces: REQ-1318, REQ-1319, REQ-1320, REQ-1321, REQ-1322.

Every step matches against the whole source, so `^` still matches only before
its first scalar and a lookbehind reads the scalars before the scan position.
The scan of one source value is one match call, and one step counter covers all
of its steps ([TEXT-12](#text-12)).

An empty match is replaced like any other ([TEXT-9](#text-9)). With pattern
`x*` and replacement `-`, `ab` becomes `-a-b-`; with pattern `a*`, `baa`
becomes `-b--`.

In `replacement`, `$`, the backslash and braces have no special meaning, and no
capture group is referenced. A source the pattern matches nowhere is returned
unchanged, and a result with no scalars is the empty string, never missing. A
missing source returns the declared `missing` value; without that handler, the
missing source is fatal under [Column derivation](columns.md).

<a id="text-12"></a>

### TEXT-12. A match that exceeds its step limit is refused, never treated as no match

The portable matcher follows the shared fixed resource policy. Each complete
match call starts a fresh step counter covering search starts, backtracking and
nested assertions. A match-step refusal is reported as `engine_rejected`, with
`resource` `regex_steps`, the applied `limit`, and `actual` when known. A
refused match cannot be treated as an unmatched string or a skipped
verification.

Fails: `engine_rejected`.
Evidence: `rust/crates/yamaa-adapters/tests/file_diagnostic_origins.rs::matching_budget_refusal_retains_pattern_path_and_applied_bound`, `rust/crates/yamaa-core/tests/numeric_aggregate_parser.rs::fixed_regex_steps_preserve_the_actual_resolution_prefix_and_fresh_retry`.

The policy is the [fixed resource policy](../docs/release.md#resource-limits).
For a column `matches` check, `spec_paths` names its declaring pattern, such as
`columns.TERM.verifications[0].matches.pattern`. Pattern or context nesting
beyond `semantic_depth` keeps its typed compilation or resource refusal and its
declaring path.
