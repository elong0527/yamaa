# Text

The text verbs pad, take a piece by position or between words, count scalars, change case,
interpolate templates, and extract and replace by pattern. They operate on the
string values of [VALUE-11](values.md#value-11) and preserve every scalar they
do not change, and `str_decimal` writes a number as text with fixed decimals.
One portable regular-expression language serves every pattern in a
specification.

## String verbs

<a id="text-1"></a>

### TEXT-1. Text verbs take string sources and fail rather than convert another type

`str_extract`, `str_replace`, `str_template`, `str_case`, `str_substr` and
`str_length` require string sources; a source of another runtime type fails
rather than being converted.

Fails: `incompatible_input_type`.
Evidence: `negative-str-case-number`, `negative-str-length-number`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/text.json::str_case_refuses_a_source_that_is_not_text`, `rust/tests/host-fixtures/cases.json::a_piece_of_a_number_in_a_row_template_is_refused_before_input`.

A source of another type fails under [VALUE-2](values.md#value-2).

<a id="text-2"></a>

### TEXT-2. str_pad left-pads a value's canonical text with ASCII spaces to a minimum width

`str_pad` takes `source` and a positive integer `width`. It converts a present
scalar source to its canonical text under [VALUE-3](values.md#value-3), then
adds ASCII spaces to the left until the text has at least `width` characters;
it does not truncate longer text. A missing source yields missing. A width
below one fails as `invalid_field_type`.

Fails: `invalid_field_type`, `engine_rejected`.
Evidence: `negative-str-pad-width`, `negative-str-pad-text-limit`, `adam-adlb-supplb-padded-key`, `rust/crates/yamaa-application/tests/fixtures/text_pad_reference.json`, `rust/crates/yamaa-core/tests/text_value_limit.rs`.

An unknown source or a failed conversion reports its existing condition.

The padded text is at most 1 MiB, 1,048,576 bytes, the `text_bytes` limit of
the [fixed resource policy](../docs/release.md#resource-limits). A present
source whose text and padding would be longer refuses the run before the text
is built, as `engine_rejected` with `resource` `text_bytes`, the applied
`limit`, the `actual` length when it is known, and the `str_pad` path; a
missing source needs no text, whatever the width. `str_decimal`,
`str_template`, `str_replace` and `STR_JOIN` hold the text they build to the
same limit.

<a id="text-13"></a>

### TEXT-13. str_substr takes the scalars of a text from a position counted from 1, and gives missing past its end

`str_substr` takes `source`, a positive integer `start` and an optional
positive integer `length`. It returns the Unicode scalar values of a present
string source from position `start`, counted from 1, at most `length` of them,
or every one to the end without `length`; fewer remaining give those that
remain. A `start` past the source's last scalar gives missing. A missing source
gives the declared `missing`, or missing. A `start` or `length` below one fails
as `invalid_field_type`.

Fails: `invalid_field_type`.
Evidence: `sdtm-co-long-comments`, `negative-str-substr-start`, `rust/tests/host-fixtures/cases.json::pieces_and_lengths_count_unicode_scalar_values`, `rust/tests/host-fixtures/cases.json::a_length_below_one_is_refused_before_input`.

A position counts the scalar values of [VALUE-11](values.md#value-11), never
bytes or UTF-16 code units, so a text splits the same way on every host. The
200 bytes [XPT-7](profiles/xpt.md#xpt-7) admits in a transport file and SDTM's
200 characters coincide for the printable ASCII that file holds. A piece is
never the empty string: a `start` within the source gives at least one scalar,
and the empty string, which has no scalar, gives missing at every `start`. A
`start` or `length` larger than any integer an implementation holds stays
valid: no text reaches such a `start`, and such a `length` takes every
remaining scalar, as an omitted one does.

With `start: 201` and `length: 200`, a text of 450 scalars gives its scalars
201 to 400, one of 250 scalars gives 201 to 250, and one of 200 scalars gives
missing. SDTM's 200-character pieces of a long value are written one
`str_substr` per piece, with `start` 1, 201, 401 and so on:

```yaml
- name: COVAL1
  type: str
  label: Comment
  derivation:
    str_substr: {source: CO_RAW.COMMENT, start: 201, length: 200}
```

Declared with `piece` instead of `start`, `str_substr` takes the pieces of a
text broken between words under [TEXT-16](#text-16).

`invalid_field_type` reports the field below one at its path, such as
`columns.COVAL1.derivation.str_substr.start`, with `expected` naming a positive
integer; when both fields are below one, `start` is reported. A source of
another type fails under [TEXT-1](#text-1). `missing` answers a missing source
and is counted under `missing`, declared or by default
([COLUMN-10](columns.md#column-10)); a `start` past the end gives missing
uncounted, as the verb's own result.

<a id="text-16"></a>

### TEXT-16. str_substr with piece breaks a text between words into pieces of at most length scalars and takes one

`str_substr` may declare a positive integer `piece` instead of `start`, and
then requires `length`. It breaks a present string source into pieces and
returns piece number `piece`, counted from 1, or missing past the last piece.
A text of at most `length` scalars is its one piece. Otherwise the next piece
ends at the last word end among its first `length` scalars, and the
whitespace after that word end is dropped; with no word end there, the piece
is exactly `length` scalars.

Fails: `invalid_field_type`, `missing_required_field`, `unknown_field`.
Evidence: `sdtm-suppmh-long-terms`, `rust/tests/host-fixtures/cases.json::pieces_break_between_words_and_cut_a_word_longer_than_length`, `rust/tests/host-fixtures/cases.json::a_start_beside_a_piece_is_refused_before_input`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/text.json::str_substr_requires_a_start_without_a_piece`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/text.json::str_substr_requires_a_length_beside_a_piece`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/text.json::str_substr_reports_only_the_piece_when_piece_and_length_are_both_below_one`, `rust/crates/yamaa-core/tests/scalar_sqrt.rs::a_piece_breaks_between_words_and_cuts_a_word_longer_than_length`.

A word end is a scalar that is not whitespace followed by one that is; the
`length`-th scalar is a word end when the scalar after it is whitespace, so a
piece fills all `length` scalars when a word ends exactly there. Whitespace is
the `\s` set of [TEXT-8](#text-8), so `U+00A0` breaks a text and a hyphen
does not. Scalars are counted as [TEXT-13](#text-13) counts them. The pieces
are read from the start of the text, each from where the one before it and
its dropped whitespace end, and the whitespace after a word end is dropped up
to the next scalar that is not whitespace, so whitespace that ends a text
longer than `length` is no piece. Every other scalar is kept in order, so a piece is never the empty string,
the empty string gives missing at every `piece`, and leading whitespace and
whitespace inside a piece stay as written. A word longer than `length` is cut
after `length` scalars and continues in the next piece. A `piece` larger than
any integer an implementation holds stays valid and gives missing.

SDTM's 200-character pieces split between words are written one
`str_substr` per piece. The first continuation, such as `MHTERM1`, is the
second piece:

```yaml
- name: MHTERM1
  type: str
  derivation:
    str_substr: {source: MH_RAW.MHTERM, length: 200, piece: 2}
```

A text of 213 scalars that is the word `papilledema` 17 times, then
`worsening`, each separated by one space, has its last word end within 200
scalars at scalar 191, so its first piece is 191 scalars, the space at 192 is
dropped, and its second piece is `papilledema worsening`. A text longer than
`length` has a second piece unless only whitespace follows its first, so a row
template that writes a continuation row filters as [TEXT-14](#text-14)
describes, or on the piece bound to a column that is not published.

A `start` declared beside `piece` fails as `unknown_field` at that `start`.
A declaration with neither `start` nor `piece` fails as
`missing_required_field` at `start`, and one with `piece` and no `length` at
`length`. Each names the field as `field`, the unknown `start` also as
`identifier`, and `str_substr` as `class`, as
[SCHEMA-13](reference/schema-language.md#schema-13) names a class field. A
`piece` below one fails as `invalid_field_type` at
`piece`, with `expected` naming a positive integer, reported before a
`length` below one. `missing` answers a missing source as under
[TEXT-13](#text-13), and a `piece` past the last piece gives missing
uncounted.

<a id="text-14"></a>

### TEXT-14. str_length counts the scalar values of a text as an int

`str_length` takes `source` and returns the number of Unicode scalar values in
a present string source as an `int`, the count that
[SUB-8](profiles/submission.md#sub-8)'s `length` and
[CHECK-8](verification.md#check-8)'s `max_length` bound, so a value that meets
a declared length has a `str_length` at or below it. The empty string gives
`0`. A missing source gives the declared `missing`, or missing.

Evidence: `sdtm-co-long-comments`, `rust/tests/host-fixtures/cases.json::pieces_and_lengths_count_unicode_scalar_values`.

A source of another type fails under [TEXT-1](#text-1). `missing` answers a
missing source and is counted under `missing`, declared or by default
([COLUMN-10](columns.md#column-10)).

A row template's `filter` and every other predicate admit no text verb: the
only functions they call are the numeric functions of
[FORMULA-13](formulas.md#formula-13) and `str_contains`
([FORMULA-8](formulas.md#formula-8)). A condition on a text's length is
therefore written in one of two ways. `str_contains` tests the length with a
quantified pattern:
`str_contains(CO_RAW.COMMENT, '^[\s\S]{201}')` is true for a comment of more
than 200 scalars, as a row template that writes a second piece filters on it;
`.` matches no line terminator ([TEXT-7](#text-7)), and `[\s\S]` matches every
scalar. Otherwise `str_length` is bound to a column, internal when it is not
published, and the condition compares that column.

<a id="text-15"></a>

### TEXT-15. str_decimal writes a number with exactly `decimals` digits by the one rounding, left-padded with spaces to a minimum width

`str_decimal` rounds a present numeric `source` to `decimals` places, zero or
more, by the one rounding of [FORMULA-16](formulas.md#formula-16) applied to
its exact binary64 value. It writes the result in positional notation with
exactly `decimals` digits after the point and a `-` only when the rounded
value is below zero, then adds ASCII spaces on the left until the text has at
least `width` characters, a positive integer when declared; it never
truncates. A missing source returns `missing`, or missing when it is omitted.

Fails: `incompatible_input_type`, `invalid_field_type`, `engine_rejected`.
Evidence: `adam-adlb-character-result-text`, `negative-str-decimal-text-source`, `negative-str-decimal-width-zero`, `rust/crates/yamaa-application/tests/fixtures/text_decimal_reference.json`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/text.json::str_decimal_refuses_both_counts_in_field_order`, `rust/crates/yamaa-core/tests/text_value_limit.rs`.

The rounding and the writing are those of CSV display precision
([CSV-9](reference/csv.md#csv-9)): a tie, or a value within
`2^-26 * 10^-decimals` below a tie, goes away from zero, and the rounded
decimal is written directly, never rounded back to binary64. A value written
with `decimals: n` therefore reads as a `float` column written under
`output.decimals` of `n` does. An `int` source widens to the nearest binary64
first, as an operand of a `float` result does
([VALUE-8](values.md#value-8)), so above `2^53` the text is that binary64
value's: `9007199254740993` with no decimals is `9007199254740992`.

With `decimals: 0` there is no decimal point; otherwise a value below one in
magnitude keeps one `0` before the point. Digits are never grouped and never
take an exponent, and a value that rounds to zero, negative zero included, is
written without a sign. `width` is a minimum, as [TEXT-2](#text-2)'s is: a
sign counts as one character, and text longer than `width` is written whole,
never shortened, given fewer decimals or replaced. Without `width` nothing is
added. This is the shape of a SAS `w.d` format, `put(AVAL, 8.2)` for
`decimals: 2, width: 8`, with the one rounding above and with text that does
not fit written whole.

```text
AVAL         decimals: 2, width: 8   decimals: 1
3.14159      |    3.14|              |3.1|
7            |    7.00|              |7.0|
-1.234       |   -1.23|              |-1.2|
2.675        |    2.68|              |2.7|
-0.001       |    0.00|              |0.0|
12345678.9   |12345678.90|           |12345678.9|
```

`2.675` is stored `1.8e-14` below its tie at two places, inside the window, so
it rounds up; at one place it is not near a tie. `missing` is a value field
([COLUMN-10](columns.md#column-10)), substituted as written and never padded,
so a period aligned with the numbers is written with its spaces,
`missing: "       ."`, and a missing source is counted under `missing`
whether or not it is declared ([COLUMN-14](columns.md#column-14)).

A `str`, `date`, `datetime` or Boolean source fails with
`incompatible_input_type` at `str_decimal.source`, whatever its values:
before any record is read where its declared type shows it
([EXPR-14](expressions.md#expr-14)), and otherwise when it binds, by the type
of the field it reads, a lookup's field by its donor's. `decimals` below zero
and `width` below one each fail validation with `invalid_field_type` at their
own field, both reported in field order. A larger count is admitted, but no
text is built longer than the 1 MiB of [TEXT-2](#text-2): a missing source
needs none, and a present value whose decimals or width would exceed it
refuses the run before the text is built, as `engine_rejected` with
`resource` `text_bytes` and the `str_decimal` path, without an `actual`
length when its decimals exceed it. The result type is `str`, and the text is
presentation ([VALUE-9](values.md#value-9)): the source keeps its unrounded
value for every other reader, arithmetic refuses the text
([FORMULA-12](formulas.md#formula-12)), and a value that must itself be
rounded is rounded by `round_half_away_from_zero`.

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
Evidence: `negative-subject-reference`, `yaml/grammar/string-template.yaml`, `rust/crates/yamaa-application/tests/fixtures/text_template_reference.json`.

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
a string fails. A missing value returns the declared `missing` value and,
unlike a single-input verb's missing source, fails without it. Otherwise the
result is the exact concatenation of literal text and values, the empty string
when the template is empty.

Fails: `incompatible_input_type`, `missing_input`, `engine_rejected`.
Evidence: `schema-shorthand`, `sdtm-dm-race-ethnicity`, `schema-expressions`, `rust/crates/yamaa-application/tests/fixtures/text_template_reference.json`, `negative-str-template-missing-placeholder`, `rust/crates/yamaa-core/tests/text_value_limit.rs`.

`str_template` accepts a bare template as
[Schema language](reference/schema-language.md) shorthand. The shorthand
expands to `{template: <written value>}` and adds no missing handler:

```yaml
str_template: "{METSTATR}|{ECOG0R}|{REGIONUSR}"
```

The canonical form exposes the optional `missing` handler, and it is written
only to declare one; a mapping holding only `template` restates the bare
template and fails as `redundant_long_form` under
[SCHEMA-15](reference/schema-language.md#schema-15):

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
value is missing; without it, the missing value fails as `missing_input` in
the `mapping` phase, and a template never gives missing by default
([COLUMN-10](columns.md#column-10)). The concatenation is exact under
[VALUE-12](values.md#value-12). A result longer than the 1 MiB of
[TEXT-2](#text-2) refuses the run before it is built, as `engine_rejected`
with `resource` `text_bytes` and the `str_template` path.

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
substitution to the first scalar, `title` to the first letter of each maximal
run of ASCII letters and `proper` to the first scalar and each scalar directly
after a delimiter, and each applies the downward one to the rest. Every other
scalar is unchanged.

Fails: `unknown_field`, `invalid_field_type`.
Evidence: `schema-sentence-case`, `schema-text-functions`, `adam-adae-proper-case-terms`, `negative-str-case-delimiters-mode`, `rust/crates/yamaa-application/tests/fixtures/text_case_reference.json`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/text.json::str_case_takes_delimiters_only_with_proper`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/text.json::str_case_refuses_delimiters_that_name_no_scalar_repeat_one_or_leave_ascii`.

`proper` writes text as SAS `PROPCASE` does. Its delimiters are the scalars
that `delimiters` lists, a non-empty string of ASCII scalars, each listed once;
without it they are the blank `U+0020`, `/`, `-`, `(`, `.` and the tab
`U+0009`, and a declared list replaces all six. `proper` first applies the
downward substitution to every ASCII letter, then identifies delimiter
positions in that lowered text before applying the upward substitution.
Delimiters are compared as written: with `delimiters: "c"`, both `MCDONALD`
and `mcdonald` give `McDonald`; with `"X"`, `AXBXC` gives `Axbxc`, and with
`"x"` it gives `AxBxC`. So `INVESTIGATOR'S DECISION` gives
`Investigator's Decision`, `2ND DEGREE BURN` gives `2nd Degree Burn` and
`PAIN,BACK` gives `Pain,back`,
where `title` gives `Investigator'S Decision`, `2Nd Degree Burn` and
`Pain,Back`; a list that names the comma gives `Pain,Back` too.

`delimiters` is declared only with `to: proper`. Beside another mode it fails
as `unknown_field` at that field, naming the field and the mode, as a field
that a temporal operation's declared type does not take fails under
[TIME-1](temporal.md#time-1); an empty list, a repeated scalar or a scalar
outside ASCII fails as `invalid_field_type`. Each fails where the specification
is read, before any input is read.

Every substitution preserves scalar count. None has a one-to-many mapping, a
context rule, language tailoring or a Unicode-version dependency, and word
detection is ASCII-only: a run for `title` is `[A-Za-z]+`, and a word for
`proper` starts only at the first scalar or after one of its delimiters, with
no locale and no Unicode word-break rule. Non-ASCII scalars, including
non-ASCII letters such as `U+00DF` and `U+0130`, pass through unchanged. A
missing source returns
`missing`, or missing when it is omitted, counted under `missing`
([COLUMN-10](columns.md#column-10)).

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

### TEXT-6. Every pattern is a portable ECMA-262 subset without lookaround or backreferences

A regular expression is admitted in a schema `pattern` descriptor,
`str_extract.pattern`, `str_replace.pattern`, a `matches` verification and the
`str_contains` predicate call. Each is ECMA-262 `Pattern` source text read with
the Unicode flag, without lookahead, lookbehind, backreferences, property
escapes `\p{...}`, flags or inline flag groups; a group name is an ASCII
identifier. An invalid pattern fails validation with `invalid_regex`, or with
`invalid_expression` in `str_contains`.

Fails: `invalid_regex`, `invalid_expression`.
Evidence: `negative-matches-bad-pattern`, `negative-str-replace-invalid-regex`, `negative-str-contains-invalid-regex`.

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
match. `(?<name>...)` names a group, and the name matches
`[A-Za-z_][A-Za-z0-9_]*`.

Every consumer reads patterns through one shared validator, which rejects
anything outside the subset, lowers the classes of [TEXT-8](#text-8) to
explicit sets, and compiles the pattern for a linear-time engine with
leftmost-first semantics ([TEXT-9](#text-9)). Every consumer therefore accepts
and rejects the same patterns and returns the same match, group and
verification outcome for the same pattern and subject, and matching takes time
linear in the subject.

These rules and `yaml/conformance/regex.yaml` (contract 3.0.0) are the
decisive authority. Each
fixture case names a pattern and either records that it is rejected or gives a
subject with its full-match, search and extraction outcomes, and optionally a
replacement and its outcome; the search outcome also governs `str_contains`.
Changing the grammar or the classes versions this contract and its fixtures.

A pattern the grammar or the classes reject, in a schema descriptor,
`str_extract`, `str_replace` or a `matches` verification, fails validation with
`invalid_regex` and reports the declaring path and the rejection. Rejected
constructs include lookahead `(?=` and `(?!`, lookbehind `(?<=` and `(?<!`,
backreferences `\1` to `\9` and `\k<name>`, `(?P<name>`, non-ASCII group
names, inline flag groups such as `(?i)`, `\p{...}` and malformed escapes such
as `\a`. The
`str_contains` predicate call instead fails with `invalid_expression` when its
expression is admitted, under [FORMULA-8](formulas.md#formula-8). Each consumer
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

Evidence: `yaml/conformance/regex.yaml`, `rust/crates/yamaa-application/tests/declaration_contract_vectors.rs::unchanged_regex_documents_and_authored_capture_controls_run_directly`.

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

Evidence: `yaml/conformance/regex.yaml`, `rust/crates/yamaa-application/tests/fixtures/regex_contract.tsv`.

The `\s` set is `U+0009`, `U+000B`, `U+000C`, `U+0020`, `U+00A0`, `U+1680`,
`U+2000` through `U+200A`, `U+202F`, `U+205F`, `U+3000`, `U+FEFF`, `U+000A`,
`U+000D`, `U+2028` and `U+2029`. `\b` and `\B` use the ASCII `\w`. The
validator lowers each class to its explicit set before compiling, so no class
widens to a Unicode category.

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
search that keeps the leftmost match and, among matches starting there, the
leftmost-first one: an earlier alternative wins, a greedy quantifier prefers
more iterations and a lazy one fewer. An empty match is a match.

Evidence: `yaml/conformance/regex.yaml`, `adam-adsl-demographics`, `schema-str-contains`.

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
Evidence: `negative-str-uncaptured-group`, `adam-adsl-site-parse`, `rust/crates/yamaa-application/tests/fixtures/text_extract_reference.json`.

`(?:...)` and a character class contribute no number. The failure reports the
path, the requested group and the number of capturing groups. A capture inside
a repetition keeps the last iteration that entered it, and past its minimum
count an empty iteration ends the repetition: `(a|){1,3}b` on `ab` captures the
empty string.

A group that the pattern declares but the match does not enter has no value,
so `str_extract` produces missing and `no_match` does not apply: the pattern
did match, and `no_match` applies only when the pattern matched nowhere in the
subject. A missing source returns the declared `missing` value, or missing
when it is omitted, counted under `missing`; present text the pattern matches
nowhere fails without `no_match`. The handlers are those of
[Column derivation](columns.md).

<a id="text-11"></a>

### TEXT-11. str_replace replaces every non-overlapping match, left to right, with literal text

`str_replace` is a search that replaces every match. The scan starts before the
source's first scalar; each step keeps the match [TEXT-9](#text-9) would keep
among those starting at or after the scan position, and the next step starts
where that match ends, so matches never overlap. After an empty match the next
step starts one scalar later, copying that scalar unchanged. Each match is
replaced by the literal `replacement` text, scalars outside matches are kept in
order, and an empty result is the empty string.

Fails: `engine_rejected`.
Evidence: `schema-str-replace`, `rust/crates/yamaa-core/tests/execution_declarative_text_replace.rs`, `yaml/conformance/regex.yaml`, `rust/crates/yamaa-core/tests/text_value_limit.rs`.

Every step matches against the whole source, so `^` still matches only before
its first scalar and an assertion such as `\b` reads the scalars around the
scan position. The whole scan is linear in the source.

An empty match is replaced like any other ([TEXT-9](#text-9)). With pattern
`x*` and replacement `-`, `ab` becomes `-a-b-`; with pattern `a*`, `baa`
becomes `-b--`.

In `replacement`, `$`, the backslash and braces have no special meaning, and no
capture group is referenced. A source the pattern matches nowhere is returned
unchanged, and a result with no scalars is the empty string, never missing. A
missing source returns the declared `missing` value, or missing when that
handler is omitted, counted under `missing`
([COLUMN-10](columns.md#column-10)).

A result longer than the 1 MiB of [TEXT-2](#text-2), such as a replacement
longer than its match written at many matches, refuses the run before it is
built, as `engine_rejected` with `resource` `text_bytes` and the
`str_replace` path.

<a id="text-12"></a>

### TEXT-12. A pattern too deep or too large to compile is refused at its path; matching is never refused

A pattern whose nesting exceeds `semantic_depth`, or whose compiled program
exceeds the fixed size limit, is refused when it compiles, as `engine_rejected`
with `resource` `semantic_depth` or `regex_size`, the applied `limit`, and the
declaring pattern path. A compiled pattern always completes a match in time
linear in the subject, so no match is refused, and a refusal is never treated
as an unmatched string or a skipped verification.

Fails: `engine_rejected`.
Evidence: `rust/crates/yamaa-core/tests/execution_declarative_text_replace.rs::oversized_patterns_report_the_compiled_size_limit_at_the_pattern`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/text_replace.json::a_catastrophic_pattern_without_a_match_leaves_the_value_unchanged`, `rust/tests/host-fixtures/cases.json::pattern_beyond_the_compiled_size_limit`.

The policy is the [fixed resource policy](../docs/release.md#resource-limits).
For a column `matches` check, `spec_paths` names its declaring pattern, such as
`columns.TERM.verifications[0].matches.pattern`. Nesting counts every group,
alternation, sequence, repetition and class.
