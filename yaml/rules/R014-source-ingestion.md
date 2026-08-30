---
id: R014
title: Source-Format Ingestion
status: normative
applies_to: [root.datasets, dataset_source, dataset_class, expression.source]
depends_on: [R002, R006, R011]
---

# Source-format ingestion

## Intent

Define what a stored field becomes before any expression reads it: when it is
missing, and which type it carries.

## Boundaries

This rule owns the step from a stored field to a bound value. R002 owns how a
name binds to a dataset or an ODM item once that value exists, R011 owns
conversion of a completed derivation result into a declared column type, and
R007 owns what each expression requires of an input it receives.

## A field's type belongs to the dataset

Every field of a source dataset has exactly one type, drawn from `column_type`,
and every value bound from that field carries it. A field's type is a property
of the dataset, not of the values one extract happens to hold, so two extracts
of the same dataset bind the same field to the same type.

Where the type comes from depends on the container:

- A **self-describing container** supplies it. An ODM `ItemDef` data type, a
  columnar file's schema, and a transport file's variable types are each the
  field's type, and the specification does not restate it.
- A **typeless container**, such as a delimited text file, supplies none. Every
  one of its fields is `str` unless the specification declares otherwise.

`dataset_class.types` declares the type of a named field of a typeless
container. A field it does not name is `str`. This is a statement about the
dataset as this specification reads it: two specifications may read the same
delimited file with different declarations, because the file carries no types
to contradict either of them. A dataset whose types matter to more than one
specification belongs in a container that carries them.

```yaml
datasets:
  EX: {path: input/ex.csv, types: {EXDOSE: float, EXSEQ: int}}
  DM: input/dm.csv
```

Both forms are the same declaration: a bare path is R006 shorthand for a
`dataset_class` with no `types`.

## Values are never inferred

**An implementation must not infer a field's type from its values.** Inference
would make a type a property of one extract:

- A dose field of digits is numeric in January and text in February, when one
  result arrives as `<50`. The specification did not change, and the run now
  fails on an expression that was correct.
- A site identifier of `007` becomes the number seven, and every value it
  identifies is silently rewritten.
- A field whose values happen to be complete dates in a small extract becomes a
  date, and the partial value the study permits fails on arrival.

A declared type states what the study collects, and a value that does not match
it is a defect in the data rather than a reason to retype the field. Guessing
substitutes the second reading for the first, and does so differently for every
extract.

## Parsing a declared type

A declared field type is applied to the stored text by the `str` row of R011's
conversion table, which is the same parsing a `str` column uses when it reaches
a declared type. `int` and `float` accept exactly R010's `number` production
with an optional sign; `date` accepts exactly a complete ISO 8601 calendar
date; `datetime` accepts exactly the lexical form R016 fixes. A value that does
not parse fails the run.

An ingestion failure is not a conversion failure. `conversion_failure` is
declared on a column and answers for a value the derivation produced, as R005
and R008 define; a stored value that does not match its field's declared type
is rejected before any derivation runs and no handler answers for it. A
specification that wants to see such a value declares the field `str` and
converts it at the column that consumes it, where a handler exists.

## Missing values

A missing value is the absence of a value, and it is recognized before typing.
Every type admits it, and R008's handlers answer for it.

In a delimited text file, a field with no characters between its delimiters is
missing. A quoted empty field is the empty string, which is a value: a `str`
field therefore distinguishes an uncollected value from a collected empty one,
and no other type admits an empty string at all.

**No text is a missing-value sentinel.** `NA`, `NULL`, `.`, `unknown`, and
every other spelling are ordinary values. A reader that treats them as absence
loses `NA` as a region, `.` as a separator, and a collected `unknown` as a
recorded answer, and it does so before any rule in this design can see the
value. A study that records absence with a code maps that code to a result
where the specification can be read.

An empty field of a declared numeric or date type is missing rather than a
parse failure, because it holds no text to parse.

## Errors

- A `types` entry naming a field the dataset does not have: fail.
- A `types` entry for a field whose container already supplies a type: fail,
  rather than override the container.
- A stored value that does not parse under its field's declared type: fail,
  reporting the dataset, field, and value.
- Inferring a field type from its values, or treating text as absence: neither
  is an implementation option.
