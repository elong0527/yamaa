# Formulas

One closed expression language is written inside a specification: in fields
typed `predicate`, in `aggregate.expr` and as text in an expression field
([EXPR-5](expressions.md#expr-5)). Its grammar has
one source under [yaml/grammar/](../yaml/grammar/README.md), rendered into
[FORMULA-1](#formula-1), and the field a text is written in decides the root
it has and what it may use ([FORMULA-27](#formula-27)). String templates keep
their own grammar ([TEXT-3](text.md#text-3)).

## The language

<a id="formula-1"></a>

### FORMULA-1. An expression is one text in the closed expression grammar

Fields typed `predicate`, `aggregate.expr` and expression fields written as
text ([EXPR-5](expressions.md#expr-5)) each hold one text in the grammar
below. Comparisons, Boolean logic, arithmetic, the closed functions and the
reducers are the whole language: `CASE`, windows, subqueries, host-language
calls and `!=` are not in it. The whole text is read, and checked against its
field, before any name is resolved or value read. Text that does not parse as
one expression fails, as does a truth value where a value belongs or a value
where a truth value belongs.

Fails: `invalid_expression`.
Evidence: `schema-predicates`, `schema-numeric-functions`, `schema-aggregation`, `negative-flag-condition`, `yaml/grammar/expression.yaml`.

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
Evidence: `yaml/grammar/expression.yaml`, `rust/crates/yamaa-application/tests/fixtures/predicate_diagnostics.tsv`, `rust/crates/yamaa-core/tests/fixtures/numeric_literals.tsv`.

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
operand. `aggregate.expr` admits numbers, `NULL`, names, arithmetic, the
numeric functions and the reducers. Text in an expression field
([EXPR-5](expressions.md#expr-5)) admits the same except the reducers, plus
quoted text and temporal literals. A refused construct fails wherever it is
written. A call of a function or reducer its field does not admit fails, as
does an admitted function called with another number of arguments.

Fails: `prohibited_construct`, `prohibited_function`.
Evidence: `negative-formula-flag`, `negative-row-aggregate`, `yaml/grammar/expression.yaml`, `rust/crates/yamaa-application/tests/fixtures/grammar_diagnostics.tsv`.

| Written in | Root | Admits | Refuses |
| --- | --- | --- | --- |
| a field typed `predicate` | a truth value, under [FORMULA-4](#formula-4) | comparisons, `IN`, `BETWEEN`, `LIKE`, `IS NULL`, `AND`, `OR`, `NOT`, every literal, `str_contains`, and in an operand arithmetic and the numeric functions | a reduction |
| an expression field: `derivation`, a `derivations` entry, `then`, `otherwise`, a handled wrapper's `value`, a key match value and a nearest selection's `to` | any value, for each current row | numbers, quoted text, temporal literals, `NULL`, names, arithmetic and the numeric functions | comparisons and `IN`, `BETWEEN`, `LIKE` and `IS NULL`; `AND`, `OR`, `NOT`, `TRUE` and `FALSE`; `str_contains`; a reduction |
| `aggregate.expr` | a number, or a reduction that is the whole expression and keeps the type it reduces ([FORMULA-23](#formula-23)) | numbers, `NULL`, names, arithmetic, the numeric functions and the reducers of [FORMULA-18](#formula-18) | comparisons and `IN`, `BETWEEN`, `LIKE` and `IS NULL`; `AND`, `OR`, `NOT`, `TRUE` and `FALSE`; a string or temporal literal; `str_contains` |

A function argument is also an expression field, and it admits one leaf
([FUNC-6](profiles/functions.md#func-6)).

A refused construct fails with `prohibited_construct`, naming it:
`comparison`, `boolean`, or, in `aggregate.expr`, `string` or `temporal`.
When a text holds several, the one written first is reported. A call of a
name outside the field's functions, including a reducer outside
`aggregate.expr` and `str_contains` outside a predicate, fails with
`prohibited_function` naming the function as written; an admitted function
called with another number of arguments also reports the count. `SUM(AVAL)`
in an expression field fails with `prohibited_function`, and `AVAL > ANRHI`
there fails with `prohibited_construct`.

Arithmetic in a predicate has the numeric semantics of expression-field text:
[FORMULA-12](#formula-12) types its operands, [FORMULA-14](#formula-14)
propagates missing and [FORMULA-15](#formula-15) fails it, and its result
compares under [FORMULA-5](#formula-5). `ALT > 3 * ULN` is `TRUE` for a row
whose `ALT` exceeds three times its `ULN`, and `UNKNOWN` when either is
missing.

Text in an expression field that holds only a name or a literal returns it
([EXPR-4](expressions.md#expr-4)), and other text returns one value per
current row. [Values](values.md)' representation, promotion and overflow rules
apply to every operator and function. `aggregate` reduces one relation under
[FORMULA-17](#formula-17).

<a id="formula-9"></a>

### FORMULA-9. An identifier names a value visible at its site, and any other name fails

Each site sees the names the table below gives, under
[Column derivation](columns.md), and text in an expression field resolves a
name as a predicate does in the same phase, except that text holding only a
name is a reference ([EXPR-4](expressions.md#expr-4)). An identifier in a
right-side predicate is qualified by the right-side dataset's ID, and one over
a completed or candidate output row is unqualified. No expression reaches an
undeclared relation. Identifiers are collected for dependency ordering, so an
unresolved name fails rather than leaving an expression dependency-free.

Fails: `unresolvable_name`, `unknown_field`, `qualified_identifier`.
Evidence: `negative-review-unknown-date`, `negative-direct-dose-read`, `adam-adsl-bmi`, `rust/crates/yamaa-application/tests/fixtures/flag_reference.json`, `rust/crates/yamaa-application/tests/scalar_language.rs`.

| Site | Visible names |
| --- | --- |
| Ungrouped row filter | The names [ROW-6](rows.md#row-6) defines |
| Grouped row filter | Only unqualified columns derived by that row |
| Aggregate, record-lookup and multiple-match filters | Records of their owning right-side dataset |
| Window filter | Constructed output columns in row construction; in an intermediate derivation, the augmented donor records [LOOKUP-15](lookup.md#lookup-15) describes |
| Verification | Completed columns and record intermediates resolved for the completed row; completed columns alone in a `not_missing`'s `when` ([CHECK-5](verification.md#check-5)) |
| `case` | The values available to its enclosing derivation |
| Expression-field text in column derivation | Constructed output columns, and fields of a declared record lookup |
| Expression-field text in row construction | The names the paragraphs below give |
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

Text in an expression field that holds only a name is a reference: a
dataset-qualified one, such as `derivation: DM.SEX`, reads its variable
through the implicit join of [LOOKUP-1](lookup.md#lookup-1). During column
derivation, an unqualified identifier in any other expression-field text is a
constructed output column, and a qualified identifier is permitted only when
its qualifier is a declared record lookup `id`; it reads the named field of
that lookup's selected record. An arbitrary `DATASET.VARIABLE` reference is
not permitted there: bind the source variable to a column first and compute
from that column. Omitting that binding column from `output.columns` keeps it
out of the final dataset. A qualified identifier whose qualifier is not a
declared record lookup fails with `qualified_identifier`, and an identifier
that resolves to neither a declared output column nor a field of a declared
record lookup fails with `unknown_field`.

During ungrouped row construction an identifier in expression-field text is
either a variable of the row template's input dataset, qualified exactly as
`row.filter` qualifies a variable, or an unqualified column derived by the
same `rows` entry. A value from another dataset reaches the expression only
through such a column, which a source or lookup binds during row construction
under [LOOKUP-2](lookup.md#lookup-2).

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
  derivation: "WEIGHTKG / POWER(HEIGHTCM / 100, 2)"
```

A lookup field carries the type [Inputs](inputs.md) assigns to the field in
the lookup's dataset. [Column derivation](columns.md) collects the
identifiers of every expression and the lookup source dependencies, so
expression-field text takes part in dependency ordering as a predicate does.

## Predicates

<a id="formula-4"></a>

### FORMULA-4. A predicate is TRUE, FALSE or UNKNOWN, and only TRUE retains a row or record

A predicate evaluates to `TRUE`, `FALSE` or `UNKNOWN` under the three-valued
tables below. `NULL` is missing and has no runtime type. A comparison with a
missing operand is `UNKNOWN`; `IS NULL` and `IS NOT NULL` test for
missingness and are never `UNKNOWN`. A filter retains a row or record only for
`TRUE`, and a verification holds only for `TRUE`.

Evidence: `rust/crates/yamaa-core/tests/fixtures/predicate_evaluation.tsv`, `schema-predicates`.

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

Fails: `invalid_expression`.
Evidence: `adam-adae-protocol-review`, `schema-predicates`, `rust/crates/yamaa-application/tests/fixtures/syntax_controls.json`.

A non-string operand fails with `incompatible_input_type` under
[FORMULA-5](#formula-5). An invalid
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

An operand of `+`, `-`, `*`, `/`, a unary sign or a numeric function must
have runtime type `int` or `float`, in every field; an operand of any other
type fails. [VALUE-2](values.md#value-2) forbids implicit conversion between
operation inputs, and expressions do not relax that rule: a collected string
is converted by binding it to a numeric column first.

Fails: `incompatible_input_type`.
Evidence: `negative-review-arithmetic`, `rust/crates/yamaa-core/tests/fixtures/evaluation.tsv`, `rust/crates/yamaa-core/tests/fixtures/numeric_integral.tsv`.

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
excluded: write `LOG10(x)` for the base-10 logarithm, `LN(x)` for the natural
one and `LN(x) / LN(b)` for another base. Widening the vocabulary amends this
list.

Fails: `prohibited_function`.
Evidence: `schema-numeric-functions`, `adam-adlb-reference-limits`, `yaml/grammar/expression.yaml`, `rust/crates/yamaa-application/tests/fixtures/grammar_diagnostics.tsv`, `yaml/conformance/expressions.yaml`, `rust/crates/yamaa-core/tests/fixtures/portable_math.tsv`.

- `ABS(x)`: absolute value.
- `CEIL(x)`: least integer value not less than `x`.
- `FLOOR(x)`: greatest integer value not greater than `x`.
- `TRUNC(x)`: `x` with its fractional part removed, toward zero.
- `SQRT(x)`: non-negative square root.
- `POWER(x, y)`: `x` raised to `y`.
- `EXP(x)`: `e` raised to `x`.
- `LN(x)`: natural logarithm.
- `LOG10(x)`: base-10 logarithm.
- `NORM_CDF(x)`: the standard normal distribution function, the probability
  that a standard normal variable is at most `x`.
- `NORM_INV(p)`: the standard normal quantile, the `x` whose `NORM_CDF(x)` is
  `p`, for `p` strictly between 0 and 1.
- `MOD(x, y)`: remainder of `x / y`, taking the sign of `x`.
- `NULLIF(x, y)`: `NULL` when `x = y`, otherwise `x`.

A call of any other name fails with `prohibited_function` naming the function
as written, and a listed function called with another number of arguments
also reports the count. Every field admits the numeric functions; in a
predicate they stand in an operand ([FORMULA-27](#formula-27)).

Each function is one computation, the same binary64 operations on every
platform, so it returns the same bits everywhere. `SQRT` is the correctly
rounded square root, and `EXP`, `LN`, `LOG10` and `POWER` are the `exp`,
`log`, `log10` and `pow` of the pinned Rust `libm` crate, a port of the musl
C library. Three are defined further:

- `LOG10(x)` is `log10` itself, never a quotient of natural logarithms:
  `LOG10(1000)` is exactly 3, while `LN(1000) / LN(10)` is
  2.9999999999999996.
- `NORM_CDF(x)` is `0.5 * erfc(-x / SQRT(2))`, with the pinned `erfc` and
  `SQRT(2)` the binary64 value 1.4142135623730951, each operation rounded to
  binary64 in that order. It is 0.5 at 0 and lies from 0 to 1: it rounds to
  1 above about 8.3 and to 0 below about -38.5.
- `NORM_INV(p)` is algorithm AS 241 of Wichura (Applied Statistics 37(3),
  1988), its double-precision routine PPND16: a rational approximation in
  three regions, with no iteration, accurate to about 1 part in 10^16. With
  `q = p - 0.5`, where `|q| <= 0.425` it is `q * A(r) / B(r)` at
  `r = 0.180625 - q * q`. Elsewhere `r = SQRT(-LN(m))`, with `m` the smaller
  of `p` and `1 - p`, and it is `C(r - 1.6) / D(r - 1.6)` where `r <= 5` and
  `E(r - 5) / F(r - 5)` beyond, negated where `q < 0`. Each of `A` to `F` is
  the published polynomial of degree 7, evaluated by Horner's rule from its
  highest coefficient, each step one binary64 operation, with the `LN` and
  `SQRT` above. `NORM_INV(0.5)` is 0, and `NORM_INV(NORM_CDF(1))` is 1 to 15
  significant digits.

No function in this grammar picks a row-wise extreme or the first present
value. Those are the `greatest`, `least` and `first_available` expressions
([EXPR-9](expressions.md#expr-9), [EXPR-10](expressions.md#expr-10)), and
this grammar is not widened to reach them.

<a id="formula-14"></a>

### FORMULA-14. A missing operand makes arithmetic and numeric functions missing, except NULLIF's second argument

An arithmetic operator or a numeric function with a missing operand yields
missing, with one exception: `NULLIF(x, y)` yields `x` when only `y` is
missing ([FORMULA-13](#formula-13)). A derivation therefore needs no guarding
predicate to survive a missing input, and a formula that must yield missing
rather than fail uses `NULLIF`: percentage change against a zero base is
`100 * (VALUE - BASE) / NULLIF(BASE, 0)`. Predicates and their logical
operators follow [FORMULA-4](#formula-4) instead.

Evidence: `rust/crates/yamaa-core/tests/fixtures/numeric_selection.tsv`, `rust/crates/yamaa-core/tests/fixtures/evaluation.tsv`, `adam-adsl-bmi`.

In a predicate, a comparison whose arithmetic operand is missing is
`UNKNOWN` under [FORMULA-4](#formula-4); the logical operators and `IS NULL`
follow their own tables rather than this rule.

<a id="formula-15"></a>

### FORMULA-15. Division by zero, an invalid root, logarithm, power or probability, and int overflow fail the run

Division by zero, by `/` or by `MOD`, `SQRT` of a negative argument, a
logarithm of a zero or negative argument, `POWER` with a zero base and a
negative exponent or a negative base and a non-integer exponent, `NORM_INV`
of a value outside the open interval from 0 to 1, and an `int` result outside
the `int` range each fail the run, in every field. A failure is never
silently converted to missing, and `NULLIF(denominator, 0)` chooses missing
explicitly. A failure reports the expression and the operation that failed.

Fails: `division_by_zero`, `sqrt_of_negative`, `ln_of_nonpositive`, `invalid_power`, `invalid_probability`, `integer_overflow`.
Evidence: `negative-zero-division`, `negative-negative-sqrt`, `negative-log-zero`, `negative-log10-zero`, `negative-integer-overflow`, `negative-power-negative-base`, `negative-norm-inv-one`, `negative-geomean-zero`, `rust/crates/yamaa-core/tests/fixtures/reduction.tsv`, `rust/crates/yamaa-core/tests/fixtures/portable_math.tsv`.

A logarithm of `LN` or `LOG10` fails with `ln_of_nonpositive`, naming the
function. `NORM_INV` fails with `invalid_probability`, naming the function
and reporting the value, also at exactly 0 and 1, whose quantiles are
infinite, and so at a `NORM_CDF` result that rounds to 1
([FORMULA-13](#formula-13)). An invalid `POWER` fails with `invalid_power`,
and an `int` result outside the range fails with `integer_overflow` under
[VALUE-5](values.md#value-5).
Failing rather than yielding missing is consistent with
[Column derivation](columns.md). In a predicate, a failed operand fails the
run rather than making its comparison `UNKNOWN`.

In `aggregate.expr`, arithmetic over reductions and inside a reduction's
argument fails the same way, and the failure reports the expression and the
column that failed. `SUM` fails on integer overflow under the same condition,
and `MEAN`, defined by `SUM`, fails on that intermediate overflow even when
the mathematical mean would fit, as `VARIANCE` and `STDDEV` do through their
`MEAN`. A quantile fails when the `int` difference of the two values it
interpolates between leaves the `int` range ([FORMULA-18](#formula-18)), and
`GEOMEAN` fails with `ln_of_nonpositive`, naming `GEOMEAN`, on a zero or
negative value, which its `LN` reads ([FORMULA-19](#formula-19)). The non-finite normalization of
[Values](values.md) applies after every arithmetic or reduction result, so a
step of a statistical reducer that leaves binary64 makes its result missing.

<a id="formula-16"></a>

### FORMULA-16. There is one rounding: half away from zero, with near ties treated as ties

Rounding to `digits` places, where a negative `digits` rounds left of the
decimal point, sends a value exactly halfway between two candidates, or within
`2^-26 * 10^-digits` below such a tie, away from zero; a result of zero is
positive zero. The `round_half_away_from_zero` verb, CSV display precision
([CSV-9](reference/csv.md#csv-9)), fixed-decimal text
([TEXT-15](text.md#text-15)), project-function result comparison
([FUNC-13](profiles/functions.md#func-13)) and second resolution in
`shift` of a datetime ([TIME-14](temporal.md#time-14)) all use it. No other
rounding exists.

Fails: `incompatible_input_type`.
Evidence: `schema-round-half-away-from-zero-ties`, `negative-round-half-away-from-zero-non-numeric-source`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/rounding.json::malformed_digits_precede_unknown_source_and_missing_study_data`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/rounding.json::a_driver_qualified_rounding_source_keeps_the_independent_rows`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/rounding.json::a_rounding_source_read_through_a_named_intermediate_keeps_the_independent_rows`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/rounding.json::rounding_nested_in_a_case_result_keeps_the_independent_rows`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/rounding.json::rounding_in_a_row_template_keeps_the_independent_rows`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/rounding.json::unselected_case_uses_static_known_type_validation_before_study_capture`.

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

The grammar is [FORMULA-1](#formula-1)'s, and its vocabulary closes the
reducers of [FORMULA-18](#formula-18). `100 * SUM(EX.EXDOSE) / EX.EXPLDOS`
divides a reduction by a grouping column ([FORMULA-22](#formula-22)).

<a id="formula-18"></a>

### FORMULA-18. The reducers are exactly SUM, COUNT, MIN, MAX, MEAN, ONLY, STR_JOIN, MEDIAN, Q1, Q3, QUANTILE, STDDEV, VARIANCE and GEOMEAN

A reduction applies one of the reducers below to one expression, or `COUNT` to
`D.*`. Any other reducer name or argument count fails. `AVG` is not an alias:
the reducer name is `MEAN`. A field of the aggregate that a reducer reads is
written only beside a reducer that reads it, and beside every reducer that
needs it, as the field table below pairs them. Any new reduction amends this
table.

Fails: `prohibited_function`, `invalid_expression`, `unpaired_fields`, `invalid_field_type`, `engine_rejected`.
Evidence: `schema-aggregation`, `adam-adlb-mean`, `adam-adcm-related-events`, `negative-join-unordered`, `negative-sum-order-by`, `negative-sum-distinct`, `adam-adsl-bmi-quartile-group`, `adam-adpp-geometric-mean-ratio`, `negative-quantile-out-of-range`, `negative-quantile-unset`, `negative-mean-quantile`, `rust/crates/yamaa-core/tests/numeric_aggregate_parser.rs`, `rust/crates/yamaa-core/tests/scalar_aggregate_reduction.rs::str_join_writes_present_values_as_column_text_in_the_given_order`, `rust/crates/yamaa-core/tests/scalar_aggregate_reduction.rs::count_with_distinct_counts_each_different_present_value_once`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/aggregates.json::a_str_join_without_its_fields_misses_each_in_the_tables_order`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/aggregates.json::a_quantile_above_one_fails_at_the_field_before_any_record`, `rust/crates/yamaa-core/tests/fixtures/reduction.tsv`, `yaml/conformance/expressions.yaml`, `rust/crates/yamaa-core/tests/text_value_limit.rs`.

| Reducer | Result |
|---|---|
| `SUM(x)` | total of the non-missing values |
| `COUNT(x)` | how many values of `x` are non-missing; with `distinct`, how many different ones |
| `COUNT(D.*)` | how many records the group contains |
| `MIN(x)` | smallest non-missing value |
| `MAX(x)` | largest non-missing value |
| `MEAN(x)` | arithmetic mean of the non-missing numeric values |
| `ONLY(x)` | value from the group's only record; more than one fails |
| `STR_JOIN(x)` | the non-missing values of `x` as text, in the aggregate's `order_by` order, its `separator` between two; with `distinct`, each text once |
| `MEDIAN(x)` | middle of the non-missing numeric values: `QUANTILE` at 0.5 |
| `Q1(x)` | first quartile of the non-missing numeric values: `QUANTILE` at 0.25 |
| `Q3(x)` | third quartile of the non-missing numeric values: `QUANTILE` at 0.75 |
| `QUANTILE(x)` | quantile of the non-missing numeric values at the aggregate's `quantile`, R's default type 7 |
| `STDDEV(x)` | sample standard deviation of the non-missing numeric values, with divisor `n - 1` |
| `VARIANCE(x)` | sample variance of the non-missing numeric values, with divisor `n - 1` |
| `GEOMEAN(x)` | geometric mean of the non-missing values, each greater than zero |

A name outside the reducers and functions, such as `AVG(x)`, fails with
`prohibited_function`. A reducer written with other than one argument, and a
record star outside `COUNT`, fail with `invalid_expression`, so
`STR_JOIN(SEQN, ', ')` fails there: the separator is a field of the aggregate,
not an argument, and `COUNT(DISTINCT AETERM)` fails the same way.

A reducer reads the fields of the aggregate below besides its argument. A
field written beside an expression that does not read it, and a field that a
reducer needs and lacks, fail validation before any input is read as
`unpaired_fields` under [LOOKUP-10](lookup.md#lookup-10), at the aggregate:
`declared` names what is written and `missing` what it lacks, a reducer in the
form this table writes it. Any new field of a reducer amends this table.

| Field | Read by | Needed beside it |
|---|---|---|
| `order_by` | `STR_JOIN(x)` | yes, with at least one term |
| `separator` | `STR_JOIN(x)` | yes |
| `distinct` | `STR_JOIN(x)` or `COUNT(x)` written as the whole expression | no; omitted, every value counts |
| `quantile` | `QUANTILE(x)` | yes, a number from 0 through 1 |

A reducer that lacks fields it needs is declared, and `missing` lists every
field it lacks. Fields written beside no reducer that reads them are
declared together, one finding for the fields of the same readers, and
`missing` lists every reader, since any one of them would pair them. Each
list follows this table: fields in its row order, readers in the order its
`Read by` column names them.

`order_by` beside `SUM(AVAL)` reports `order_by` declared and `STR_JOIN(x)`
missing, and `STR_JOIN(SEQN)` with neither field reports `STR_JOIN(x)`
declared and `order_by` and `separator` missing. `distinct` beside any other
expression, such as `SUM(x)`, `COUNT(D.*)` or `COUNT(x) + 1`, reports
`distinct` declared and `STR_JOIN(x)` and `COUNT(x)` missing. A `STR_JOIN`
written inside arithmetic still reads `order_by` and `separator`, and the
arithmetic fails under [FORMULA-23](#formula-23).

`STR_JOIN(x)` writes each non-missing value of `x` as the text a `str` column
converts it to ([VALUE-3](values.md#value-3)): an `int` as decimal text, a
`float` as the shortest positional text of [VALUE-8](values.md#value-8), a
`date` or `datetime` as the canonical text of [VALUE-17](values.md#value-17),
and text as it is. It is the one reduction that converts its argument. It
joins the values in the aggregate's `order_by` order
([LOOKUP-10](lookup.md#lookup-10)), writing `separator`, which may be the
empty string, between two of them; with `distinct: true` it writes each text
once, where it first occurs in that order, comparing the written text scalar
for scalar ([VALUE-12](values.md#value-12)). `COUNT(x)` with `distinct: true`
counts the different non-missing values of `x`: text equal scalar for scalar,
numbers by exact value, an `int` and a `float` alike
([VALUE-19](values.md#value-19)), and dates and datetimes on the day or second
they name ([VALUE-21](values.md#value-21)). `COUNT(D.*)` counts records and
takes no `distinct`. A joined text is at most the 1 MiB of
[TEXT-2](text.md#text-2): a longer one refuses the run before it is built, as
`engine_rejected` with `resource` `text_bytes` and the path of the
aggregate's `expr`, and a shorter value too long for an output is refused
there, as any other text is, by [XPT-7](profiles/xpt.md#xpt-7) and
[SUB-8](profiles/submission.md#sub-8).

```yaml
- name: AESEQS
  type: str
  derivation:
    aggregate:
      key: {USUBJID: USUBJID, RELID: RELID}
      filter: "RELREC.RDOMAIN = 'AE' AND RELREC.IDVAR = 'AESEQ'"
      derivations:
        SEQN: {type: int, derivation: RELREC.IDVARVAL}
      order_by: [SEQN]
      separator: ", "
      expr: "STR_JOIN(SEQN)"
```

The related records store the sequence numbers as text, `10` before `2`, and
the column holds `2, 10`: the binding makes them numbers, which `order_by`
sorts numerically.

`MEDIAN`, `Q1`, `Q3`, `QUANTILE`, `STDDEV`, `VARIANCE` and `GEOMEAN` are the
statistical reducers. Each has the one definition this card or
[FORMULA-19](#formula-19) gives it, and returns a `float`
([FORMULA-23](#formula-23)).

`QUANTILE(x)` is R's default quantile, type 7 of Hyndman and Fan, at the
probability `p` that the aggregate's `quantile` gives. Its `n` non-missing
values sort ascending by exact value ([VALUE-19](values.md#value-19)), a
stable sort that keeps equal values in relation record order, as `x[1]` to
`x[n]`. With `h = (n - 1) * p + 1`, computed with the binary64 `*` and then
`+`, `j` the integer part of `h` and `g = h - j`, the result is `x[j]` when
`g` is 0, as it is when `j = n`, and otherwise `x[j] + g * (x[j+1] - x[j])`,
evaluated in that written form with the numeric `-`, `*` and `+`
([VALUE-10](values.md#value-10)). An `int` result widens to a `float`
([VALUE-8](values.md#value-8)), and an `int` difference outside the `int`
range fails ([FORMULA-15](#formula-15)). One value gives that value. Another
implementation of type 7 may arrange the interpolation otherwise and round
differently in the last place; this form is the one evaluated.

`MEDIAN`, `Q1` and `Q3` are `QUANTILE` at 0.5, 0.25 and 0.75, written without
the field. The median is the middle value, or for an even count
`x[j] + 0.5 * (x[j+1] - x[j])`, between the two middle values. A `quantile`
outside 0 through 1, such as `1.5`, fails validation as `invalid_field_type`
at the field, before any input is read.

```yaml
- name: BMIQ1
  type: float
  derivation:
    aggregate:
      group_by: [STUDYID]
      expr: "Q1(BMIBL)"

- name: BMIP90
  type: float
  derivation:
    aggregate:
      group_by: [STUDYID]
      quantile: 0.9
      expr: "QUANTILE(BMIBL)"
```

On the values 2, 4, 5, 7, 9 and 10, `MEDIAN` is 6, `Q1` is 4.25 and `Q3` is
8.5. SAS's five `PCTLDEF` definitions include none equal to type 7: its
default, `PCTLDEF=5`, returns an observed value or the midpoint of two, so on
those values it gives a first quartile of 4 and a third of 9. The median is the
same value under both definitions for every n. No other definition is
offered.

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
record first, with an entry of the aggregate's `derivations`
([LOOKUP-22](lookup.md#lookup-22)) of type `date` or `datetime` whose
derivation is `impute` ([TIME-8](temporal.md#time-8)), and reduces the bound
values, which compare chronologically under
[VALUE-21](values.md#value-21).

<a id="formula-19"></a>

### FORMULA-19. SUM adds in relation record order, MEAN divides that SUM by COUNT, and VARIANCE, STDDEV and GEOMEAN add in the same order

`SUM(x)` adds the non-missing argument values in relation record order,
starting with the first such value and adding each later value with the
numeric `+`. Summation is never reordered, reassociated or partitioned, and
never compensated or correctly rounded. For a group with at least one
non-missing value, `MEAN(x)` is that ordered `SUM(x)` divided by `COUNT(x)`
with the numeric `/`, inheriting the binary64 rounding of `SUM`, and
`VARIANCE`, `STDDEV` and `GEOMEAN` add in the same order. A reduction does not
sort the records, except as `STR_JOIN` and the quantiles do
([FORMULA-18](#formula-18)).

Evidence: `adam-adlb-ordered-sum`, `adam-adcm-related-events`, `adam-adpp-geometric-mean-ratio`, `adam-adlb-reference-limits`, `rust/crates/yamaa-core/tests/fixtures/reduction.tsv`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/value_semantics.json::sum_in_stored_order_loses_a_one_added_before_the_sum_cancels`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/value_semantics.json::sum_in_stored_order_keeps_a_one_added_after_the_sum_cancels_and_mean_divides_that_sum`.

`VARIANCE(x)` takes two passes in relation record order. The first is the
ordered `MEAN(x)` above, `m`, so a value that is not a number, or the `int`
overflow of its sum, fails as `MEAN` does. The second adds the first squared
deviation, `(x - m) * (x - m)`, and then each later one, with the numeric `-`,
`*` and `+`, and divides the total by `n - 1` with the numeric `/`, `n` being
the count of non-missing values. `STDDEV(x)` is the `SQRT` of that variance
([FORMULA-13](#formula-13)). Fewer than two non-missing values give missing
([FORMULA-24](#formula-24)). On 2, 4, 4, 4, 5, 5, 7 and 9 the mean is 5 and
the squared deviations add to 32, so `VARIANCE` is `32 / 7` and `STDDEV` is
`SQRT(32 / 7)`.

`GEOMEAN(x)` is the `EXP` of the ordered mean of `LN(x)`: in relation record
order, the first logarithm and then each later one added with the numeric
`+`, and the total divided by their count with the numeric `/`, under the
portable `LN` and `EXP` of [FORMULA-13](#formula-13). A zero or negative value
fails as `LN` does ([FORMULA-15](#formula-15)). The geometric mean of 1, 10
and 100 is `EXP((LN(1) + LN(10) + LN(100)) / 3)` in those binary64 steps,
`10.000000000000002`.

The `filter`, when present, removes records and keeps the order of the records
that remain. [Inputs](inputs.md) defines stored-source
record order, and [Column derivation](columns.md) defines
constructed-output and grouped-input record order. `COUNT`, `MIN` and `MAX`
are independent of record order, `COUNT(x)` with `distinct` too, a quantile
too except for which of two equal values, such as `0.0` and `-0.0`, sorts
first, and `ONLY` accepts no group in which an order could choose among
records ([FORMULA-20](#formula-20)). A rule that needs one record chosen by value
order uses a window or the `order_by` plus `keep` selection
([LOOKUP-10](lookup.md#lookup-10)), where the value order is declared.

`STR_JOIN` alone reads the records in a declared order: the aggregate's
`order_by` sorts them for it, records equal on every term keeping relation
record order ([LOOKUP-10](lookup.md#lookup-10)). An `order_by` beside any
other reducer is refused ([FORMULA-18](#formula-18)), and `STR_JOIN` stands
only as the whole expression ([FORMULA-23](#formula-23)), so `SUM`, `MEAN`,
`VARIANCE`, `STDDEV` and `GEOMEAN` always add in relation record order.

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
Evidence: `yaml/grammar/expression.yaml`, `rust/crates/yamaa-core/tests/numeric_aggregate_parser.rs`, `rust/crates/yamaa-application/tests/fixtures/syntax_controls.json`.

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
Evidence: `negative-dose-intensity`, `rust/crates/yamaa-application/tests/fixtures/correlated_aggregates.json`.

`SUM(EX.EXDOSE) / EX.EXPLDOS` is legal exactly when `EX.EXPLDOS` is declared
in `group_by`.

<a id="formula-23"></a>

### FORMULA-23. A reduction keeps the type it reduces, and arithmetic over reductions is numeric

`COUNT` returns `int`, `MEAN` and the statistical reducers return `float` and
`STR_JOIN` returns `str`; `SUM` keeps its argument's numeric type, and `MIN`,
`MAX` and `ONLY` keep the type they reduce, whatever it is. `SUM`, `MEAN` and
the statistical reducers require a numeric argument, `MIN` and `MAX` mutually
comparable values, and `COUNT`, `ONLY` and `STR_JOIN` accept any type. An
expression using an operator or numeric function is numeric: every reduction
and grouped identifier in it must be numeric, and [Values](values.md)
promotion gives its type.

Fails: `incompatible_input_type`, `incomparable_sources`.
Evidence: `negative-sum-non-numeric`, `negative-join-arithmetic`, `negative-stddev-non-numeric`, `rust/crates/yamaa-core/tests/fixtures/reduction.tsv`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/aggregates.json::a_statistical_reducer_over_a_name_that_is_not_numeric_is_refused_before_any_record`.

`SUM`, `MEAN` or a statistical reducer over a non-numeric argument fails,
naming the reducer, as does arithmetic over a non-numeric reduction or grouped
identifier. A statistical reducer's result is a number, so it may stand inside
arithmetic: `100 * STDDEV(AVAL) / MEAN(AVAL)` is a coefficient of variation in
percent. `MIN` or `MAX` over incomparable
values fails: a column mixing incomparable types is an error rather than an
implementation-defined order. Their string order is that of
[Values](values.md).

`STR_JOIN` is text whatever it joins, so it stands only as the whole
expression: `STR_JOIN(SEQN) + 1` fails before any record is read as
arithmetic over a non-numeric reduction, naming `STR_JOIN(SEQN)` as the
operand, even when `SEQN` is a number.

No implicit conversion happens inside the grammar.
[Values](values.md) converts the completed derivation
result, as it does for every other expression. These types, over the declared
types of the relation's fields and of the aggregate's `derivations`, give
`aggregate` its result type ([EXPR-14](expressions.md#expr-14)).

<a id="formula-24"></a>

### FORMULA-24. Value reducers skip missing values, and a group with no record reduces to missing

Inside a reduction's argument a missing operand propagates, so a record whose
operand is missing contributes a missing value rather than a zero:
`SUM(EX.EXDOSE * EX.EXDUR)` skips a record missing either factor. `SUM`,
`COUNT(x)`, `MIN`, `MAX`, `MEAN`, `STR_JOIN` and the statistical reducers then
ignore missing values, while `ONLY` counts records
([FORMULA-20](#formula-20)). A group with no record after `filter` yields
missing. When every value is missing, `COUNT(x)` yields `0`, with or without
`distinct`, and the others missing, never zero or empty text, as `STDDEV` and
`VARIANCE` do for one value.

Evidence: `adam-adex-uncollected`, `adam-adcm-related-events`, `adam-adsl-bmi-quartile-group`, `adam-adpp-geometric-mean-ratio`, `rust/crates/yamaa-core/tests/fixtures/reduction.tsv`.

| Group | Result |
|---|---|
| No record in the group after `filter` | missing, uncounted: an aggregate declares no handler ([COLUMN-11](columns.md#column-11)) |
| Every value missing -- `SUM`, `MIN`, `MAX`, `MEAN`, `STR_JOIN` | missing, never zero or empty text |
| Every value missing -- `MEDIAN`, `Q1`, `Q3`, `QUANTILE`, `STDDEV`, `VARIANCE`, `GEOMEAN` | missing |
| Every value missing -- `COUNT(x)`, with or without `distinct` | `0` |
| One non-missing value -- `STDDEV(x)`, `VARIANCE(x)` | missing: a sample deviation needs two values |
| One non-missing value -- `MEDIAN`, `Q1`, `Q3`, `QUANTILE` | that value, as a `float` |
| One non-missing value -- `GEOMEAN(x)` | `EXP(LN(x))`, which can differ from the value in the last place |
| No record in the group -- `COUNT(x)`, `COUNT(D.*)`, `STR_JOIN(x)` | missing |
| No record in the group -- `ONLY(x)` | missing |
| One record whose value is missing -- `ONLY(x)` | missing |

An absent record remains distinguishable from a collected missing value, and
an uncollected quantity is never reported as a measured zero. `MEAN` returns
missing before its division when no non-missing value remains, so an
all-missing group does not fail with division by zero, and the statistical
reducers return missing before any step that reads a value. Arithmetic over
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

`a / (b * b)` and `a / b / b` are different formulas and may return different
doubles. Both are correct, and evaluation returns the double for the written
formula. Language or engine settings that make division integral are
overridden. A host SQL engine's coercion, collation, `LIKE` escape and
missing-value behaviors are configured and overridden to match these rules, or
the grammar is evaluated directly.
