# Formulas

Three closed text languages are written inside a specification: predicates
in fields typed `predicate`, numeric formulas in `compute`, and reducer
expressions in `aggregate`. Each grammar has one source under
[yaml/grammar/](../yaml/grammar/README.md), rendered into the card that owns
it.

## Predicates

<a id="formula-1"></a>

### FORMULA-1. A predicate is one Boolean text in the closed predicate grammar

A field typed `predicate` holds one Boolean text in the grammar below. Row,
aggregate, window, record-lookup and multiple-match filters, `case` and the
`assert` verification use it. An operand is only a name or a literal:
arithmetic, `CASE`, aggregates, windows, subqueries, host-language calls and
`!=` are not in the grammar, and the one function call is `str_contains`
([FORMULA-8](#formula-8)). Text that does not parse as one predicate,
including a prohibited operator or construct, fails.

Fails: `invalid_predicate`.
Evidence: `schema-predicates`, `negative-review-arithmetic`, `yaml/grammar/predicate.yaml`.
Replaces: REQ-0158, REQ-0160, REQ-0162, REQ-0188.

<!-- grammar: predicate -->

```text
predicate   := disjunction
disjunction := conjunction ("OR" conjunction)*
conjunction := negation ("AND" negation)*
negation    := "NOT"* boolean
boolean     := comparison | null_test | call | "(" predicate ")" | "TRUE" | "FALSE"
comparison  := operand compare operand
             | operand ["NOT"] "IN" "(" operand ("," operand)* ")"
             | operand ["NOT"] "BETWEEN" operand "AND" operand
             | operand ["NOT"] "LIKE" operand ["ESCAPE" string]
null_test   := operand "IS" ["NOT"] "NULL"
call        := "str_contains" "(" operand "," string ")"
compare     := "=" | "<>" | "<" | "<=" | ">" | ">="
operand     := identifier | literal
identifier  := name ["." name]
name        := (letter | "_") { letter | digit | "_" }
literal     := number | string | temporal | "NULL"
number      := ["+" | "-"] digits ["." digits]
               [("e" | "E") ["+" | "-"] digits]
digits      := digit { digit }
letter      := "A" ... "Z" | "a" ... "z"
digit       := "0" ... "9"
string      := "'" { non_quote | "''" } "'"
non_quote   := any Unicode scalar value other than "'"
temporal    := "DATE" string | "DATETIME" string
```

The block renders `yaml/grammar/predicate.yaml`, the grammar's single
source. Its `reserved` list closes the keywords of
[FORMULA-2](#formula-2), and its cases record the texts the language accepts
and rejects, the identifiers in each accepted text and each accepted parse.
Text that does not parse fails with `invalid_predicate`. A value computed
before comparison is first bound to a named column; an internal column may be
omitted from `output.columns`.

<a id="formula-2"></a>

### FORMULA-2. Predicate keywords are reserved and case-insensitive, and NOT binds before AND before OR

Whitespace may separate tokens but cannot occur inside a number, identifier or
keyword. Precedence is `NOT`, then `AND`, then `OR`; repeated binary operators
associate from the left, and parentheses override precedence. Keywords,
`NULL`, `TRUE` and `FALSE` are case-insensitive, and identifiers are
case-sensitive. The keywords are reserved as bare names, but a qualified field
may use one of those spellings after its qualifier.

Evidence: `yaml/grammar/predicate.yaml`, `schema-predicates`.
Replaces: REQ-0161.

The reserved keywords are `AND`, `BETWEEN`, `DATE`, `DATETIME`, `ESCAPE`,
`FALSE`, `IN`, `IS`, `LIKE`, `NOT`, `NULL`, `OR` and `TRUE`.

<a id="formula-3"></a>

### FORMULA-3. A predicate literal has a fixed runtime type, and a temporal literal needs its keyword

A `number` has the number form of the numeric grammar
([FORMULA-10](#formula-10)) and may have a leading sign; it has runtime type
`int` when it has neither a fractional part nor an exponent, and `float`
otherwise. A `string` is delimited by single quotes, a doubled quote denotes
one quote, backslash has no escape meaning, and its type is `str`. `DATE '...'`
and `DATETIME '...'` are temporal literals whose text must parse under
[Values](values.md) for the named type.

Fails: `invalid_predicate`.
Evidence: `yaml/grammar/predicate.yaml`, `rust/crates/yamaa-adapters/tests/fixtures/predicate_diagnostics.tsv`.
Replaces: REQ-0163, REQ-0164, REQ-0165, REQ-0192.

`'C:\new'` contains a backslash, and a string literal is text under
[Values](values.md). The temporal keyword is required:
`'2025-06-01'` alone is a `str`, not a `date`.

A temporal literal that [Values](values.md) rejects while
the predicate is parsed fails with `invalid_predicate`. The parser finding
keeps the literal kind and the applicable temporal cause, which standalone
syntax diagnostic context reports; a finding in the original document keeps
the predicate and its character position. Temporal operations outside the
predicate grammar keep their own temporal conditions.

<a id="formula-4"></a>

### FORMULA-4. A predicate is TRUE, FALSE or UNKNOWN, and only TRUE retains a row or record

A predicate evaluates to `TRUE`, `FALSE` or `UNKNOWN` under the three-valued
tables below. `NULL` is missing and has no runtime type. A comparison with a
missing operand is `UNKNOWN`; `IS NULL` and `IS NOT NULL` test for
missingness and are never `UNKNOWN`. A filter retains a row or record only for
`TRUE`, and a verification holds only for `TRUE`.

Evidence: `rust/crates/yamaa-core/tests/fixtures/predicate_evaluation.tsv`, `schema-predicates`.
Replaces: REQ-0159, REQ-0166, REQ-0171.

`NOT TRUE` is `FALSE`, `NOT FALSE` is `TRUE`, and `NOT UNKNOWN` is `UNKNOWN`.

| `AND` | TRUE | FALSE | UNKNOWN |
|---|---|---|---|
| **TRUE** | TRUE | FALSE | UNKNOWN |
| **FALSE** | FALSE | FALSE | FALSE |
| **UNKNOWN** | UNKNOWN | FALSE | UNKNOWN |

| `OR` | TRUE | FALSE | UNKNOWN |
|---|---|---|---|
| **TRUE** | TRUE | TRUE | TRUE |
| **FALSE** | TRUE | FALSE | UNKNOWN |
| **UNKNOWN** | TRUE | UNKNOWN | UNKNOWN |

[Verification](verification.md) defines every other consequence of
a predicate's result.

<a id="formula-5"></a>

### FORMULA-5. Two non-missing operands compare only when their runtime types are mutually comparable

Two non-missing operands compare only when
[VALUE-19](values.md#value-19) makes their runtime types mutually
comparable, in the order the table below gives, and no other operand
conversion occurs. A mixed `int` and `float` pair widens the integer to
binary64 under [VALUE-8](values.md#value-8) before comparing. Every
other pair fails: a temporal value is not comparable to text, and a `date` is
not comparable to a `datetime`.

Fails: `incompatible_input_type`.
Evidence: `negative-review-date-text`, `rust/crates/yamaa-core/tests/fixtures/predicate_evaluation.tsv`.
Replaces: REQ-0167, REQ-0168, REQ-0169, REQ-0190.

| Types | Order |
|---|---|
| `int` with `int` | Exact integer order |
| `float` with `float` | Binary64 numeric order |
| Mixed `int` and `float` | Widen the integer to binary64 under [VALUE-8](values.md#value-8), then compare |
| `str` with `str` | [Values](values.md) text order |
| `date` with `date` | Chronological under [Values](values.md) |
| `datetime` with `datetime` | Chronological under [Values](values.md) |

Strings use the equality and total order [Values](values.md)
defines. Above `2^53`, widening can make a neighboring integer equal to a
float: `9007199254740993 = 9007199254740992.0` is `TRUE`. The same pair stays
unequal in the exact comparisons [VALUE-19](values.md#value-19) lists.
Two non-missing operands that are not mutually comparable, or a non-string
operand to `LIKE` ([FORMULA-7](#formula-7)), fail with
`incompatible_input_type`.

<a id="formula-6"></a>

### FORMULA-6. IN and BETWEEN expand into comparisons, and their NOT forms negate the result

`x IN (a, b, c)` is `x = a OR x = b OR x = c`, and every non-missing list
operand must be comparable with `x`. `x BETWEEN a AND b` is
`x >= a AND x <= b`, and both endpoints are inclusive. `NOT IN`,
`NOT BETWEEN` and `NOT LIKE` negate the corresponding result, so a missing
operand produces `UNKNOWN`, not `TRUE`.

Evidence: `schema-predicates`, `rust/crates/yamaa-core/tests/fixtures/predicate_evaluation.tsv`.
Replaces: REQ-0172, REQ-0173, REQ-0174, REQ-0175.

These compound operators expand into the primitive logic of
[FORMULA-4](#formula-4) and the comparisons of [FORMULA-5](#formula-5).

<a id="formula-7"></a>

### FORMULA-7. LIKE matches text scalar by scalar, case-sensitively, with an optional one-scalar ESCAPE

Both non-missing `LIKE` operands must be `str`. In the pattern, `%` matches
any sequence of scalar values, `_` matches exactly one, and every other scalar
matches by [Values](values.md) equality; matching is
case-sensitive. No escape character exists by default. `ESCAPE` declares a
string literal of exactly one scalar value, which marks the next pattern scalar
as literal. An invalid `ESCAPE` literal or a trailing escape character fails.

Fails: `incompatible_input_type`, `invalid_predicate`.
Evidence: `adam-adae-protocol-review`, `schema-predicates`, `rust/crates/yamaa-adapters/tests/fixtures/syntax_controls.json`.
Replaces: REQ-0176, REQ-0177, REQ-0191.

A non-string operand fails with `incompatible_input_type`. An invalid
`ESCAPE` literal, or a pattern with a dangling escape, fails with
`invalid_predicate`.

```yaml
filter: "AEDECOD LIKE '100!%' ESCAPE '!'"
```

<a id="formula-8"></a>

### FORMULA-8. str_contains is the one predicate function and searches its source for a portable regex

`str_contains(source, pattern)` takes any operand as `source` and a string
literal holding a portable regex as `pattern`. It is `TRUE` when the regex
finds a match anywhere in the source, `FALSE` when it does not, and `UNKNOWN`
when the source is missing, including under `NOT`. A non-`str` source fails
with `incompatible_input_type`; a pattern the portable regex contract rejects
fails with `invalid_predicate` at parse time.

Fails: `incompatible_input_type`, `invalid_predicate`.
Evidence: `schema-str-contains`, `negative-str-contains-invalid-regex`.
Replaces: REQ-1244.

The portable regex contract is the one of
[Text operations](text.md#text-6). Function-name matching is
case-insensitive, so `STR_CONTAINS(...)` is the same call. `str_contains` is
the sole Boolean function the grammar permits: any other `name(...)` is
`invalid_predicate`, and a bare `str_contains` without `(` stays an
identifier.

<a id="formula-9"></a>

### FORMULA-9. A predicate identifier names a value visible at its site, and any other name fails

Each predicate site sees the names the table below gives, under
[Column derivation](columns.md). An identifier in a right-side
predicate is qualified by the right-side dataset's ID, and an identifier over
a completed or candidate output row is unqualified. No predicate can reach an
undeclared relation. Predicate identifiers are collected for dependency
ordering, so the parser rejects an unresolved name rather than treating a
predicate as dependency-free.

Fails: `unresolvable_name`, `unknown_field`.
Evidence: `negative-review-unknown-date`, `rust/crates/yamaa-adapters/tests/fixtures/flag_reference.json`.
Replaces: REQ-0178, REQ-0179, REQ-0180, REQ-0181, REQ-0182, REQ-0183,
REQ-0184, REQ-0185, REQ-0186, REQ-0189.

| Predicate site | Visible names |
| --- | --- |
| Ungrouped row filter | The names [ROW-6](rows.md#row-6) defines |
| Grouped row filter | Only unqualified columns derived by that row |
| Aggregate, record-lookup and multiple-match filters | Records of their owning right-side dataset |
| Window filter | Constructed output columns in row construction; in an intermediate derivation, the augmented donor records [LOOKUP-15](lookup.md#lookup-15) describes |
| Verification | Completed columns and record intermediates resolved for the completed row |
| `case` | The values available to its enclosing derivation |

An enclosing column derivation may also bind a qualified dataset or
record-lookup field under [Name binding](names.md) and
[Lookup and joins](lookup.md). A verification may read a declared record
lookup for its completed row.

An identifier that is unavailable at its predicate site fails. An unqualified
identifier that names a field of an in-scope dataset fails with
`unresolvable_name` and suggests the qualified spelling; any other unavailable
identifier fails with `unknown_field`. Both report under
[Column derivation](columns.md).

## Numeric formulas

<a id="formula-10"></a>

### FORMULA-10. compute evaluates one closed numeric formula per row

`compute` evaluates `expr`, a formula in the closed numeric grammar below, over
constructed output columns, fields of a declared record lookup and numeric
literals, and returns one numeric value per current row.
[Values](values.md)' representation, promotion and overflow
rules apply to every operator and function. Precedence is unary sign, then `*`
and `/`, then binary `+` and `-`, all left-associative, and parentheses
override it. Function names and `NULL` are case-insensitive; identifiers are
not. A formula that does not parse fails.

Evidence: `schema-numeric-functions`, `adam-adsl-bmi`, `yaml/grammar/numeric.yaml`.
Replaces: REQ-0407, REQ-0413, REQ-0414, REQ-0439, REQ-1119, REQ-1120,
REQ-1121.

<!-- grammar: numeric -->

```text
expr       := term (("+" | "-") term)*
term       := factor (("*" | "/") factor)*
factor     := ("-" | "+")? primary
primary    := number | "NULL" | identifier | call | "(" expr ")"
identifier := name ["." name]
call       := function "(" [expr ("," expr)*] ")"
number     := digits ["." digits] [("e" | "E") ["+" | "-"] digits]
```

The block renders `yaml/grammar/numeric.yaml`, the grammar's single source.
Its vocabulary closes the function list of [FORMULA-13](#formula-13), and its
cases record the texts the language accepts and rejects, the identifiers an
accepted text binds and the resulting parse. The grammar is a subset of SQL.

<a id="formula-11"></a>

### FORMULA-11. A formula identifier resolves as a predicate identifier does in the same phase

A formula identifier resolves as a predicate identifier does in the same
phase, so a formula and a predicate never disagree about a name. During column
derivation an unqualified identifier is a constructed output column, and a
qualified identifier is permitted only when its qualifier is a declared record
lookup `id`; it reads the named field of that lookup's selected record. An
identifier that does not resolve in its phase fails.

Evidence: `negative-direct-dose-read`, `rust/crates/yamaa-adapters/tests/scalar_language.rs`, `adam-adsl-bmi`.
Replaces: REQ-0408, REQ-0409, REQ-0410, REQ-0411, REQ-0412, REQ-0442,
REQ-0443.

An arbitrary `DATASET.VARIABLE` reference is not permitted during column
derivation: bind the source variable to a column first and compute from that
column. Omitting that binding column from `output.columns` keeps it out of the
final dataset. A qualified identifier whose qualifier is not a declared record
lookup fails, as does an identifier that resolves to neither a declared output
column nor a field of a declared record lookup.

During ungrouped row construction an identifier is either a variable of the
row template's input dataset, qualified exactly as `row.filter` qualifies a
variable, or an unqualified column derived by the same `rows` entry. A value
from another dataset reaches the formula only through such a column, bound by
a row-phase source or lookup under [LOOKUP-2](lookup.md#lookup-2).

During grouped row construction an identifier qualified to the row template's
input dataset must be one of the enclosing `row.group_by` variables. Other
values are first reduced to a row-derived column with `aggregate`. A value
from another dataset is first bound to a row-derived column with a source or
lookup under [LOOKUP-2](lookup.md#lookup-2); the formula itself qualifies no
dataset but its own.

```yaml
- name: HEIGHTCM
  type: float
  derivation:
    source: DM.HEIGHTCM
- name: BMI
  type: float
  derivation:
    compute:
      expr: "WEIGHTKG / POWER(HEIGHTCM / 100, 2)"
```

A lookup field carries the type [Inputs](inputs.md)
assigns to the field in the lookup's dataset.
[Column derivation](columns.md) collects formula identifiers and
lookup source dependencies, so a `compute` derivation takes part in dependency
ordering as a predicate does.

<a id="formula-12"></a>

### FORMULA-12. Every formula identifier must be numeric

Every identifier in a `compute` formula must have runtime type `int` or
`float`, and an identifier of any other type fails.
[VALUE-2](values.md#value-2) forbids implicit conversion between
operation inputs, and formulas do not relax that rule: a collected string is
converted by binding it to a numeric column first.

Evidence: `rust/crates/yamaa-core/tests/fixtures/evaluation.tsv`, `rust/crates/yamaa-core/tests/fixtures/numeric_integral.tsv`.
Replaces: REQ-0307, REQ-0426, REQ-0444.

<a id="formula-13"></a>

### FORMULA-13. The numeric functions are a closed list, each with a fixed argument count

The permitted functions are exactly those below, and each takes exactly the
arguments shown. Any other function name or argument count fails, as do an
operator outside the grammar, a string literal, a comparison or Boolean
operator, `CASE`, an aggregate function, a window function or `OVER`, a
subquery and a host-language call. `LOG` is excluded: write `LN(x)` or
`LN(x) / LN(b)`. Widening the vocabulary amends this list.

Evidence: `schema-numeric-functions`, `negative-formula-flag`, `negative-row-aggregate`.
Replaces: REQ-0415, REQ-0416, REQ-0417, REQ-0425, REQ-0440, REQ-0441.

- `ABS(x)`: absolute value.
- `CEIL(x)`: least integer value not less than `x`.
- `FLOOR(x)`: greatest integer value not greater than `x`.
- `TRUNC(x)`: `x` with its fractional part removed, toward zero.
- `SQRT(x)`: non-negative square root.
- `POWER(x, y)`: `x` raised to `y`.
- `EXP(x)`: `e` raised to `x`.
- `LN(x)`: natural logarithm.
- `MOD(x, y)`: remainder of `x / y`, taking the sign of `x`.
- `NULLIF(x, y)`: `NULL` when `x = y`, otherwise `x`.

No function in this grammar picks a row-wise extreme or the first present
value. Those are the `greatest`, `least` and `first_available` expressions
([EXPR-9](expressions.md#expr-9), [EXPR-10](expressions.md#expr-10)), and
this grammar is not widened to reach them.

<a id="formula-14"></a>

### FORMULA-14. NULL propagates through every operator and function except NULLIF

A `NULL` argument to an operator or function produces a `NULL` result, except
`NULLIF`, whose argument-level behavior [FORMULA-13](#formula-13) defines. A
`compute` derivation therefore needs no guarding predicate to survive a
missing input. A formula that must yield missing rather than fail uses
`NULLIF`: percentage change against a zero base is
`100 * (VALUE - BASE) / NULLIF(BASE, 0)`.

Evidence: `rust/crates/yamaa-core/tests/fixtures/numeric_selection.tsv`, `rust/crates/yamaa-core/tests/fixtures/evaluation.tsv`, `adam-adsl-bmi`.
Replaces: REQ-0427, REQ-0428.

<a id="formula-15"></a>

### FORMULA-15. Division by zero and an invalid root, logarithm or power fail the run

Division by zero, by `/` or by `MOD`, `SQRT` of a negative argument, `LN` of a
zero or negative argument, and `POWER` with a zero base and a negative
exponent or a negative base and a non-integer exponent each fail the run. A
failure is never silently converted to missing, and `NULLIF(denominator, 0)`
chooses missing explicitly. A failure reports the expression and the operation
that failed.

Fails: `invalid_power`.
Evidence: `negative-zero-division`, `negative-negative-sqrt`, `negative-log-zero`.
Replaces: REQ-0429, REQ-0430, REQ-0431, REQ-0432, REQ-0433, REQ-0445.

An invalid `POWER` fails with `invalid_power`. Failing rather than yielding
missing is consistent with [Column derivation](columns.md).

<a id="formula-16"></a>

### FORMULA-16. round_half_away_from_zero is the one rounding, and its tie behavior is fixed

`round_half_away_from_zero` rounds a numeric `source` to `digits` places,
where `digits` is an integer and a negative value rounds left of the decimal
point. A value exactly halfway between two candidates, or within
`sqrt(2^-52) * 10^-digits` below such a tie, rounds half away from zero.
Missing stays missing, a value that rounds to zero returns positive zero,
never negative zero, and the result is a `float`. No other rounding exists,
and a non-numeric source fails.

Fails: `incompatible_input_type`.
Evidence: `schema-round-half-away-from-zero-ties`, `negative-round-half-away-from-zero-non-numeric-source`, `rust/crates/yamaa-adapters/tests/scalar_declarative_rounding.rs`.
Replaces: REQ-0418, REQ-0419, REQ-1172.

The `compute` grammar has no rounding function, and a derivation must not
round by any other spelling. A non-numeric source is an
`incompatible_input_type` validation error. Analysis datasets otherwise carry
computed values at full precision; reporting decides the displayed places.
[Values](values.md) has the same rule at conversion, where
a non-integral value fails rather than being truncated.

`CEIL`, `FLOOR` and `TRUNC` are not presentation rounding: they return an
integral part, with no mode to choose. `FLOOR(a / b)` floors the binary64
quotient under [VALUE-10](values.md#value-10) and returns an integral
`float` under [VALUE-10](values.md#value-10). It need not equal the
floor of the exact integer quotient.

## Aggregate expressions

<a id="formula-17"></a>

### FORMULA-17. aggregate reduces one relation to one value with a closed reducer expression

`aggregate` is the only aggregate expression. It reduces one relation, or the
current input group, to one value with `expr`, a reducer expression in the
closed grammar below, in one of the contexts
[LOOKUP-18](lookup.md#lookup-18) permits. Precedence, associativity and the
permitted functions follow the numeric grammar ([FORMULA-10](#formula-10),
[FORMULA-13](#formula-13)). Reducer and function names and `NULL` are
case-insensitive, and identifiers are case-sensitive. An expression that does
not parse fails.

Evidence: `schema-aggregation`, `yaml/grammar/aggregate.yaml`, `rust/crates/yamaa-core/tests/numeric_aggregate_parser.rs`.
Replaces: REQ-0295, REQ-0475, REQ-0476, REQ-0499, REQ-1089, REQ-1091,
REQ-1092.

<!-- grammar: aggregate -->

```text
expr       := term (("+" | "-") term)*
term       := factor (("*" | "/") factor)*
factor     := ("-" | "+")? primary
primary    := number | "NULL" | identifier | reduction | call | "(" expr ")"
reduction  := reducer "(" (expr | star) ")"
star       := name "." "*"
call       := function "(" [expr ("," expr)*] ")"
identifier := name ["." name]
number     := digits ["." digits] [("e" | "E") ["+" | "-"] digits]
```

The block renders `yaml/grammar/aggregate.yaml`, the grammar's single source.
Its vocabulary closes the reducers of [FORMULA-18](#formula-18), and its cases
record the texts the language accepts and rejects, how accepted identifiers
bind and the parse each case produces.

<a id="formula-18"></a>

### FORMULA-18. The reducers are exactly SUM, COUNT, MIN, MAX, MEAN and ONLY

A reduction applies one of the reducers below to an expression, or `COUNT` to
`D.*`. Any other reducer name or argument count fails, as do a window function
or `OVER`, a subquery, `CASE`, a comparison or Boolean operator, a string
literal and a host-language call. `AVG` is not an alias: the reducer name is
`MEAN`, and no median is registered. Any new reduction amends this table.

Evidence: `schema-aggregation`, `adam-adlb-mean`, `rust/crates/yamaa-core/tests/numeric_aggregate_parser.rs`.
Replaces: REQ-0477, REQ-0478, REQ-0481, REQ-0484, REQ-0500, REQ-0505,
REQ-0512.

| Reducer | Result |
|---|---|
| `SUM(x)` | total of the non-missing values |
| `COUNT(x)` | how many values of `x` are non-missing |
| `COUNT(D.*)` | how many records the group contains |
| `MIN(x)` | smallest non-missing value |
| `MAX(x)` | largest non-missing value |
| `MEAN(x)` | arithmetic mean of the non-missing numeric values |
| `ONLY(x)` | value from the group's only record; more than one fails |

`COUNT(D.*)` takes no other argument. `D` stands for the relation named by the
expression's qualified identifiers, as in `COUNT(EX.*)`, and a `COUNT(D.*)`
whose dataset is not the expression's relation fails. It is the one reducer
that names no column, and it counts records where `COUNT(x)` counts values.

<a id="formula-19"></a>

### FORMULA-19. SUM adds in relation record order, and MEAN divides that SUM by COUNT

`SUM(x)` adds the non-missing argument values in relation record order,
starting with the first such value and adding each later value with the
numeric `+`. Summation is never reordered, reassociated or partitioned, and
never compensated or correctly rounded. For a group with at least one
non-missing value, `MEAN(x)` is that ordered `SUM(x)` divided by `COUNT(x)`
with the numeric `/`, and it inherits the binary64 rounding of `SUM`. A
reduction does not sort the records.

Evidence: `adam-adlb-ordered-sum`, `rust/crates/yamaa-core/tests/fixtures/reduction.tsv`, `rust/crates/yamaa-core/tests/execution_case_predicates.rs::correlated_sum_and_mean_keep_donor_order_through_cancellation_and_missing_values`.
Replaces: REQ-0479, REQ-0480, REQ-0498.

The `filter`, when present, removes records and keeps the order of the records
that remain. [Inputs](inputs.md) defines stored-source
record order, and [Column derivation](columns.md) defines
constructed-output and grouped-input record order. `COUNT`, `MIN` and `MAX`
are independent of record order, and `ONLY` accepts no group in which an order
could choose among records ([FORMULA-20](#formula-20)). A rule that needs one
record chosen by value order uses a window or the `order_by` plus `keep`
selection ([LOOKUP-10](lookup.md#lookup-10)), where the value order is
declared.

<a id="formula-20"></a>

### FORMULA-20. ONLY returns the value of a group's one record and fails on more than one

`ONLY` counts records, not non-missing values. An eligible group with one
record returns that record's value, even when the value is missing. An
eligible group with more than one record after its filter fails the current
derivation rather than choosing by value or record order; this is not a
missing-value case. The failure reports the enclosing row, the group values
and the record count.

Evidence: `negative-multiple-baselines`, `negative-adlb-duplicate-wbc`, `adam-adlb-bds`.
Replaces: REQ-0482, REQ-0494, REQ-0501.

`ONLY` is the reduction for a grouped calculation that requires one source
record and must reject duplicates.

<a id="formula-21"></a>

### FORMULA-21. Reductions do not nest

A reduction argument contains no reduction, so `MAX(SUM(EX.EXDOSE))` fails,
reporting the outer and inner reducers. Reducing at one key level and reducing
that result at another uses two specifications: the first artifact names and
validates the intermediate keys, and the downstream specification declares
that artifact as an ordinary source under
[Name binding](names.md), linking its producing specification
through `dataset_class.schema`.

Evidence: `yaml/grammar/aggregate.yaml`, `rust/crates/yamaa-core/tests/numeric_aggregate_parser.rs`, `rust/crates/yamaa-adapters/tests/fixtures/syntax_controls.json`.
Replaces: REQ-0483, REQ-0502.

The link through `dataset_class.schema`
([INPUT-10](inputs.md#input-10)) supplies the workflow boundary:
the producer completes first under
[INPUT-10](inputs.md#input-10), and the consumer reads its held
serialized artifact under [FILE-3](reference/files.md#file-3). One
domain build executes both levels; neither artifact must already exist, and
only explicit Save publishes them. A source path alone does not establish this
boundary. The producer's selected columns must declare their types and labels
under [INPUT-11](inputs.md#input-11).

For example, `daily.yaml` can group exposure records by subject, treatment
and day and derive `ADOSE: {aggregate: "SUM(EX.EXDOSE)"}`. A downstream
specification declares `DAILY: {path: daily.csv, schema: daily.yaml}`, with
the producer's `output.path` resolving to that same `daily.csv`. It groups
`DAILY` by subject, treatment and `DAILY.ADOSE` and derives
`TRTDUR: {aggregate: "COUNT(DAILY.*)"}`. If a subject has two 100-unit records
on day 1 and one 200-unit record on each of days 2 and 3, the first level
produces three daily totals of 200 and the second produces one row with
`ADOSE = 200` and `TRTDUR = 3`.

<a id="formula-22"></a>

### FORMULA-22. An identifier outside a reduction must name a grouping column

Every identifier appears inside a reduction unless it names a `group_by`
column, which is constant within the group and may be used directly.
`SUM(a) / SUM(b)` is legal, and `SUM(a) + b` fails unless `b` is grouped on;
the failure reports the identifier. For a grouped-row aggregate, the
enclosing `row.group_by` supplies the grouped columns.

Evidence: `negative-dose-intensity`, `rust/crates/yamaa-adapters/tests/fixtures/correlated_aggregates.json`.
Replaces: REQ-0485, REQ-0486, REQ-0503.

`SUM(EX.EXDOSE) / EX.EXPLDOS` is legal exactly when `EX.EXPLDOS` is declared
in `group_by`.

<a id="formula-23"></a>

### FORMULA-23. A reduction keeps the type it reduces, and arithmetic over reductions is numeric

`COUNT` returns `int` and `MEAN` returns `float`; `SUM` keeps its argument's
numeric type, and `MIN`, `MAX` and `ONLY` keep the type they reduce, whatever
it is. `SUM` and `MEAN` require a numeric argument, `MIN` and `MAX` mutually
comparable values, and `COUNT` and `ONLY` accept any type. An expression using
an operator or numeric function is numeric: every reduction and grouped
identifier in it must be numeric, and [Values](values.md)
promotion gives its type.

Evidence: `negative-sum-non-numeric`, `rust/crates/yamaa-core/tests/fixtures/reduction.tsv`.
Replaces: REQ-0313, REQ-0317, REQ-0487, REQ-0488, REQ-0489, REQ-0490,
REQ-0510, REQ-0511.

`SUM` or `MEAN` over a non-numeric argument fails, as does arithmetic over a
non-numeric reduction or grouped identifier. `MIN` or `MAX` over incomparable
values fails: a column mixing incomparable types is an error rather than an
implementation-defined order. Their string order is that of
[Values](values.md).

No implicit conversion happens inside the grammar.
[Values](values.md) converts the completed derivation
result, as it does for every other expression. A `function` expression
returns the type its logical contract declares under
[Project functions](profiles/functions.md).

<a id="formula-24"></a>

### FORMULA-24. A reduction skips missing values, and a group with nothing to reduce yields missing

Inside a reduction's argument `NULL` propagates, so a record whose operand is
missing contributes a missing value rather than a zero:
`SUM(EX.EXDOSE * EX.EXDUR)` skips a record missing either factor. A reduction
then ignores missing values. A group with no record after `filter` yields
missing. When every value is missing, `SUM`, `MIN`, `MAX` and `MEAN` yield
missing, never zero, and `COUNT(x)` yields `0`. An uncollected quantity is
never reported as a measured zero.

Evidence: `adam-adex-uncollected`, `rust/crates/yamaa-core/tests/fixtures/reduction.tsv`.
Replaces: REQ-0491, REQ-0492, REQ-0493, REQ-0495.

| Group | Result |
|---|---|
| No record in the group after `filter` | missing, as the absent match of [LOOKUP-13](lookup.md#lookup-13) |
| Every value missing -- `SUM`, `MIN`, `MAX`, `MEAN` | missing, never zero |
| Every value missing -- `COUNT(x)` | `0` |
| No record in the group -- `COUNT(x)`, `COUNT(D.*)` | missing |
| No record in the group -- `ONLY(x)` | missing |
| One record whose value is missing -- `ONLY(x)` | missing |

An absent record remains distinguishable from a collected missing value.
`MEAN` returns missing before its division when no non-missing value remains,
so an all-missing group does not fail with division by zero. Arithmetic over
reduction results follows [FORMULA-14](#formula-14): a missing reduction
propagates through an operator, and a formula that must yield missing rather
than fail says so with `NULLIF`.

<a id="formula-25"></a>

### FORMULA-25. Numeric failures and integer overflow inside an aggregate fail the run

The failure conditions of [FORMULA-15](#formula-15) apply to aggregate
arithmetic unchanged: division by zero, `SQRT` of a negative argument, `LN` of
a non-positive argument, an invalid `POWER` and integer overflow each fail the
run. `SUM` fails on integer overflow under the same condition, and `MEAN`,
defined by `SUM`, fails on that intermediate overflow even when the
mathematical mean would fit. The failure reports the expression and the column
that failed.

Replaces: REQ-0496, REQ-0513.

The non-finite normalization of [Values](values.md)
applies after every arithmetic or reduction result.

## Evaluation

<a id="formula-26"></a>

### FORMULA-26. Formulas evaluate deterministically and exactly as written

Evaluating a predicate, numeric formula or aggregate expression is
deterministic and free of side effects. Evaluation follows the written
association: no expression is reassociated, redistributed or algebraically
simplified, and no numeric option that does any of these is enabled. `/`
never truncates. String comparison uses [Values](values.md), and no
implicit coercion, collation, `LIKE` escape or missing-value behavior is taken
from a host SQL engine.

Evidence: `rust/crates/yamaa-core/tests/fixtures/arithmetic.tsv`, `rust/crates/yamaa-core/tests/portable_math_golden.rs`, `tools/tests/test_portable_math.py`.
Replaces: REQ-0187, REQ-0436, REQ-0437, REQ-0438, REQ-0497.

`a / (b * b)` and `a / b / b` are different formulas and may return different
doubles. Both are correct, and evaluation returns the double for the written
formula. Language or engine settings that make division integral are
overridden. A host SQL engine's coercion, collation, `LIKE` escape and
missing-value behaviors are configured and overridden to match these rules, or
the grammar is evaluated directly.
