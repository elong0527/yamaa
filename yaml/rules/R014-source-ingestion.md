---
id: R014
title: Source-Format Ingestion
status: normative
applies_to: [root.datasets, dataset_source, dataset_class, expression.source]

---

# Source-format ingestion

## Intent

Define what a stored field becomes before any expression reads it: when it is
missing, and which type it carries.

## Boundaries

This rule owns the step from a stored field to a bound value. R002 owns how a
name binds to a dataset or an ODM item once that value exists, R011 owns
numeric parsing and non-finite normalization in addition to conversion of a
completed derivation result, and R007 owns what each expression requires of an
input it receives. R019 owns valid text and failures while decoding it. R021
owns which file `path` and `schema` may reach and the byte snapshot this rule
reads. R023 owns the syntax of a delimited source and delivers each field to
this rule as its text together with whether it was quoted; this rule owns what
that field then means.

## Source record order

**R014-1.** The sequence of records in a stored source is part of the input
contract. A reader must deliver records in their stored order and must
preserve that order after field decoding and typing; parallel reads, batches,
partitions, or an engine's scan plan must not reorder them. Filtering
preserves the relative order of the records that remain. R001 uses this
sequence as base-record and grouped-driver order, R007 uses it to break
window ties, and R013 consumes it for ordered floating-point reduction.

**R014-2.** Record order is not a substitute for a business key or a declared
sort. It is the stable sequence of this particular artifact: replacing an
artifact with the same records in another order changes the input and can
therefore change an order-sensitive result.

## A field's type belongs to the dataset

**R014-3.** Every field of a source dataset has exactly one type, drawn from
`column_type`, and every value bound from that field carries it. A field's
type is a property of the dataset, not of the values one extract happens to
hold, so two extracts of the same dataset bind the same field to the same
type.

**R014-4.** Where the type comes from depends on the container:

- A **self-describing source** supplies it. An ODM `ItemDef` data type, a
  container's embedded schema, and a delimited artifact's producing
  specification are each the field's type authority, and the consuming
  specification does not restate it.
- A **typeless container**, such as a delimited text file, supplies none.
  Every one of its fields is `str` unless the specification declares
  otherwise.

**R014-5.** `dataset_class.types` declares the type of a named field of a
typeless container. A field it does not name is `str`. This is a statement
about the dataset as this specification reads it: two specifications may read
the same delimited file with different declarations, because the file carries
no types to contradict either of them. A dataset whose types matter to more
than one specification belongs in a container that carries them.

```yaml
datasets:
  EX: {path: input/ex.csv, types: {EXDOSE: float, EXSEQ: int}}
  DM: input/dm.csv
```

**R014-6.** Both forms are the same declaration: a bare path is R006
shorthand for a `dataset_class` with no `types`.

## Producing-specification link

**R014-7.** `dataset_class.schema` makes a stored artifact carry the output
contract and workflow provenance of the Yamaa specification that produces it.
It is a `project_path` resolved relative to the consuming specification like
`dataset_class.path`, so R021 confines both. The referenced document is a
complete specification validated against the same `root_class` in
`schema.yaml`; there is no second source-schema class or field-description
language.

```yaml
datasets:
  DM:
    path: input/dm.csv
    schema: input/dm.schema.yaml
```

**R014-8.** The referenced specification is an executable workflow
predecessor. Its own sources must exist and validate, its derivations must be
complete, and any `schema` links it declares are validated recursively. The
resulting dependency graph must be acyclic. The producer completes before the
consumer reads the artifact named by `path`; a link cannot name the consuming
specification or any other specification already above it in the workflow.

**R014-9.** The producer's `output.columns` names every stored field exactly
once and in artifact order. Each selected entry in its `columns` supplies
that field's R011 type and label. Declared internal columns omitted from
`output.columns` are not stored fields. A delimited artifact's header must
equal `output.columns`; missing, extra, reordered, duplicate, or undeclared
fields fail.

**R014-10.** The producing specification remains the only type authority.
`types` may be present only for a typeless source, so it must be absent
whenever `schema` is present. This rejects even an inline entry that agrees
with the producer instead of creating two authorities for one type.

**R014-11.** The stored cells are still delimited text. After recognizing
missing values, ingestion applies the `str` row of R011's conversion table
to every non-missing cell. In particular, a producer column declared `date`
or `datetime` uses R016's lexical grammar and representations, exactly as an
inline `types` declaration or a column conversion does; the workflow link
does not enable a runtime's more permissive temporal parser.

## Values are never inferred

**R014-12.** An implementation must not infer a field's type from its
values. A declared type states what the study collects, and a value that
does not match it is a defect in the data rather than a reason to retype the
field.

## Parsing a declared type

**R014-13.** A declared field type is applied to the stored text by the `str`
row of R011's conversion table, which is the same parsing a `str` column
uses when it reaches a declared type. `int` and `float` use R011's numeric
text parsing, including its non-finite normalization; `date` and `datetime`
accept exactly the lexical forms R016 fixes. A value that does not parse
fails the run. R011 separately recognizes the YAML 1.2 non-finite forms
during declared numeric parsing; they remain text when the field's type is
`str` and normalize only after being parsed as numbers.

**R014-14.** An ingestion failure is not a conversion failure.
`conversion_failure` is declared on a column and answers for a value the
derivation produced, as R005 and R008 define; a stored value that does not
match its field's declared type is rejected before any derivation runs and
no handler answers for it. A specification that wants to see such a value
declares the field `str` and converts it at the column that consumes it,
where a handler exists.

## Missing values

**R014-15.** A missing value is the absence of a value, and it is recognized
before typing. Every type admits it, and R008's handlers answer for it.

**R014-16.** In a delimited source, a bare field with no characters is
missing. A quoted empty field is the empty string, which is a value: a `str`
field therefore distinguishes an uncollected value from a collected empty
one, and no other type admits an empty string at all.

**R014-17.** No text is a missing-value sentinel. `NA`, `NULL`, `.`,
`unknown`, and every other spelling are ordinary string values.

**R014-18.** An empty field of any declared type other than `str` is missing
rather than a parse failure, because it holds no text to parse.

## Rationale

Inference would make a type a property of one extract: a dose field of
digits is numeric in January and text in February when one result arrives as
`<50`, and the run then fails on an expression that was correct; a site
identifier of `007` becomes the number seven, silently rewriting every value
it identifies; a field complete with dates in a small extract becomes a
date, and the partial value the study permits fails on arrival. Guessing
substitutes a reading of the extract for the declaration of the study, and
does so differently for every extract. A reader that treats a spelling such
as `NA` or `.` as absence loses `NA` as a region, `.` as a separator, and a
collected `unknown` as a recorded answer before any rule in this design can
see the value; a study that records absence with a code maps that code to a
result where the specification can be read.

## Errors

- **R014-19.** A `types` entry naming a field the dataset does not have:
  fail.
- **R014-20.** A `types` entry for a field whose container or producing
  specification already supplies a type: fail, rather than override the
  source contract.
- **R014-21.** A document named by `schema` that does not validate as a
  complete Yamaa specification, or whose producer dependency creates a cycle:
  fail.
- **R014-22.** A stored artifact whose fields do not exactly match the
  producing specification's `output.columns`: fail.
- **R014-23.** A stored value that does not parse under its field's declared
  type: fail, reporting the dataset, field, and value.
- **R014-24.** Inferring a field type from its values, or treating text as
  absence: neither is an implementation option.
- **R014-25.** Reordering source records while reading, decoding, typing,
  filtering, or assembling parallel batches: fail.
