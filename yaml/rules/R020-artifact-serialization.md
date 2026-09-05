---
id: R020
title: Artifact Serialization
status: normative
applies_to: [root.output, output.profile, output.decimals, artifact_profile]
depends_on: [R005, R011, R014, R016, R019]
---

# Artifact serialization

## Intent

Define how a completed, ordered artifact becomes bytes: which container carries
it, what each value looks like inside that container, how an uncollected value
differs from a collected empty one, the single point at which a float takes a
display precision, and how a written artifact replaces its target.

## Boundaries

This rule begins where R005 ends. R005 owns which columns the artifact has,
their order, which rows it holds, and the order those rows leave in; nothing
here can change any of them. R011 owns what value a column holds and the text a
value carries when it is converted to `str`, and defers to this rule the one
display rounding that happens after every calculation. R016 owns the canonical
text of a `date` and a `datetime`. R019 owns the contents of a string, the
failure of ill-formed encoded text, and the order of two strings.

R014 owns the other direction. It states what a stored field means when a
specification reads it, and `csv` below is the writing counterpart of the
delimited form it reads: the two agree on missing and on the empty string, and
neither restates the other. The profile a specification reads a delimited
*source* under is not settled by either rule.

This rule owns writing the artifact and replacing the target with it. How that
target's path is resolved is a property of the specification's resolution and
is not settled here.

## Two profiles

`output.profile` names the profile an artifact is written under. Version 1.0
defines exactly two, and omitting the field selects `parquet`, so a
specification that says nothing about serialization still has one contract
rather than a host default.

| Profile | Container | What two runtimes must agree on |
|---|---|---|
| `parquet` | Apache Parquet | the schema, column order, row order, and values that read back |
| `csv` | delimited text | the bytes |

The two exist for different readers. `parquet` is the production container:
it carries its own types, so an artifact read by another specification needs no
declaration to be understood, and a large one does not pay for decimal text.
`csv` is the reviewable container: a human can read it, a diff can show what
moved in it, and its bytes are fixed exactly, which is what makes it usable as
a golden contract.

A profile names a container, and the specification's `schema_version` fixes
which release's contract it was written under. The two together identify the
bytes exactly, and a consumer reading a stored artifact receives both, because
the producing-specification link R014 defines carries the whole producer
document rather than the profile alone.

A later release that changes any byte-level or mapping decision below therefore
changes what a profile means at that schema version, and an artifact keeps the
meaning its producer's version gives it. A profile that ever has to diverge
from the schema version is added as a further name rather than by redefining
one of these two.

## The csv profile

### Bytes

- The artifact is encoded UTF-8 and carries no byte-order mark. R019 owns the
  text being encoded.
- `U+000A` terminates every record, including the last, so every artifact ends
  with it. `U+000D` is never written as part of a terminator; it appears only
  inside a quoted field that contains one.
- `U+002C` separates fields. No other delimiter is defined.
- The first record is the header: the names of `output.columns`, in that order,
  written under the quoting rule below.
- Each following record is one row, in the order R005 fixes, holding one field
  per header name in the same order.
- An artifact with no rows is the header record and its terminator alone. It is
  not an empty file, because the columns exist whether or not a row does.

### Quoting

A field is quoted exactly when its text contains `U+0022`, `U+002C`, `U+000D`,
or `U+000A`, or when the field is the empty string. Every other field is
written bare.

A quoted field is wrapped in `U+0022` and each `U+0022` within its text is
written twice. Nothing else is escaped: a quoted field carries its newlines,
delimiters, and every other scalar exactly.

The rule is stated as an exact condition rather than as a minimum, so two
runtimes quote the same fields. A writer that quotes a field this rule leaves
bare, or leaves bare a field it quotes, does not conform even though an
ordinary reader accepts its output.

### Missing and the empty string

A missing value is written as no characters at all, unquoted. A collected empty
string is written as two quote characters. These are the two forms R014 reads
back as absence and as the empty string, so a `str` column round-trips through
this profile without ambiguity, and no text is ever pressed into service as a
sentinel for absence.

    STUDYID,COMMENT,NOTE
    S1,plain text,
    S1,"has, comma",""
    S1,"say ""hi""",x

The third row's `NOTE` is the ordinary string `x`, the second row's is a
collected empty string, and the first row's is missing.

### Value text

| Column type | Text written |
|---|---|
| `str` | its scalar values, under R019 |
| `int` | its decimal digits, with a leading `U+002D` when negative |
| `float` | R011's float text, or the fixed-point form below |
| `date` | R016's canonical `date` text |
| `datetime` | R016's canonical `datetime` text |

An `int` is written without a leading `U+002B`, without digit grouping, and
without a leading zero; zero is `0`. A `float` that takes no display precision
is written by R011's conversion to `str`: the shortest round-tripping digits in
positional notation, with a trailing `.0` omitted. That conversion carries no
exponent, which is what lets this profile promise bytes -- a value with two
admissible spellings would leave two runtimes both conforming and different.

## The parquet profile

### Column mapping

Each declared type maps to exactly one Parquet physical and logical type.

| Column type | Physical | Logical |
|---|---|---|
| `str` | `BYTE_ARRAY` | `String` |
| `int` | `INT64` | none |
| `float` | `DOUBLE` | none |
| `date` | `INT32` | `Date` |
| `datetime` | `INT64` | `Timestamp`, microseconds, not adjusted to UTC |

The schema's fields are the names in `output.columns`, in that order. Every
field is optional, because every column type admits a missing value.

