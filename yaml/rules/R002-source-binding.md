---
id: R002
title: Source Binding
status: normative
applies_to: [root.input, root.base, row.dataset, row.group_by,
  expression.source, string_template]

---

# Source binding

## Intent

Bind source files, row templates' input datasets, and input variables.
Never infer same-named variables.

## Boundaries

This rule owns dataset declarations and name resolution. R003 owns a
qualified name that reaches another dataset. R008 owns an absent bound name or
a bound name matching several records. R014 owns the value and type of a
stored field before binding, including a missing field. R019 owns string
contents. R021 owns the declared files a path may reach and the bytes a run
reads from a path.

## Dataset declarations

**R002-1.** `input` maps dataset identifiers to source data
declarations. Identifiers are used by `base`, `rows.dataset`, qualified
source variables, and `lookup`.

**R002-2.** A declaration is a path, or a path with the types the fields
carry. R014 owns that reading and the shorthand between the two forms.

**R002-3.** A declared path is a `project_path`. R021 fixes its written form,
approved root, and readable bytes. R017 preserves the path origin when a
declaration is inherited. R017 rebases a relative path in a materialized
resolved specification.

**R002-4.** Every referenced dataset identifier must exist in `input`.

**R002-5.** A dataset identifier must not equal the output `domain`.

**R002-6.** A finished dataset from an earlier run is an ordinary source.
Declare that source under its own name.

**R002-7.** No keyed construct reaches a sibling record of the output
dataset. R001 owns what happens when a column reaches its own value
through the window partition rows of that column. Addressing a sibling record by
key is open work.

## Source expressions

The concise source form names one variable:

```yaml
source: DM.SEX
```

**R002-8.** `DATASET.VARIABLE` refers to `VARIABLE` in the declared
source dataset. An unqualified reference such as `AVAL` refers to a
variable in the output dataset.

**R002-9.** A qualifier is a dataset identifier or a record lookup identifier;
both share one namespace; R015 owns what a record lookup resolves to.

**R002-10.** A qualified reference to the current row template's input
dataset reads the current input record. A qualified reference to
another dataset follows R003.

**R002-11.** During grouped row construction there is no single current
input record. A qualified reference to the input dataset of the row
template is scalar only when the exact variable appears in the enclosing
`row.group_by`. That reference then returns the key value of that group.

**R002-12.** Reading any other source variable with a scalar `source` is
an error. A grouped row reduces non-key variables with an aggregate
expression, as R007 and R013 define.

**R002-13.** Operation operand fields typed as `variable` accept a
concise source or current output variable. Compose operations through
a named derived column:

```yaml
- name: PREFIX
  type: str
  derivation:
    str_extract:
      source: RAW.TEXT
      pattern: '^[A-Z]+'
- name: CATEGORY
  type: str
  derivation:
    mapping:
      source: PREFIX
      dict: {A: Alpha}
```

**R002-14.** An operation cannot place an arbitrary nested expression in
a variable field.

**R002-15.** A `string_template` placeholder is a variable reference. R012
owns braces and escaping. The placeholder's complete name binds as a
`variable` field name. Text outside placeholders is literal.

**R002-16.** Plain strings outside fields typed as `variable` are
literal under R019.

**R002-17.** Implementations must not infer same-named source variables
when an output column has no derivation.

## Structured source binding

**R002-18.** The `source` expression also accepts an object containing
`variable` and local binding or join behavior:

```yaml
source:
  variable: ADSL.TRTSDT
  missing: null
```

**R002-19.** `missing` and `multiple_matches` are handlers; R008 defines
them and R003 defines the join uniqueness `multiple_matches` relaxes.

**R002-19a.** `filter` is not a handler. It states which records the
source may read, and R003 defines that selection. Reading no record is an
absent match rather than a handled condition.

## ODM contextual references

This form is retired. A source states which records it reads through
`filter`, which R003-21 owns, so an ODM item is addressed by a predicate
over `ItemOID` rather than by hiding that identifier in the variable name.
#506 removes the requirements below from the language; they describe only
the specifications still awaiting that rewrite, and the repository
validator rejects a new use.

**R002-20.** ODM item identifiers may contain periods.
`ODM.IT.LB.LBDTC` means the `Value` whose `ItemOID` is `IT.LB.LBDTC`,
resolved within the current ODM context.

**R002-21.** An ODM context is the current row's values for the
following columns, in this order, when those columns exist in the
declared ODM projection:

1. `StudyOID`;
2. `MetaDataVersionOID`;
3. `SubjectKey`;
4. `StudyEventOID`;
5. `StudyEventRepeatKey`;
6. `FormOID`;
7. `FormRepeatKey`;
8. `ItemGroupOID`;
9. `ItemGroupRepeatKey`.

**R002-22.** ODM resolution first matches every available context column,
then matches the complete `ItemOID`. A projection may omit a context
column only when the projection's source does not carry that level.

**R002-23.** A projection that carries `FormOID` must use that column.
Identical item identifiers in two forms are different contextual values.
Those values must not be collapsed.

**R002-24.** No contextual match is an absent item. The reference fails
unless a structured source declares `missing`, under R008.

**R002-25.** More than one match after applying every available context
column is a multiple right-side match. The reference fails unless a
structured source declares `multiple_matches`, also under R008.

**R002-26.** A present matched row with a missing `Value` returns missing.
That row does not invoke the absent-item handler.

## Rationale

Unqualified names address the output dataset. Reusing the output domain
as a dataset identifier would be ambiguous. Forbidding arbitrary nested
expressions in variable fields keeps each operation self-contained and
its dependencies visible.

## Errors

**R002-27.** An unknown dataset identifier or variable: fail.

**R002-28.** A dataset identifier equal to the output `domain`: fail.

**R002-29.** A source path that R021 does not accept: fail, under R021's
condition.

**R002-30.** An unresolved unqualified reference: fail.

**R002-31.** A scalar source in a grouped row naming a source variable
absent from that row's `group_by`: fail.

**R002-32.** An ODM contextual reference with no available context
column: fail.

**R002-33.** More than one ODM contextual match: fail unless locally
handled.

**R002-34.** No ODM contextual match: fail unless locally handled.
