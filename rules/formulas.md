# Formulas

One closed expression language is written inside a specification: in fields
typed `predicate`, in `compute.expr` and in `aggregate.expr`. Its grammar has
one source under [yaml/grammar/](../yaml/grammar/README.md), rendered into
[FORMULA-1](#formula-1), and the field a text is written in decides the root
it has and what it may use ([FORMULA-27](#formula-27)). String templates keep
their own grammar ([TEXT-3](text.md#text-3)).

## The language

<a id="formula-1"></a>

### FORMULA-1. An expression is one text in the closed expression grammar

Fields typed `predicate`, `compute.expr` and `aggregate.expr` each hold one
text in the grammar below. Comparisons, Boolean logic, arithmetic, the closed
functions and the reducers are the whole language: `CASE`, windows,
subqueries, host-language calls and `!=` are not in it. The whole text is
read, and checked against its field, before any name is resolved or value
read. Text that does not parse as one expression fails, as does a truth value
where a value belongs or a value where a truth value belongs.

Fails: `invalid_expression`.
Evidence: `schema-predicates`, `schema-numeric-functions`, `schema-aggregation`, `negative-flag-condition`, `yaml/grammar/expression.yaml`.
Replaces: REQ-0158, REQ-0160, REQ-0162, REQ-0188, REQ-0413, REQ-0439,
REQ-0475, REQ-0499, REQ-1089, REQ-1120.

<!-- grammar: expression -->

```text
expression  := disjunction
disjunction := conjunction ("OR" conjunction)*
conjunction := negation ("AND" negation)*
negation    := "NOT"* comparison
comparison  := sum [compare sum | membership | range | pattern | null_test]
compare     := "=" | "<>" | "<" | "<=" | ">" | ">="
membership  := ["NOT"] "IN" "(" expression ("," expression)* ")"
range       := ["NOT"] "BETWEEN" sum "AND" sum
pattern     := ["NOT"] "LIKE" sum ["ESCAPE" string]
null_test   := "IS" ["NOT"] "NULL"
sum         := product (("+" | "-") product)*
product     := factor (("*" | "/") factor)*
factor      := ["+" | "-"] primary
primary     := literal | reduction | call | identifier | "(" expression ")"
reduction   := reducer "(" expression ")" | "COUNT" "(" star ")"
star        := name "." "*"
call        := function "(" [expression ("," expression)*] ")"
identifier  := name ["." name]
literal     := number | string | temporal | "TRUE" | "FALSE" | "NULL"
temporal    := ("DATE" | "DATETIME") string
number      := digits ["." digits] [("e" | "E") ["+" | "-"] digits]
digits      := digit { digit }
name        := (letter | "_") { letter | digit | "_" }
letter      := "A" ... "Z" | "a" ... "z"
digit       := "0" ... "9"
string      := "'" { non_quote | "''" } "'"
non_quote   := any Unicode scalar value other than "'"
```

The block renders `yaml/grammar/expression.yaml`, the grammar's single
source. Its vocabulary closes the functions of [FORMULA-13](#formula-13) and
[FORMULA-8](#formula-8) and the reducers of [FORMULA-18](#formula-18), its
`reserved` list closes the keywords of [FORMULA-2](#formula-2), and its cases
record, in each context of [FORMULA-27](#formula-27), the texts the language
accepts and rejects, the identifiers an accepted text binds and its parse.
The grammar is a subset of SQL.

A text is admitted in this order: the whole text is tokenized, then parsed,
then its integer literals are checked ([FORMULA-3](#formula-3)), then its
field refuses the constructs it does not admit, and then each function,
reduction and operand is checked, an inner one before the one that contains
it. The first fault found is the one reported. Text that does not parse fails
with `invalid_expression`, reporting the field's text and, in a predicate, the
character position of the fault.

`AND`, `OR` and `NOT` take truth values, the root of a predicate is a truth
value, and every other operand is a value. `A = TRUE` and `A + (B = 1) = 2`
place a truth value where a value belongs, and a predicate `A` or `NOT 1`
places a value where a truth value belongs; each fails with
`invalid_expression`.

<a id="formula-2"></a>

### FORMULA-2. Keywords and the names of functions and reducers are case-insensitive, and a sign binds tightest and OR loosest

Whitespace may separate tokens but cannot occur inside a number, identifier or
keyword. From loosest to tightest, precedence is `OR`, `AND`, `NOT`, a
comparison, binary `+` and `-`, `*` and `/`, then a unary sign. Repeated
binary operators associate from the left, a comparison does not associate,
and parentheses override precedence. Keywords and function and reducer names
are case-insensitive, and identifiers are case-sensitive. A reserved keyword
is not a bare name, but a qualified field may use one after its qualifier.

Evidence: `yaml/grammar/expression.yaml`, `schema-predicates`.
Replaces: REQ-0161, REQ-0414, REQ-0476.

The reserved keywords are `AND`, `BETWEEN`, `FALSE`, `IN`, `IS`, `LIKE`,
`NOT`, `NULL`, `OR` and `TRUE`. `DATE` and `DATETIME` begin a temporal
literal only directly before a string literal, and `ESCAPE` is a keyword only
after a `LIKE` pattern; anywhere else each is an ordinary name, as are `CASE`,
`WHEN`, `THEN`, `ELSE`, `END` and `OVER`. A function or reducer name is not
reserved either: `SUM` without `(` is a name.

`A = B = C` does not parse, since a comparison takes two sums.
`ALT > 3 * ULN` compares `ALT` with `3 * ULN`, and the upper bound of
`BETWEEN` binds tighter than `AND`: `X BETWEEN 1 AND Y + 1 AND Z = 2` is
`(X BETWEEN 1 AND (Y + 1)) AND (Z = 2)`. `-A * B` is `(-A) * B`, and `--A`
does not parse.

<a id="formula-3"></a>

### FORMULA-3. A literal has a fixed runtime type, an integer literal fits int, and a temporal literal needs its keyword

A `number` with neither a fractional part nor an exponent is an `int`, any
other is a `float`, and a sign written before a number belongs to it. An
integer literal must lie in the `int` range. A `string` is delimited by single
quotes, a doubled quote denotes one quote, backslash has no escape meaning,
and its type is `str`. `TRUE` and `FALSE` are truth values and `NULL` is
missing. `DATE '...'` and `DATETIME '...'` are temporal literals whose text
must parse under [Values](values.md) for the named type.

Fails: `invalid_expression`, `integer_overflow`.
Evidence: `yaml/grammar/expression.yaml`, `rust/crates/yamaa-adapters/tests/fixtures/predicate_diagnostics.tsv`, `rust/crates/yamaa-core/tests/fixtures/numeric_literals.tsv`.
Replaces: REQ-0163, REQ-0164, REQ-0165, REQ-0192.

`-9223372036854775808` is the least `int`, and `- 1` is the literal `-1`. An
integer literal outside the `int` range of [VALUE-5](values.md#value-5),
sign included, fails validation with `integer_overflow` at the field's path,
wherever it is written, once the whole text parses. A `float` literal beyond
the binary64 range is non-finite and becomes missing under
[VALUE-4](values.md#value-4).

`'C:\new'` contains a backslash, and a string literal is text under
[Values](values.md). The temporal keyword is required:
`'2025-06-01'` alone is a `str`, not a `date`.

A temporal literal that [Values](values.md) rejects fails with
`invalid_expression`. The finding keeps the literal kind and the applicable
temporal cause, which standalone syntax diagnostic context reports; a finding
in the original document keeps the field's text and, in a predicate, its
character position. Temporal operations outside the grammar keep their own
temporal conditions. Which literals a field admits is
[FORMULA-27](#formula-27)'s.

<a id="formula-27"></a>

### FORMULA-27. The field an expression is written in fixes its root and the constructs it admits

A predicate has a truth-valued root and admits comparisons, Boolean logic,
every literal, `str_contains`, and arithmetic and the numeric functions in an
operand. `compute.expr` has a numeric root over numbers, `NULL`, names,
arithmetic and the numeric functions. `aggregate.expr` admits what
`compute.expr` admits and the reducers. A reduction is refused outside
`aggregate.expr`, and a refused construct fails wherever it is written. A
call of a function or reducer its field does not admit fails, as does an
admitted function called with another number of arguments.

Fails: `prohibited_construct`, `prohibited_function`.
Evidence: `negative-formula-flag`, `negative-row-aggregate`, `yaml/grammar/expression.yaml`, `rust/crates/yamaa-adapters/tests/fixtures/grammar_diagnostics.tsv`.
Replaces: REQ-0407, REQ-0441, REQ-0512, REQ-1119, REQ-1121.

| Written in | Root | Admits | Refuses |
| --- | --- | --- | --- |
| a field typed `predicate` | a truth value, under [FORMULA-4](#formula-4) | comparisons, `IN`, `BETWEEN`, `LIKE`, `IS NULL`, `AND`, `OR`, `NOT`, every literal, `str_contains`, and in an operand arithmetic and the numeric functions | a reduction |
| `compute.expr` | a number for each current row | numbers, `NULL`, names, arithmetic and the numeric functions | comparisons and `IN`, `BETWEEN`, `LIKE` and `IS NULL`; `AND`, `OR`, `NOT`, `TRUE` and `FALSE`; a string or temporal literal; `str_contains`; a reduction |
| `aggregate.expr` | a number, or a reduction that is the whole expression and keeps the type it reduces ([FORMULA-23](#formula-23)) | what `compute.expr` admits, and the reducers of [FORMULA-18](#formula-18) | what `compute.expr` refuses, except a reduction |

A refused construct fails with `prohibited_construct`, naming it:
`comparison`, `boolean`, `string` or `temporal`. When a text holds several,
the one written first is reported. A call of a name outside the field's
functions, including a reducer outside `aggregate.expr` and `str_contains`
outside a predicate, fails with `prohibited_function` naming the function as
written; an admitted function called with another number of arguments also
reports the count. `SUM(AVAL)` in `compute.expr` fails with
`prohibited_function`, and `AVAL > ANRHI` in `compute.expr` fails with
`prohibited_construct`.

Arithmetic in a predicate has the numeric semantics of `compute`:
[FORMULA-12](#formula-12) types its operands, [FORMULA-14](#formula-14)
propagates missing and [FORMULA-15](#formula-15) fails it, and its result
compares under [FORMULA-5](#formula-5). `ALT > 3 * ULN` is `TRUE` for a row
whose `ALT` exceeds three times its `ULN`, and `UNKNOWN` when either is
missing.

`compute` evaluates `expr` over constructed output columns, fields of a
declared record lookup and numeric literals, and returns one numeric value per
current row. [Values](values.md)' representation, promotion and overflow rules
apply to every operator and function. `aggregate` reduces one relation under
[FORMULA-17](#formula-17).

<a id="formula-9"></a>

### FORMULA-9. An identifier names a value visible at its site, and any other name fails

Each site sees the names the table below gives, under
[Column derivation](columns.md), and `compute.expr` resolves a name as a
predicate does in the same phase. An identifier in a right-side predicate is
qualified by the right-side dataset's ID, and one over a completed or
candidate output row is unqualified. No expression reaches an undeclared
relation. Identifiers are collected for dependency ordering, so an unresolved
name fails rather than leaving an expression dependency-free.

Fails: `unresolvable_name`, `unknown_field`, `qualified_identifier`.
Evidence: `negative-review-unknown-date`, `negative-direct-dose-read`, `adam-adsl-bmi`, `rust/crates/yamaa-adapters/tests/fixtures/flag_reference.json`, `rust/crates/yamaa-adapters/tests/scalar_language.rs`.
Replaces: REQ-0178, REQ-0179, REQ-0180, REQ-0181, REQ-0182, REQ-0183,
REQ-0184, REQ-0185, REQ-0186, REQ-0189, REQ-0408, REQ-0409, REQ-0410,
REQ-0411, REQ-0412, REQ-0442, REQ-0443.

| Site | Visible names |
| --- | --- |
| Ungrouped row filter | The names [ROW-6](rows.md#row-6) defines |
| Grouped row filter | Only unqualified columns derived by that row |
| Aggregate, record-lookup and multiple-match filters | Records of their owning right-side dataset |
| Window filter | Constructed output columns in row construction; in an intermediate derivation, the augmented donor records [LOOKUP-15](lookup.md#lookup-15) describes |
| Verification | Completed columns and record intermediates resolved for the completed row |
| `case` | The values available to its enclosing derivation |
| `compute.expr` in column derivation | Constructed output columns, and fields of a declared record lookup |
| `compute.expr` in row construction | The names the paragraphs below give |
| `aggregate.expr` | The relation [LOOKUP-18](lookup.md#lookup-18) gives, and grouping columns ([FORMULA-22](#formula-22)) |

An enclosing column derivation may also bind a qualified dataset or
record-lookup field under [Name binding](names.md) and
[Lookup and joins](lookup.md). A verification may read a declared record
lookup for its completed row.

An identifier that is unavailable at its site fails. An unqualified
identifier that names a field of an in-scope dataset fails with
`unresolvable_name` and suggests the qualified spelling; any other unavailable
identifier fails with `unknown_field`. Both report under
[Column derivation](columns.md).

During column derivation an unqualified identifier in `compute.expr` is a
constructed output column, and a qualified identifier is permitted only when
its qualifier is a declared record lookup `id`; it reads the named field of
that lookup's selected record. An arbitrary `DATASET.VARIABLE` reference is
not permitted there: bind the source variable to a column first and compute
from that column. Omitting that binding column from `output.columns` keeps it
out of the final dataset. A qualified identifier whose qualifier is not a
declared record lookup fails with `qualified_identifier`, and an identifier
that resolves to neither a declared output column nor a field of a declared
record lookup fails with `unknown_field`.

During ungrouped row construction an identifier in `compute.expr` is either a
variable of the row template's input dataset, qualified exactly as
`row.filter` qualifies a variable, or an unqualified column derived by the
same `rows` entry. A value from another dataset reaches the expression only
through such a column, bound by a row-phase source or lookup under
[LOOKUP-2](lookup.md#lookup-2).

During grouped row construction an identifier qualified to the row template's
input dataset must be one of the enclosing `row.group_by` variables. Other
values are first reduced to a row-derived column with `aggregate`. A value
from another dataset is first bound to a row-derived column with a source or
lookup under [LOOKUP-2](lookup.md#lookup-2); the expression itself qualifies
no dataset but its own.

```yaml
- name: HEIGHTCM
  type: float
  derivation: DM.HEIGHTCM
- name: BMI
  type: float
  derivation:
    compute:
      expr: "WEIGHTKG / POWER(HEIGHTCM / 100, 2)"
```

A lookup field carries the type [Inputs](inputs.md) assigns to the field in
the lookup's dataset. [Column derivation](columns.md) collects the
identifiers of every expression and the lookup source dependencies, so a
`compute` derivation takes part in dependency ordering as a predicate does.

## Predicates

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

An operand whose arithmetic yields missing ([FORMULA-14](#formula-14)) is a
missing operand. [Verification](verification.md) defines every other
consequence of a predicate's result.

<a id="formula-5"></a>

### FORMULA-5. Two non-missing operands compare only when their runtime types are mutually comparable

Two non-missing operands compare only when
[VALUE-19](values.md#value-19) makes their runtime types mutually
comparable, in the order the table below gives, and no operand conversion
occurs. A mixed `int` and `float` pair compares by exact mathematical value.
Every other pair fails: a temporal value is not comparable to text, and a
`date` is not comparable to a `datetime`.

Fails: `incompatible_input_type`.
Evidence: `negative-review-date-text`, `rust/crates/yamaa-core/tests/fixtures/predicate_evaluation.tsv`.
Replaces: REQ-0167, REQ-0168, REQ-0169, REQ-0190.

| Types | Order |
|---|---|
| `int` with `int` | Exact integer order |
| `float` with `float` | Binary64 numeric order |
| Mixed `int` and `float` | Exact mathematical order under [VALUE-19](values.md#value-19) |
| `str` with `str` | [Values](values.md) text order |
| `date` with `date` | Chronological under [Values](values.md) |
| `datetime` with `datetime` | Chronological under [Values](values.md) |

Strings use the equality and total order [Values](values.md)
defines. `9007199254740993 = 9007199254740992.0` is `FALSE`. An arithmetic
operand has the numeric type of its result: `X + 0 = 9007199254740993` keeps
an exact `int`, while `X * 1.0` is a `float`. Two non-missing operands that are
not mutually comparable, or a non-string operand to `LIKE`
([FORMULA-7](#formula-7)), fail with `incompatible_input_type`.

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

Fails: `incompatible_input_type`, `invalid_expression`.
Evidence: `adam-adae-protocol-review`, `schema-predicates`, `rust/crates/yamaa-adapters/tests/fixtures/syntax_controls.json`.
Replaces: REQ-0176, REQ-0177, REQ-0191.

A non-string operand fails with `incompatible_input_type`. An invalid
`ESCAPE` literal, or a pattern with a dangling escape, fails with
`invalid_expression`: a literal pattern when the expression is admitted, and
a pattern read from a value when it is evaluated.

```yaml
filter: "AEDECOD LIKE '100!%' ESCAPE '!'"
```

<a id="formula-8"></a>

### FORMULA-8. str_contains is the one truth-valued function and searches its source for a portable regex

`str_contains(source, pattern)` takes any operand as `source` and a string
literal holding a portable regex as `pattern`. It is `TRUE` when the regex
finds a match anywhere in the source, `FALSE` when it does not, and `UNKNOWN`
when the source is missing, including under `NOT`. A non-`str` source fails
with `incompatible_input_type`; a pattern the portable regex contract rejects
fails with `invalid_expression` when the expression is admitted.

Fails: `incompatible_input_type`, `invalid_expression`, `prohibited_function`.
Evidence: `schema-str-contains`, `negative-str-contains-invalid-regex`.
Replaces: REQ-1244.

The portable regex contract is the one of
[Text operations](text.md#text-6). Each pattern is compiled once, when its
expression is admitted. Function-name matching is case-insensitive, so
`STR_CONTAINS(...)` is the same call, and a bare `str_contains` without `(`
stays an identifier. `str_contains` stands where a truth value belongs, in a
predicate only ([FORMULA-27](#formula-27)). Called with other than two
arguments it fails with `prohibited_function`, and a pattern that is not a
string literal fails with `invalid_expression`.

## Arithmetic

<a id="formula-12"></a>

### FORMULA-12. Every operand of arithmetic or of a numeric function must be numeric

An operand of `+`, `-`, `*`, `/`, a unary sign or a numeric function, and the
root of `compute.expr`, must have runtime type `int` or `float`, in every
field; an operand of any other type fails.
[VALUE-2](values.md#value-2) forbids implicit conversion between
operation inputs, and expressions do not relax that rule: a collected string
is converted by binding it to a numeric column first.

Fails: `incompatible_input_type`.
Evidence: `negative-review-arithmetic`, `rust/crates/yamaa-core/tests/fixtures/evaluation.tsv`, `rust/crates/yamaa-core/tests/fixtures/numeric_integral.tsv`.
Replaces: REQ-0307, REQ-0426, REQ-0444.

The failure reports the expression, the operand as written, `numeric` and the
operand's actual type, once however often the operand is written; a
comparison over that arithmetic reports nothing more. `ASTDT <= TRTEDT + 30`
over a `date` `TRTEDT` fails: carry the day count in its own column, for
example with `diff` ([TIME-4](temporal.md#time-4)), and compare that column.
The types of `aggregate.expr` follow [FORMULA-23](#formula-23).

<a id="formula-13"></a>

### FORMULA-13. The numeric functions are a closed list, each with a fixed argument count

The numeric functions are exactly those below, and each takes exactly the
arguments shown. Any other function name or argument count fails, and
`str_contains` ([FORMULA-8](#formula-8)) is the only other function. `LOG` is
excluded: write `LN(x)` or `LN(x) / LN(b)`. Widening the vocabulary amends
this list.

Fails: `prohibited_function`.
Evidence: `schema-numeric-functions`, `yaml/grammar/expression.yaml`, `rust/crates/yamaa-adapters/tests/fixtures/grammar_diagnostics.tsv`.
Replaces: REQ-0415, REQ-0416, REQ-0417, REQ-0425, REQ-0440.

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

A call of any other name fails with `prohibited_function` naming the function
as written, and a listed function called with another number of arguments
also reports the count. Every field admits the numeric functions; in a
predicate they stand in an operand ([FORMULA-27](#formula-27)).

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

In a predicate, a comparison whose arithmetic operand is missing is
`UNKNOWN` under [FORMULA-4](#formula-4); the logical operators and `IS NULL`
follow their own tables rather than this rule.

<a id="formula-15"></a>

### FORMULA-15. Division by zero, an invalid root, logarithm or power, and int overflow fail the run

Division by zero, by `/` or by `MOD`, `SQRT` of a negative argument, `LN` of a
zero or negative argument, `POWER` with a zero base and a negative exponent or
a negative base and a non-integer exponent, and an `int` result outside the
`int` range each fail the run, in every field. A failure is never silently
converted to missing, and `NULLIF(denominator, 0)` chooses missing
explicitly. A failure reports the expression and the operation that failed.

Fails: `division_by_zero`, `sqrt_of_negative`, `ln_of_nonpositive`, `invalid_power`, `integer_overflow`.
Evidence: `negative-zero-division`, `negative-negative-sqrt`, `negative-log-zero`, `negative-integer-overflow`.
Replaces: REQ-0429, REQ-0430, REQ-0431, REQ-0432, REQ-0433, REQ-0445,
REQ-0496, REQ-0513.

An invalid `POWER` fails with `invalid_power`, and an `int` result outside
the range fails with `integer_overflow` under [VALUE-5](values.md#value-5).
Failing rather than yielding missing is consistent with
[Column derivation](columns.md). In a predicate, a failed operand fails the
run rather than making its comparison `UNKNOWN`.

In `aggregate.expr`, arithmetic over reductions and inside a reduction's
argument fails the same way, and the failure reports the expression and the
column that failed. `SUM` fails on integer overflow under the same condition,
and `MEAN`, defined by `SUM`, fails on that intermediate overflow even when
the mathematical mean would fit. The non-finite normalization of
[Values](values.md) applies after every arithmetic or reduction result.

<a id="formula-16"></a>

### FORMULA-16. There is one rounding: half away from zero, with near ties treated as ties

Rounding to `digits` places, where a negative `digits` rounds left of the
decimal point, sends a value exactly halfway between two candidates, or within
`2^-26 * 10^-digits` below such a tie, away from zero; a result of zero is
positive zero. The `round_half_away_from_zero` verb, CSV display precision
([CSV-9](reference/csv.md#csv-9)), project-function result comparison
([FUNC-13](profiles/functions.md#func-13)) and second resolution in
`shift` of a datetime ([TIME-14](temporal.md#time-14)) all use it. No other
rounding exists.

Fails: `incompatible_input_type`.
Evidence: `schema-round-half-away-from-zero-ties`, `negative-round-half-away-from-zero-non-numeric-source`, `rust/crates/yamaa-adapters/tests/scalar_declarative_rounding.rs`.
Replaces: REQ-0418, REQ-0419, REQ-1172.

`2^-26` is `sqrt(2^-52)`. At two places, `2.675` (stored as
`2.674999999999999822...`, inside the window) rounds to `2.68`, and
`1.0049999999` (`1e-8` below the tie) rounds to `1.01`, while `1.004999999`
(`1e-7` below) rounds to `1.00`. The `round_half_away_from_zero` verb takes a numeric `source` and an
integer `digits`; missing stays missing and the result is a `float`. The
expression grammar has no rounding function, and a derivation must not round
by any other spelling. A non-numeric source is an `incompatible_input_type`
validation error. Analysis datasets otherwise carry
computed values at full precision; reporting decides the displayed places.
[Values](values.md) has the same rule at conversion, where
a non-integral value fails rather than being truncated.

`CEIL`, `FLOOR` and `TRUNC` are not presentation rounding: they return an
integral part, with no mode to choose. `FLOOR(a / b)` floors the binary64
quotient and returns an integral `float`, both under
[VALUE-10](values.md#value-10). It need not equal the floor of the exact
integer quotient.

## Aggregate expressions

<a id="formula-17"></a>

### FORMULA-17. aggregate reduces one relation to one value with a closed reducer expression

`aggregate` is the only aggregate expression. It reduces one relation, or the
current input group, to one value with `expr`, which admits the reducers
([FORMULA-27](#formula-27)), in one of the relation forms
[LOOKUP-18](lookup.md#lookup-18) permits. A reduction that is the whole
expression keeps the type it reduces, and any other expression is numeric
([FORMULA-23](#formula-23)).

Evidence: `schema-aggregation`, `rust/crates/yamaa-core/tests/numeric_aggregate_parser.rs`.
Replaces: REQ-0295, REQ-1091, REQ-1092.

The grammar is [FORMULA-1](#formula-1)'s, and its vocabulary closes the
reducers of [FORMULA-18](#formula-18). `100 * SUM(EX.EXDOSE) / EX.EXPLDOS`
divides a reduction by a grouping column ([FORMULA-22](#formula-22)).

<a id="formula-18"></a>

### FORMULA-18. The reducers are exactly SUM, COUNT, MIN, MAX, MEAN and ONLY

A reduction applies one of the reducers below to one expression, or `COUNT` to
`D.*`. Any other reducer name or argument count fails. `AVG` is not an alias:
the reducer name is `MEAN`, and no median is registered. Any new reduction
amends this table.

Fails: `prohibited_function`, `invalid_expression`.
Evidence: `schema-aggregation`, `adam-adlb-mean`, `rust/crates/yamaa-core/tests/numeric_aggregate_parser.rs`.
Replaces: REQ-0477, REQ-0478, REQ-0481, REQ-0484, REQ-0500, REQ-0505.

| Reducer | Result |
|---|---|
| `SUM(x)` | total of the non-missing values |
| `COUNT(x)` | how many values of `x` are non-missing |
| `COUNT(D.*)` | how many records the group contains |
| `MIN(x)` | smallest non-missing value |
| `MAX(x)` | largest non-missing value |
| `MEAN(x)` | arithmetic mean of the non-missing numeric values |
| `ONLY(x)` | value from the group's only record; more than one fails |

A name outside the reducers and functions, such as `AVG(x)`, fails with
`prohibited_function`. A reducer written with other than one argument, and a
record star outside `COUNT`, fail with `invalid_expression`.

`COUNT(D.*)` takes no other argument. `D` stands for the relation named by the
expression's qualified identifiers, as in `COUNT(EX.*)`, and a `COUNT(D.*)`
whose dataset is not the expression's relation fails. It is the one reducer
that names no column, and it counts records where `COUNT(x)` counts values.

A reduction over text compares text. `MIN` and `MAX` of a `str` argument
order its values by [VALUE-20](values.md#value-20), not as dates, so over
collected date text ([TIME-7](temporal.md#time-7)) `2025-01` sorts before
`2025-01-08` and `2025-01-08` before `2025-01-08T06:00`: the least text need
not be the earliest date, and completing it afterwards can leave the result
missing. A reduction that means the earliest or latest date completes each
record first, with an aggregate derive binding
([LOOKUP-22](lookup.md#lookup-22)) of type `date` or `datetime` whose
derivation is `impute` ([TIME-8](temporal.md#time-8)), and reduces the bound
values, which compare chronologically under
[VALUE-21](values.md#value-21).

<a id="formula-19"></a>

### FORMULA-19. SUM adds in relation record order, and MEAN divides that SUM by COUNT

`SUM(x)` adds the non-missing argument values in relation record order,
starting with the first such value and adding each later value with the
numeric `+`. Summation is never reordered, reassociated or partitioned, and
never compensated or correctly rounded. For a group with at least one
non-missing value, `MEAN(x)` is that ordered `SUM(x)` divided by `COUNT(x)`
with the numeric `/`, and it inherits the binary64 rounding of `SUM`. A
reduction does not sort the records.

Evidence: `adam-adlb-ordered-sum`, `rust/crates/yamaa-core/tests/fixtures/reduction.tsv`, `rust/crates/yamaa-adapters/tests/execution_value_semantics.rs::sum_adds_matched_donor_records_in_stored_order_and_mean_divides_that_sum`.
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

Fails: `aggregate_multiple_records`.
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

Fails: `nested_reduction`.
Evidence: `yaml/grammar/expression.yaml`, `rust/crates/yamaa-core/tests/numeric_aggregate_parser.rs`, `rust/crates/yamaa-adapters/tests/fixtures/syntax_controls.json`.
Replaces: REQ-0483, REQ-0502.

The link through `dataset_class.schema`
([INPUT-10](inputs.md#input-10)) types the stored fields from the
producer's output contract. Each level is its own domain build: the caller
builds and saves the first specification, and the downstream build reads the
saved artifact as an ordinary input under
[FILE-3](reference/files.md#file-3). The producer's selected columns must
declare their types and labels under [INPUT-11](inputs.md#input-11).

For example, `daily.yaml` can group exposure records by subject, treatment
and day and derive `ADOSE: {aggregate: "SUM(EX.EXDOSE)"}`. A downstream
specification declares `DAILY: {path: daily.csv, schema: daily.yaml}` and is
built after `daily.yaml` has saved that `daily.csv`. It groups
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

Fails: `aggregate_identifier_not_grouped`.
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

Fails: `incompatible_input_type`, `incomparable_sources`.
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
result, as it does for every other expression. These types, over the declared
types of the relation's fields and derive bindings, give `aggregate` its
result type ([EXPR-14](expressions.md#expr-14)).

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
| No record in the group after `filter` | missing, uncounted: an aggregate declares no handler ([COLUMN-11](columns.md#column-11)) |
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

## Evaluation

<a id="formula-26"></a>

### FORMULA-26. Expressions evaluate deterministically and exactly as written

Evaluating an expression is deterministic and free of side effects, in every
field. Evaluation follows the written association: no expression is
reassociated, redistributed or algebraically simplified, and no numeric option
that does any of these is enabled. `/` never truncates. String comparison uses
[Values](values.md), and no implicit coercion, collation, `LIKE` escape or
missing-value behavior is taken from a host SQL engine.

Evidence: `rust/crates/yamaa-core/tests/fixtures/arithmetic.tsv`, `rust/crates/yamaa-core/tests/portable_math_golden.rs`, `tools/tests/test_portable_math.py`.
Replaces: REQ-0187, REQ-0436, REQ-0437, REQ-0438, REQ-0497.

`a / (b * b)` and `a / b / b` are different formulas and may return different
doubles. Both are correct, and evaluation returns the double for the written
formula. Language or engine settings that make division integral are
overridden. A host SQL engine's coercion, collation, `LIKE` escape and
missing-value behaviors are configured and overridden to match these rules, or
the grammar is evaluated directly.
