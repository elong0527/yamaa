---
id: R024
title: Numeric Text Rendering
status: normative
applies_to: [expression.format_number, output.decimals]
depends_on: [R007, R011, R019]
---

# Numeric text rendering

## Intent

Fix the text a number takes at a stated precision: the digits, the point, the
sign, and the rounding that selects them. One form serves both consumers -- a
character value carried inside a dataset and an artifact's display precision
-- so the two cannot disagree about what `12.50` means.

## Boundaries

This rule owns rendering at a stated precision and nothing else. R011 owns
what an `int` and a `float` are, the normalization that leaves every float
reaching this rule finite, and the shortest round-trip text a float takes when
no precision is stated. R019 owns the string this rule produces and orders it
like any other. R010 owns the calculation, which rounds nothing.

R020 owns the artifact: which profile carries it, and that `output.decimals`
is a `csv` display applied once as a field is written. It takes its text from
this rule rather than defining one, and the conditions under which the field
itself is rejected stay there.

R007 owns registration, so `format_number` appears in the registry beside
every other expression and its payload is validated there. This rule owns what
it returns.

Nothing here decides a value. Rendering reads a number and produces text;
which number it reads is the caller's, and nothing here changes it.

## Rendering produces text, never a number

A rendered result is a string from the moment it exists. No numeric value is
rounded anywhere in this design: a number keeps every digit its calculation
gave it, and a specification that needs the number keeps reading the number.

The distinction is what allows a precision to be declared at all. A language
that rounded the value would make every dependent column, predicate,
aggregate, verification, key, and order term depend on a presentation
decision. A language that renders leaves the number untouched and produces a
second thing beside it, and a reviewer who reads `12.50` knows it is text that
was never arithmetic.

## The written form

A number rendered at `n` digits, where `n` is a non-negative integer, is
written in fixed-point positional notation with exactly `n` digits after the
decimal point, and with a decimal point only when `n` is greater than zero. A
value keeps its declared width whether or not its digits require it: at an `n`
of 4 an integral 25 is written `25.0000`, and at an `n` of 0 a value of 12.5
is written `13`.

A negative value carries a leading `U+002D`. A value that rounds to zero is
written without a sign, so at an `n` of 2 the value -0.004 is written `0.00`
and not `-0.00`: the text reports zero at the width it was asked for, and a
sign on it would assert a distinction that width does not carry.

The text never carries an exponent and never groups digits, so one value at
one precision has exactly one spelling. R011 makes the same promise for a
float's shortest text and for the same reason: a value with two admissible
spellings leaves two runtimes both conforming and different.

An `int` renders under this same form. It is already exact, so there is
nothing to round, and at an `n` of 2 it takes a point and two zeros. A
collected count reported beside a measured result therefore reaches the same
width without first becoming a float.

## The rounding is exact and host-independent

Every binary64 value is exactly some decimal fraction. Round that exact value:
multiply it by ten raised to `n`, round the product to an integer with a tie
going away from zero, and divide by ten raised to `n` again.

The tie is decided on the exact value, never on a shortened representation of
it, and the difference is observable:

| Value as written in source | Its exact binary64 value | at an `n` of 2 |
|---|---|---|
| `0.125` | 0.125 | `0.13` |
| `-0.125` | -0.125 | `-0.13` |
| `2.675` | 2.674999999999999822364316059974953532218933105468750 | `2.67` |

`0.125` is representable, so it is a genuine tie and rounds away from zero.
`2.675` is not representable and the nearest binary64 is below it, so there is
no tie to break and it rounds down. An implementation that first shortens the
value to `2.675` and then rounds reports `2.68` and does not conform.

No host rounding or formatting routine may be assumed to do this. R's `round`
and Python's `round` both send an exact tie to the even digit rather than away
from zero, and the C formatting both ecosystems build on does the same. Each
of the three disagrees with this rule on `0.125`, so an implementation
performs the exact scaling above rather than delegating.

## The two consumers

`format_number` renders a value. Its result is a `str` the specification holds
like any other: it can be a column, it reaches whichever profile R020 selects,
and every later stage sees it. SDTM `--STRESC` and ADaM `AVALC` are character
renderings of numbers at a stated precision, and they are data a consumer
reads back rather than a presentation of a number stored elsewhere.

`output.decimals` renders a display. R020 applies it to the `float` columns of
a `csv` artifact as they are written, after everything that rule sequences,
and nothing in the run observes the result.

The two never collide. A column derived by `format_number` is a `str` and
takes no display precision, and a `float` column carries no rendered text. A
specification that needs `12.50` beside a four-decimal number therefore
declares the character column and renders it, rather than asking one
artifact-wide setting for two widths it cannot hold at once.

## format_number

`format_number` takes one numeric `source`, a required non-negative integer
`decimals`, and an optional `missing` literal. It returns the text the form
above gives its source at that precision.

A missing source with no declared `missing` produces a missing result.
Rendering has no text for a value that was never collected, and writing zeroes
for one would report a measurement that does not exist.

`decimals` is required. A default width would place a presentation decision
somewhere other than the specification, and the reviewer of a character result
reads its precision where the column is declared.

`decimals` is a literal, so one rendering writes one width. A study that
reports each test at the precision that test was collected at selects among
renderings rather than varying one: `case` nests an expression in every
branch, so a branch per precision names its width where the condition that
chooses it is written. A width read from the data would instead leave the
artifact's shape decided by a value, and no reviewer could say from the
specification how wide a column is.

## Errors

- A `decimals` that is not a non-negative integer: fail validation.
- A `format_number` whose `source` is neither `int` nor `float`: fail
  validation with `incompatible_input_type` and report the source, the
  expected types, and the type it has.
- Rounding with a host routine whose ties do not go away from zero, or
  rounding the number rather than rendering text from it: neither is an
  implementation option.
