# Values

Every value has one runtime type. These rules define the column types, the
missing value, numbers, text, dates and datetimes, the one conversion of a
completed result, and how values compare and order.

## Column types and conversion

<a id="value-1"></a>

### VALUE-1. A column declares exactly one of five types, and every type also admits missing

A column must declare exactly one type: `str`, `int`, `float`, `date` or
`datetime`. Every type also admits the missing value. There is no Boolean
column type; a flag is a `str` column with `allowed_values`. The vocabulary is
closed: a declared type outside it fails schema validation, and extending it
is a rule change, never an implementation option.

Fails: `value_not_permitted`.
Evidence: `negative-ambiguous-type`, `rust/crates/yamaa-core/tests/schema_references.rs`.
Replaces: REQ-0001, REQ-0002, REQ-0012, REQ-1150.

`str` follows [VALUE-11](#value-11) to [VALUE-12](#value-12), `int` and
`float` follow [VALUE-5](#value-5) to [VALUE-10](#value-10), and `date` and
`datetime` follow [VALUE-13](#value-13) to [VALUE-18](#value-18). A declared
type outside the vocabulary fails under
[Schema language](reference/schema-language.md)'s `values` constraint.

A schema descriptor's `type`, a column's declared `type` and a value's runtime
type belong to different naming lists. In
`- type: {type: column_type, required: true}`, the outer `type` names a
specification field and the inner `type` is a schema descriptor keyword.
Schema types include `str`, `int`, `float`, `bool`, `"null"`, `list`, `dict`
and named types. Of these built-ins only `str`, `int` and `float` are also
column types, with the same runtime value meanings. Values carry their runtime
types; an expression input does not declare one.

<a id="value-2"></a>

### VALUE-2. An operation never converts its inputs, and an input of an incompatible type fails

Named operation inputs must have compatible runtime types. An operation must
not implicitly convert one named input to the type of another, and an input
with an incompatible runtime type fails. Column conversion applies only to the
completed derivation result, at the conversion stage of the
[Execution lifecycle](columns.md#column-8).

Fails: `incompatible_input_type`.
Evidence: `rust/crates/yamaa-core/tests/support/diagnostic/binding.rs`, `rust/crates/yamaa-adapters/tests/fixtures/correlated_aggregates.json`.
Replaces: REQ-0004, REQ-0323.

<a id="value-3"></a>

### VALUE-3. A completed result converts to its column's type by one table, or fails

At the conversion stage, a completed derivation result converts from its
runtime type to its column's declared type by the table below. A missing
result stays missing without attempting conversion and never triggers the
`unconvertible` handler. A `fail` cell, an unsuccessful parse or an undefined
conversion is a conversion failure: `unconvertible` answers it when declared,
and otherwise it is fatal. A conversion never selects an unspecified
representation or silently substitutes missing.

Fails: `conversion_failed`.
Evidence: `negative-number-below-limit`, `rust/crates/yamaa-core/tests/fixtures/conversion.tsv`, `rust/crates/yamaa-adapters/tests/fixtures/original_conversion_handlers.json`.
Replaces: REQ-0009, REQ-0010, REQ-0011, REQ-0013.

The source is the value's runtime type and the destination is the column's
declared type. Identity preserves the value. A Boolean fails conversion to
every column type even when the schema accepts it as a literal.

| Source | `str` | `int` | `float` | `date` | `datetime` |
| --- | --- | --- | --- | --- | --- |
| missing | missing | missing | missing | missing | missing |
| `str` | identity | numeric parse, then integral conversion | numeric parse | temporal parse | temporal parse |
| `int` | decimal text | identity | widening | fail | fail |
| `float` | shortest positional text | integral conversion | identity | fail | fail |
| `date` | canonical text | fail | fail | identity | fail |
| `datetime` | canonical text | fail | fail | fail | identity |
| `bool` | fail | fail | fail | fail | fail |

Numeric parse is [VALUE-6](#value-6), integral conversion
[VALUE-7](#value-7), and widening and numeric text [VALUE-8](#value-8); a
non-integral or out-of-range value fails as [VALUE-7](#value-7) states.
Temporal parse is the lexical form of [VALUE-15](#value-15), canonical text is
[VALUE-17](#value-17), and the prohibited temporal conversions are
[VALUE-18](#value-18). Converting a temporal value to `str` keeps its
canonical fields but not its collected precision. Ingestion uses the same
`str` row for a field's declared type ([INPUT-7](inputs.md#input-7)). The
`unconvertible` handler is [Column derivation](columns.md)'s, and an
unanswered failure is fatal under the
[Column derivation](columns.md). A derivation whose every result would meet
a `fail` cell is refused before any record is read ([VALUE-24](#value-24)).

<a id="value-24"></a>

### VALUE-24. A derivation whose result type can never convert to its declared type fails validation

A derivation fails validation with `conversion_failed`, before any record is
read, when no non-missing result of its expression's result type
([EXPR-14](expressions.md#expr-14)) can convert to its declared type: every
type the result can take meets a `fail` cell of [VALUE-3](#value-3)'s table.
Column and row derivations, typed intermediate derivations and aggregate
derive bindings are checked. A derivation declaring `unconvertible` is not
refused, as its literal answers each failure, and a result that is always
missing converts to every type.

Fails: `conversion_failed`.
Evidence: `negative-study-day-date-column`, `rust/crates/yamaa-core/tests/support/diagnostic/result_types.rs`, `rust/crates/yamaa-adapters/tests/fixtures/column_literals.json`, `rust/crates/yamaa-adapters/tests/scalar_language.rs::row_boolean_is_refused_at_validation_and_refuses_every_save`.

The failure names the column whose lifecycle cannot complete, as a
conversion failure does, or the typed intermediate derivation or derive
binding, and reports `from`, the result type, and `to`, the declared type. A
`str` result can always parse, and an `int` or `float` result can convert to
either number, so neither is refused here; a value that fails still fails at
conversion under [VALUE-3](#value-3). A study day declared `date`, a date
difference declared `datetime` and a Boolean literal in any column are
refused. `yamaa check` reports the refusal wherever the declared types
establish it; a result type taken from an undeclared stored field is known
only once binding captures the field.

## Missing values

<a id="value-4"></a>

### VALUE-4. A non-finite float becomes missing before any consumer can see it

Positive infinity, negative infinity and every binary64 NaN become missing
immediately at each boundary where one can arise: when a YAML scalar
resolves, when a typed source value is received or stored text parses as a
number, when an expression or handler selects or substitutes a result, after
each numeric operation, and when a project binding returns a scalar. No
consumer observes a non-finite float, and no artifact or canonical value
spells one.

Evidence: `schema-non-finite`, `rust/crates/yamaa-adapters/tests/memory_resolver_benchmarks.rs`.
Replaces: REQ-0006, REQ-0007, REQ-0008, REQ-1149.

The boundaries are exactly these:

- after YAML core-schema scalar resolution and before validating or using
  literals or defaults in specifications, schemas, environments or
  conformance documents;
- after receiving a typed source value or parsing stored text as a number;
- after a built-in expression, mapping, conditional, coalescing operation or
  handler selects or substitutes a result;
- after each numeric operator, scalar numeric function or aggregate
  reduction;
- after a project binding returns a scalar and before its result contract is
  checked.

Normalization precedes dispatch, missing handling, conversion, comparison,
equality, grouping, ordering, range selection, key validation, verification,
contract fingerprinting and artifact rendering. Each consumer observes missing
and applies its own missing behavior.

Normalization acts on values, not on text sentinels. An unquoted YAML
non-finite scalar resolves to a float and becomes missing. The same characters
quoted remain text, including when stored in a `str` destination; only numeric
parsing ([VALUE-6](#value-6)) gives such text a numeric meaning.

Normalization does not bypass a constraint that prohibits missing: non-missing
keys, a mandatory column ([SUB-11](profiles/submission.md#sub-11)) and
`not_missing` still reject the value. A function binding that returns
a non-finite float satisfies its result contract only when that contract
allows missing through `may_return_missing: true`.

## Numbers

<a id="value-5"></a>

### VALUE-5. An int is a signed 64-bit integer, and leaving that range fails with integer_overflow

`int` is a signed 64-bit integer, from `-2^63` through `2^63 - 1`, and `float`
is IEEE 754 binary64. An integer literal outside the `int` range fails with
`integer_overflow` during validation at its specification path, whether it is
written in an expression or as any other runtime literal. An `int` result of
`+`, `-` or `*` outside the range fails arithmetic evaluation with
`integer_overflow`.

Fails: `integer_overflow`.
Evidence: `negative-integer-overflow`, `rust/crates/yamaa-adapters/tests/fixtures/integer_literal_overflow.json`, `rust/crates/yamaa-core/tests/fixtures/arithmetic.tsv`.
Replaces: REQ-0014, REQ-0434.

An out-of-range literal never introduces an arbitrary-width integer value.
Runtime literals include derivation and replacement literals,
`allowed_values` entries, and length or value bounds. Declaring a `float` or
`str` result does not widen an integer literal's range: quote a numeric
identifier as text, or write a floating-point literal when that is the
intended value. Declaration integers such as the mathematical row-count bounds
of [CHECK-13](verification.md#check-13) keep their own rules and are
not runtime integer values. Non-finite floats become missing under
[VALUE-4](#value-4) before any consumer can use them.

<a id="value-6"></a>

### VALUE-6. Numeric text parses by one grammar, and every other spelling fails

Text parses as a number only when it is the expression grammar's `number`
production with an optional leading `+` or `-` and no surrounding whitespace,
or exactly `.inf`, `.Inf` or `.INF` with an optional sign, or `.nan`, `.NaN` or
`.NAN` without a sign. The non-finite spellings parse as floats and become
missing under [VALUE-4](#value-4) instead of failing. Every other spelling is
a conversion failure under [VALUE-3](#value-3).

Fails: `conversion_failed`.
Evidence: `rust/crates/yamaa-core/tests/fixtures/conversion.tsv`, `negative-source-na-age`.
Replaces: REQ-0015, REQ-0020.

<a id="value-7"></a>

### VALUE-7. Conversion to int succeeds only for an exactly integral value within range

Converting a number to `int` succeeds only for an exactly integral value
within the signed 64-bit range; it never rounds or truncates. A non-integral
value or one outside the range is a conversion failure under
[VALUE-3](#value-3), distinct from an arithmetic error. A specification that
wants an integral part chooses it explicitly with `FLOOR`, `CEIL` or `TRUNC`
before column conversion.

Fails: `conversion_failed`.
Evidence: `negative-conversion-fractional`, `rust/crates/yamaa-core/tests/fixtures/numeric_lifecycle.tsv`.
Replaces: REQ-0016, REQ-0021.

<a id="value-8"></a>

### VALUE-8. An int widens to the nearest binary64, and a number converts to one canonical text

Widening `int` to `float` is exact for magnitudes below `2^53`; a larger
magnitude takes the nearest binary64 value. An `int` converts to `str` as
decimal text. A finite `float` converts to the shortest sequence of
significant decimal digits that parses back to the same binary64 value,
written in positional notation without an exponent, and an integral `float`
omits the trailing `.0`.

Evidence: `rust/crates/yamaa-core/tests/fixtures/conversion.tsv`.
Replaces: REQ-0017, REQ-0018.

Shortest chooses the significant digits, not the character count. Artifact
serialization under the [CSV profile](reference/csv.md) uses the same canonical
numeric text when no display precision is selected.

<a id="value-9"></a>

### VALUE-9. Values are used unrounded, and presentation rounding never feeds back

Calculations, comparisons, verifications and dependent derivations use
unrounded values. Display precision under the [CSV profile](reference/csv.md)
applies the one rounding of [FORMULA-16](formulas.md#formula-16) once, when a
field is written, and [Project functions](profiles/functions.md) conformance
comparison rounds a temporary copy the same way; neither changes a value the
specification uses.

Evidence: `adam-adlb-reported-precision`, `rust/crates/yamaa-core/tests/source_csv_precision.rs::ties_and_near_ties_round_away_from_zero_with_independent_text`.
Replaces: REQ-0019, REQ-0435.

Floating-point results are not exact decimals: `POWER(x, 2)` and `x * x` may
differ in the last place, and a specification cannot round the difference
away. A derivation that needs a stable decimal is written as a formula that
produces a stable decimal.

<a id="value-10"></a>

### VALUE-10. Each arithmetic operator and numeric function returns a fixed numeric type

`+`, `-` and `*` return `int` for two `int` operands and `float` when either
operand is a `float`; `ABS`, `MOD` and `NULLIF` likewise return `int` when
every argument is `int` and otherwise `float`. `/` always returns `float`,
widening each `int` operand first under [VALUE-8](#value-8); there is no
integer division. `SQRT`, `POWER`, `EXP`, `LN`, `CEIL`, `FLOOR` and `TRUNC`
return `float`.

Evidence: `schema-numeric-functions`, `rust/crates/yamaa-core/tests/fixtures/numeric_math.tsv`, `rust/crates/yamaa-core/tests/fixtures/numeric_integral.tsv`, `rust/crates/yamaa-adapters/tests/intermediate_result_kinds.rs::nullif_widens_its_int_result_when_the_float_argument_is_missing`.
Replaces: REQ-0420, REQ-0421, REQ-0422, REQ-0423, REQ-0424.

The quotient is the binary64 quotient of the widened operands, including when
both operands are `int`. Above `2^53` it need not equal the exact integer
quotient rounded once to binary64: `9007199254740993 / 3` returns
`3002399751580330.5`, although the exact quotient `3002399751580331` is
representable as a float. `FLOOR(a / b)` floors this binary64 quotient and
returns an integral `float`; it need not equal the floor of the exact integer
quotient.

To get an integer from `CEIL`, `FLOOR` or `TRUNC`, declare the column
`type: int`: the [Column derivation](columns.md) converts the
completed result under [VALUE-7](#value-7).

`NULLIF` compares its arguments exactly under [VALUE-19](#value-19),
including a mixed `int` and `float` pair. Thus
`NULLIF(9007199254740993, 9007199254740992.0)` returns `9007199254740993`
promoted to `float`, which is `9007199254740992.0`. An argument's type is its
declared type, also where its value is missing: `NULLIF(x, y)` with an `int`
`x` and a `float` `y` is a `float`, and where `y` is missing it returns `x`
widened under [VALUE-8](#value-8), so `9007199254740993` again returns
`9007199254740992.0`. Only over two `int` arguments, or a `NULL` `y`, does it
return `x` as an `int`.

These types, applied to the declared types of the operands, give `compute` and
`aggregate` their result type ([EXPR-14](expressions.md#expr-14)).

## Text

<a id="value-11"></a>

### VALUE-11. A string is a sequence of Unicode scalar values, decoded strictly where it enters

A runtime `str` is a finite sequence of Unicode scalar values, `U+0000` through
`U+D7FF` and `U+E000` through `U+10FFFF`. Unassigned scalar values are valid
and have no special behavior, and missing is not a string. External text is
decoded before it enters the language, by the encoding its container fixes
where one is specified. Ill-formed encoded text or a decoded surrogate code
point fails with `invalid_text` at its entry boundary.

Fails: `invalid_text`.
Evidence: `negative-source-bad-encoding`, `rust/crates/yamaa-core/tests/support/diagnostic/yaml_source.rs`.
Replaces: REQ-0022, REQ-0023, REQ-0029, REQ-0030.

Surrogate code points are invalid. Ill-formed encoded values are rejected,
never replaced, skipped or decoded by a machine default, and no host default
substitutes for this contract.

<a id="value-12"></a>

### VALUE-12. Text is never normalized, and two strings are equal only scalar for scalar

No stage implicitly applies NFC, NFD, NFKC, NFKD or any other normalization.
Ingestion, binding, string conversion, concatenation, interpolation, casing,
mapping, grouping, comparison, key validation and rendering preserve scalar
sequences, except the explicitly defined ASCII casing substitutions. Two
strings are equal exactly when they contain the same scalar values in the same
positions; identity comparisons perform no casing, folding, normalization,
locale tailoring or compatibility mapping.

Evidence: `schema-text-functions`, `yaml/conformance/regex.yaml`.
Replaces: REQ-0024, REQ-0025, REQ-0027, REQ-0028.

Canonically equivalent sequences remain distinct. The language has no
normalization operation; a specification that needs normalized data receives
it from an explicitly governed upstream process. The ASCII casing
substitutions are those of [TEXT-5](text.md#text-5).

This equality applies to predicates, case-sensitive mapping, joins and record
intermediates, group and window partitions, keys, uniqueness checks, allowed
values, and every other comparison of string identities. Equal strings have
byte-identical UTF-8 encodings.

## Dates and datetimes

<a id="value-13"></a>

### VALUE-13. A date names one calendar day and a datetime one civil second, with no gaps

A `date` is a complete proleptic Gregorian calendar date naming one day. A
`datetime` is a complete local civil datetime: a date of the same kind
together with a time of day resolved to a whole second, naming a reading on a
wall clock rather than an instant on a timeline. Both also admit the missing
value. Every combination of fields in the ranges below names one day or civil
moment, and every day or moment in range has one combination.

Evidence: `rust/crates/yamaa-core/tests/fixtures/conversion.tsv`.
Replaces: REQ-0539, REQ-0540, REQ-0541, REQ-0542, REQ-0543, REQ-0559.

| Field | Range | In `date` | In `datetime` |
|---|---|---|---|
| year | `0001` to `9999` | yes | yes |
| month | `01` to `12` | yes | yes |
| day | `01` to the length of that month in that year | yes | yes |
| hour | `00` to `23` | -- | yes |
| minute | `00` to `59` | -- | yes |
| second | `00` to `59` | -- | yes |

Both value spaces are total and gapless. No civil time is nonexistent or
ambiguous: a daylight-saving gap or repetition arises only when a wall-clock
reading is mapped to a timeline, and a zone supplies that mapping. Without a
zone, every field combination in the ranges above is a value of the type and
denotes the reading it spells, so there is nothing to reject as
unrepresentable and nothing to disambiguate.

<a id="value-14"></a>

### VALUE-14. A datetime carries no zone, offset or fractional second, and never shifts implicitly

A `datetime` carries no timezone and no offset and resolves to a whole second.
Text carrying a zone, an offset or a fractional second is rejected rather than
normalized, and parsing keeps the fields of the text. A datetime is never
implicitly shifted into another zone; only an explicit `shift` moves a civil
value, under [TIME-14](temporal.md#time-14). A study that collects sub-second
times keeps the collected text in a `str` column.

Evidence: `negative-datetime-zones`, `adam-adae-post-dose-window`.
Replaces: REQ-0555, REQ-0560, REQ-0561, REQ-0563, REQ-0564, REQ-0565, REQ-0612.

Whole seconds are exact in both hosts, which fixes how a host represents a
temporal value:

| Host | `date` | `datetime` |
|---|---|---|
| Python | `datetime.date` | `datetime`; tzinfo unset; microsecond zero |
| R | `Date` | `POSIXct` with `tzone` set to `"UTC"` |

An R `datetime` sets `tzone` to `UTC` rather than leaving it empty.

A value holding a fractional or leap second is never reached: such text is
rejected first, and an implementation never rounds to reach such a value.

<a id="value-15"></a>

### VALUE-15. Text becomes a date or datetime only in one zero-padded extended form

Every place text becomes a temporal value uses one form: a `date` is
`YYYY-MM-DD`, and a `datetime` is a date, `T`, and `hh:mm` with an optional
`:ss`. `YYYY` is four ASCII digits and every other field two, zero-padded. An
omitted `ss` names second `00`; nothing else is defaulted, and no sign,
surrounding whitespace, other separator or field order is recognised. Any other
text, or a field outside its range, is not a temporal value: reaching a
temporal column, it is a conversion failure under [VALUE-3](#value-3).

Fails: `conversion_failed`.
Evidence: `negative-date-incomplete`, `negative-datetime-zones`, `rust/crates/yamaa-core/tests/fixtures/conversion.tsv`.
Replaces: REQ-0549, REQ-0550, REQ-0551, REQ-0552, REQ-0553, REQ-0554, REQ-0599, REQ-0600, REQ-0601, REQ-0602, REQ-0603.

```text
date     := YYYY "-" MM "-" DD
datetime := date "T" time
time     := hh ":" mm [ ":" ss ]
```

One production defines the calendar half of both types, so a date parses
identically wherever it appears. These text forms are rejected:

| Rejected | Offered as | Why |
|---|---|---|
| `2025-01` | `date` | truncated; a partial date is not a date |
| `20250112` | `date` | basic format; the extended form only |
| `2025-1-2` | `date` | fields not padded to their width |
| `2025-02-30` | `date` | not a date in the calendar |
| `2025-01-12T14:00:00` | `date` | a time of day; a moment is not a day |
| `2025-01-12` | `datetime` | no time of day; a day is not a moment |
| `14:00:00` | `datetime` | no date; a time of day alone is not a value |
| `2025-01-12 14:00:00` | `datetime` | a space; the separator is `T` |
| `2025-01-12t14:00:00` | `datetime` | lowercase `t`; the separator is uppercase |
| `2025-01-12T14:00:00Z` | `datetime` | a zone designator |
| `2025-01-12T14:00:00+02:00` | `datetime` | an offset |
| `2025-01-12T14:00:00.5` | `datetime` | a fractional second |
| `2025-01-12T24:00` | `datetime` | hour 24; midnight is the next day's `00:00` |
| `2025-01-12T23:59:60` | `datetime` | a leap second |

A date part that is not a date in the calendar, such as `2025-02-30`, and a
year outside `0001` to `9999` fail the same way; the four-digit field admits
no other year. A clock reading that carries no date is not a value of either
type: a study that collects one, as the `--TM` family does, keeps the
collected text as `str`.

[Inputs](inputs.md) applies the `str` row of [VALUE-3](#value-3), which
reaches this form, so a field declared `date` or `datetime` in a `types`
declaration parses exactly as a column conversion parses. The two paths differ
only in what answers a bad value. An ingested value that does not parse is
rejected before any derivation, so no handler applies
([INPUT-7](inputs.md#input-7)). A `str` field converted at a column declaring
a temporal type fails there, where `unconvertible` can answer under
[Column derivation](columns.md); otherwise the failure is fatal under
the [Column derivation](columns.md). A specification that wants
to see a malformed value therefore leaves the field `str`.

<a id="value-16"></a>

### VALUE-16. Every temporal value is complete and records its collected precision on one ladder

A temporal value is complete or is not a value of its type; a truncated
collected value stays text until `impute` completes it. Each value has a
collected precision on one ladder, `none`, `year`, `month`, `day`, `hour`,
`minute`, `second`: the finest field its collected source supplied, or `none`
when no collected field supplied it. A `date`'s precision is at most `day`.
Where a value came from fixes its collected precision, and every field finer
than it was supplied.

Evidence: `schema-birth-date`, `adam-adae-partial-dates`, `adam-adae-partial-times`, `schema-impute-fallback`, `schema-imputation-flags`.
Replaces: REQ-0544, REQ-0545, REQ-0546, REQ-0547, REQ-0548.

| Origin | Collected precision |
|---|---|
| parsed text | `date`: `day`; `datetime`: `second` |
| `impute` from text | the finest field the text carried, at most `day` for a `date` |
| `impute` from a value of its type | unchanged |
| `impute`'s `fallback` | `none` |
| `to_date` | from text, `day`; from a `datetime`, its precision capped at `day` |
| selecting an existing value | unchanged; the property follows the selected value |
| `shift` | unchanged from the source temporal value |

The parsed-text row admits no other answer: the form of
[VALUE-15](#value-15) admits only complete text, so nothing a parse produces
was collected in part. A parsed datetime is collected through the second: an
omitted `ss` names second zero rather than claiming a coarser value. `impute`
may instead complete collected text that stops after its year, month, day,
hour or minute under declared rules, producing a complete value whose
collected precision is the last field that text carried
([TIME-8](temporal.md#time-8)): a datetime completed from `2025` carries
`year`. [Expressions](expressions.md) fixes the selecting row for the extreme,
conditional, coalescing and offset expressions that return an operand rather
than computing one.

Provenance is read off collected precision rather than recorded beside it. A
date whose precision is `day` was collected in full, `month` had its day
supplied, `year` its month and day, and `none` its every field. A datetime
whose precision is `day` had its whole time supplied, `hour` had its minute and
second supplied, `minute` had its second supplied, and `second` means the
source carried every field. [TIME-18](temporal.md#time-18) reports these as
ADaM imputation flags.

<a id="value-17"></a>

### VALUE-17. A temporal value is written in one canonical text that carries its fields alone

A temporal value is written back in exactly one form: `YYYY-MM-DD` for a
`date` and `YYYY-MM-DDThh:mm:ss` for a `datetime`, every field zero-padded and
seconds always shown. This is the text a temporal value converts to as `str`
and the text the artifact records for a temporal column; neither form takes a
project setting. Canonical text is not the collected text, and it carries no
collected precision.

Evidence: `rust/crates/yamaa-core/tests/fixtures/conversion.tsv`, `rust/crates/yamaa-adapters/tests/table_arrow_table.rs`.
Replaces: REQ-0003, REQ-0566, REQ-0567, REQ-0568, REQ-0569, REQ-0570.

A `str` column derived from a temporal value and the artifact's rendering of
that value therefore never disagree; [VALUE-8](#value-8) fixes the same
relationship for `float`. A value parsed from `2025-01-12T14:00` renders as
`2025-01-12T14:00:00`: the value names second zero. Like `float`, which renders
`1.50` as `1.5`, a declared type stores a value rather than the received
characters, and a column that must keep the collected characters unchanged is
`str`. Rendered precision is fixed at one day and one second.

Other text remains `str` and uses scalar order under
[VALUE-20](#value-20). For values of one temporal type, canonical fixed-width
text has the same order as the represented fields; this does not widen the
accepted forms of [VALUE-15](#value-15).

Collected precision is not observable outside the derivation. Each of these
carries the day or the moment and nothing about how much of it was collected:

- a temporal value converted to `str` under [VALUE-3](#value-3);
- the artifact's record of a temporal column;
- the typed value [Project functions](profiles/functions.md) encodes for a
  function argument.

A specification that must carry precision past any of those boundaries
derives a column from `precision` or `imputation_flag`
([TIME-7](temporal.md#time-7), [TIME-18](temporal.md#time-18)), which is data
the artifact records like any other.

<a id="value-18"></a>

### VALUE-18. A date and a datetime never convert into each other or enter compute arithmetic

A `date` and a `datetime` do not convert in either direction; such a
conversion fails and chooses neither value. A temporal value never enters
`compute` arithmetic, whose grammar under
[Formulas](formulas.md) is numeric. Differences and
shifts are the named operations of [Dates and times](temporal.md), with
explicit units and input contracts.

Fails: `conversion_failed`.
Evidence: `rust/crates/yamaa-core/tests/fixtures/conversion.tsv`.
Replaces: REQ-0576, REQ-0577, REQ-0604.

## Comparison and order

<a id="value-19"></a>

### VALUE-19. An int and a float compare exactly; every other type compares only with itself

`int` and `float` are mutually comparable, and every other runtime type is
comparable only with itself. Every comparison of a mixed `int` and `float`
pair uses its exact mathematical value, never rounding the integer to
binary64: predicates, `NULLIF`, extremes, lookup key pairs, grouping keys,
ordering terms, aggregate `MIN` and `MAX`, `cut` breaks and range bounds alike.
Comparing values that are not mutually comparable fails rather than converting
an operand.

Fails: `incompatible_input_type`, `incomparable_sources`.
Evidence: `negative-greatest-mixed`, `negative-least-mixed`, `rust/crates/yamaa-adapters/tests/fixtures/intermediate_key_reference.json`.
Replaces: REQ-0005, REQ-0324, REQ-0605.

So `9007199254740993` and `9007199254740992.0` are unequal everywhere.
Collected temporal precision changes neither a value's
runtime type nor its comparability. The type's owner defines its order:
[VALUE-20](#value-20) for `str` and [VALUE-21](#value-21) for `date` and
`datetime`.

Incompatible inputs fail as the incompatible-input error of
[VALUE-2](#value-2); this includes comparing or ordering a temporal value
against another type. A `sources` list or an ordering term mixing two runtime
types that are not mutually comparable fails rather than converting an
operand. For `greatest` and `least`, comparability is checked statically
against the declared source and output column types, including sources whose
collected values are missing, and a failure is `incomparable_sources`. Equal
extreme values keep the first source in authored order, including that
source's type and collected temporal precision.

<a id="value-20"></a>

### VALUE-20. Strings order by scalar value, position by position

Non-missing strings order lexicographically by numeric scalar value. The first
unequal position determines the order; when one sequence is a prefix of
another, the shorter is less, and equal sequences tie. A supplementary-plane
scalar compares as one value, never as a surrogate pair. The order does not
depend on normalization, case folding, locale, collation, character names,
script properties, encoded bytes or UTF-16 code units.

Evidence: `schema-text-functions`, `schema-ordering`.
Replaces: REQ-0026.

Predicates, ordering terms, `greatest`, `least` and aggregate `MIN` and `MAX`
use this order. Their owning operations define missing placement or empty
results separately.

<a id="value-21"></a>

### VALUE-21. Values of one temporal type order chronologically, whatever their collected precision

Two values of the same temporal type compare field by field, most significant
first: year, then month, then day, and for a `datetime` then hour, minute and
second. Every pair of non-missing values of one type is ordered, and that order
is chronological; no datetime comparison fails. Collected precision takes no
part in a comparison: a value completed from a year and a month compares as
the day it names.

Evidence: `adam-adsl-alive-date`, `adam-adcm-end-date-bound`, `sdtm-dm-dates`.
Replaces: REQ-0571, REQ-0572, REQ-0573, REQ-0574.

No value carries a zone, so a comparison cannot be between a civil time and an
instant. A completed date names a day; a value completed from a year and a
month therefore:

- orders against a fully collected one on the day it names;
- wins a `greatest` it is the latest operand of;
- satisfies a predicate the day satisfies.

Both types are comparable wherever a rule requires mutually comparable values.
`greatest` and `least` reduce them across a row, an `order_by` term orders by
one, and [Formulas](formulas.md)'s `MIN` and `MAX` reduce one.
[VALUE-22](#value-22) sorts missing values lowest unless the term's `nulls`
places them, as it does for every other type. Every pair of non-missing values of one type stays
ordered, which keeps an `order_by` term total and keeps the comparability of
[VALUE-19](#value-19) intact.

<a id="value-22"></a>

### VALUE-22. An order term sorts by its type's order and direction, and missing sorts lowest unless nulls places it

Every field typed `list[order_by_term]` is a list of order terms, applied in
order. A term is a bare variable or a mapping declaring `variable`,
`direction` and `nulls`; a bare variable means `{variable: X, direction: asc}`.
`direction` is `asc` or `desc` and orders the non-missing values. Without
`nulls`, a missing value sorts lowest: first under `asc` and last under
`desc`. A declared `nulls: first` or `nulls: last` places missing values there
under either direction.

Evidence: `schema-ordering`, `adam-adae-review-order`, `adam-adae-serious-sequence`, `schema-baseline-tie-break`, `rust/crates/yamaa-adapters/tests/execution_declarative_self.rs::windows_and_presentation_sort_an_undeclared_missing_value_lowest`, `rust/crates/yamaa-adapters/tests/execution_declarative_self.rs::source_and_lookup_selection_sort_an_undeclared_missing_value_lowest`, `rust/crates/yamaa-core/tests/dataset_ordering.rs::presentation_order_preserves_all_types_ties_and_direction_independent_nulls`.
Replaces: REQ-0297, REQ-0298, REQ-0299, REQ-0300, REQ-0301, REQ-0302, REQ-0312, REQ-1102, REQ-1103.

The bare form is a [Schema language](reference/schema-language.md) shorthand
union, whichever operation declares the list, and a mapping holding only
`variable` restates it and fails as `redundant_long_form` under
[SCHEMA-15](reference/schema-language.md#schema-15). `direction` defaults to
`asc`.
`nulls` has no schema default: a resolved specification leaves an omitted
`nulls` omitted, and the placement it then means follows `direction`. Each
term has its own direction and placement:

| `direction` | `nulls` omitted | `nulls: first` | `nulls: last` |
| --- | --- | --- | --- |
| `asc` | missing first | missing first | missing last |
| `desc` | missing last | missing first | missing last |

The same reading applies to every order term: a window's `order_by`, the
`order_by` beside `keep` in a named lookup or a source selection,
`output.order_by` and the `tie_break` of `baseline_flag`
([WINDOW-9](windows.md#window-9)). In `[{variable: A, direction: desc}, B]` a
missing `A` sorts after every present `A` and a missing `B` before every
present `B`.

Non-missing values use the order their type owns: exact numeric comparison
under [VALUE-19](#value-19), including a mixed `int` and `float` pair without
rounding the integer to binary64; text order under [VALUE-20](#value-20); and
chronological order for `date` and `datetime` under [VALUE-21](#value-21).
Window ordering requires mutually comparable values for each term;
[Inputs](inputs.md) fixes source-field types ([INPUT-5](inputs.md#input-5)),
and [VALUE-1](#value-1) declared column types.

Ties, totality and tie-breaking follow [VALUE-23](#value-23), so a row's
neighbours are determined.

<a id="value-23"></a>

### VALUE-23. Rows equal on every order term keep their construction order

Rows equal on every declared term keep their construction order, and window
ordering uses the same tie-break. Ties are valid and must not change between
runs over the same input. A specification may declare an additional term to
break a tie.

Evidence: `schema-ordering`, `rust/crates/yamaa-core/tests/dataset_ordering.rs::presentation_order_preserves_all_types_ties_and_direction_independent_nulls`.
Replaces: REQ-0225.