### Missing and the empty string

A missing value is a Parquet null. A collected empty string is a present
`BYTE_ARRAY` of zero length, which is not null. This is the same distinction
`csv` draws between a bare field and two quote characters, carried by the
container instead of by a convention.

### Temporal values

A `date` is the count of days from 1970-01-01, and a `datetime` the count of
microseconds from 1970-01-01T00:00:00 on the same wall clock the value names.

R016's `datetime` is a reading on a wall clock and carries no zone and no
offset, so its Timestamp is not adjusted to UTC and an implementation must not
attach a zone on the way out or on the way back in. A runtime whose native
timestamp always carries one -- R016 names R's `POSIXct` as such a type -- must
still write and read this column so that the same wall clock survives; shifting
a value into or out of a machine timezone changes it, and two runtimes that
each shift by their own offset do not agree.

A `datetime` is resolved to a whole second, so its microsecond part is always
zero. Microseconds are chosen because the format offers no second unit and
because both ecosystems' readers agree on this one; the finer resolution is
never used.

### Determinism

Two runtimes writing the same completed dataset must produce Parquet artifacts
that read back identically: the same field names in the same order, the same
logical types, the same rows in the same order, the same nulls, and the same
values, with every `DOUBLE` bit-identical.

An implementation writes uncompressed pages and adds no key-value metadata of
its own beyond what the format requires.

The bytes themselves are not fixed. A Parquet writer stamps its own identity
and version into the file, and the row-group and page sizing, the encodings it
selects, and the statistics it records are properties of the library rather
than of this design. Requiring identical bytes would require every conforming
implementation to abandon its ecosystem's writer, which buys less than it
costs. An artifact whose bytes must be compared directly is written under
`csv`, whose byte guarantee is exactly that.

### Floats are stored, not rendered

A `float` reaches this profile as the binary64 value the derivation produced.
`output.decimals` does not apply, and no rounding happens on the way out: a
consumer that reads the artifact back receives the value the calculation used.
Storing a container's native double is not a display, and this design rounds
once, at a display.

## Display precision

`output.decimals` is an optional non-negative integer. It applies to `csv`
alone, and to every `float` column of the artifact.

When it is absent, a `float` is written as R011's float text. When it is
present with the value `n`, a `float` is written in fixed-point form with
exactly `n` digits after the decimal point, and with a decimal point only when
`n` is greater than zero. A value therefore keeps its declared width whether or
not its digits require it: at `n` of 4, an integral 25 is written `25.0000`.

**This is the only place a value is rounded for presentation.** It happens
once, when the field is written, and after everything R005 sequences: every
derivation, every conversion, every override, every verification, key
validation, and row ordering. No dependent column, predicate, aggregate,
verification, key, or order term ever sees a rounded value, and changing
`output.decimals` cannot change whether a run passes or which rows it produces.

### The rounding is exact and host-independent

Every binary64 value is exactly some decimal fraction. Round that exact value:
multiply it by ten raised to `n`, round the product to an integer with a tie
going away from zero, and divide by ten raised to `n` again. A value that
rounds to zero is written without a sign.

The tie is decided on the exact value, never on a shortened representation of
it, and the difference is observable:

| Value as written in source | Its exact binary64 value | `decimals: 2` |
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
from zero, and the C formatting both ecosystems build on does the same. Each of
the three disagrees with this rule on `0.125`, so an implementation performs
the exact scaling above rather than delegating.

## A stored artifact carries its profile

A specification that reads an artifact another specification produced learns
how those bytes are encoded from the producer, through the producing
specification link R014 defines: the producer's `output.profile` states the
profile, exactly as its `output.columns` states the fields. A consumer does not
infer a profile from a file's name or extension, and a reader that guesses one
can read a conforming artifact wrongly without failing.

## Publication

An artifact becomes visible in one step. An implementation:

1. writes the complete artifact into a temporary regular file in the same
   directory as the target;
2. flushes and closes that file, so its bytes reach the filesystem rather than
   a buffer; and
3. atomically replaces the target with it.

The temporary file is a regular file, and it is in the target's own directory
so that the replacement stays within one filesystem and remains atomic. Its
name is not fixed, but it must not collide with the target or with another
run's temporary file.

A run that fails at any point leaves the target as it was and removes its
temporary file, so a failure produces neither an accepted artifact nor
residue. A reader observes either the artifact that was there before or the
complete new one, and never a prefix of the new one.

Publication happens once, after the whole artifact is complete: after every
value's lifecycle, key validation, and verification under R005, and after its
rows are ordered. Rows are not streamed to the target as they are constructed,
because a partially constructed dataset is not yet ordered and a run that fails
midway would already have published part of it.

## Errors

- An `output.profile` naming a profile this version does not define: fail
  validation with `unknown_artifact_profile` and report the value.
- An `output.decimals` that is not a non-negative integer: fail validation.
- An `output.decimals` declared with `parquet`: fail validation with
  `decimals_not_applicable`. A display precision that cannot take effect is a
  defect in the specification rather than a setting to ignore.
- A value that cannot be written under its column's mapping: fail and report
  the column, the row's key, and the value.
- A failed atomic replacement: fail and report the target. The run produces no
  artifact, and the previous one is unchanged.
- Writing a byte-order mark, a `U+000D` record terminator, or a quoting that
  differs from the `csv` condition: none is an implementation option.
- Rounding with a host routine whose ties do not go away from zero, or
  rounding a value any other stage can observe: neither is an implementation
  option.
