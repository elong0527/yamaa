---
id: R002
title: Source Binding
status: normative
applies_to: [root.datasets, root.base, row.dataset, row.group_by, expression.source, string_template]

---

# Source binding

## Intent

Bind source files, row drivers, and input variables without implicit same-name
inference.

## Boundaries

This rule owns dataset declaration and how a name resolves to a value. What
happens when a qualified name reaches another dataset is R003. What happens
when a bound name is absent or matches several records is R008. What a stored
field becomes before it is bound at all, including when it is missing and which
type it carries, is R014. R019 owns the text a string value contains. Which
files a declared path may reach, and the bytes a run reads from one, is R021.

## Dataset declarations

**R002-1.** `datasets` maps dataset identifiers to source data
declarations. Identifiers are used by `base`, `rows.dataset`, qualified
source variables, and `mapping_from`.

**R002-2.** A declaration is a path, or a path with the types its fields
carry; R014 owns that reading and the shorthand between the two forms.

**R002-3.** A declared path is a `project_path`, resolved relative to the
specification file and confined by R021. R017 preserves that origin when
a declaration is inherited and rebases the path in a materialized
resolved specification.

**R002-4.** Every referenced dataset identifier must exist in `datasets`.

**R002-5.** A dataset identifier must not equal the output `domain`.

**R002-6.** A finished dataset an earlier run produced is an ordinary
source and is declared under a name of its own.

**R002-7.** No keyed construct reaches a sibling record of the dataset
the run is building; R001 owns what happens when a column reaches its
own value through the rows of its partition. Addressing a sibling
record by key is open work.

## Source expressions

The concise source form names one variable:

```yaml
source: DM.SEX
```

**R002-8.** `DATASET.VARIABLE` refers to `VARIABLE` in the declared
source dataset. An unqualified reference such as `AVAL` refers to a
variable in the output currently being derived.

**R002-9.** A qualifier is a dataset identifier or a record lookup
identifier, which share one namespace; R015 owns what a record lookup
resolves to.

**R002-10.** A qualified reference to the current row-driving dataset
reads the current source record. A qualified reference to another
dataset follows R003.

**R002-11.** During grouped row construction there is no single current
source record. A qualified reference to the row driver is scalar only
when the exact variable appears in the enclosing `row.group_by`; it
then returns that group's key value.

**R002-12.** Reading any other driver field with a scalar `source` is an
error. An aggregate expression is how a grouped row reduces non-key
fields, as R007 and R013 define.

**R002-13.** Operation operand fields typed as `variable` accept a
concise source or current output variable. Compose operations through
an explicitly named derived column:

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

**R002-15.** A placeholder in a `string_template` is also a variable
reference. R012 owns the braces and escaping; the placeholder's
complete name binds here exactly as if it appeared in a field typed as
`variable`. Text outside placeholders is literal text.

**R002-16.** Plain strings outside fields typed as `variable` are
literal strings under R019.

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
them and R003 defines the join uniqueness that `multiple_matches`
relaxes.

## ODM contextual references

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

**R002-22.** Resolution first matches every available context column and
then matches the complete `ItemOID`. A projection may omit a context
column only when its source does not carry that level.

**R002-23.** A projection that carries `FormOID` must use it: identical
item identifiers in two forms are different contextual values and must
not be collapsed.

**R002-24.** No contextual match is an absent item. It fails unless a
structured source declares `missing`, under R008.

**R002-25.** More than one match after applying every available context
column is a multiple right-side match. It fails unless a structured
source declares `multiple_matches`, also under R008.

**R002-26.** A present matched row whose `Value` is missing returns
missing and does not invoke the absent-item handler.

## Rationale

Unqualified names address the dataset currently being derived, so
reusing the output domain as a dataset identifier would be ambiguous.
Forbidding arbitrary nested expressions in variable fields keeps each
operation self-contained and makes dependencies visible.

## Errors

**R002-27.** An unknown dataset identifier or variable: fail.

**R002-28.** A dataset identifier equal to the output `domain`: fail.

**R002-29.** A source path that R021 does not accept: fail, under R021's
condition.

**R002-30.** An unresolved unqualified reference: fail.

**R002-31.** A scalar source in a grouped row naming a driver field
absent from that row's `group_by`: fail.

**R002-32.** An ODM contextual reference with no available context
column: fail.

**R002-33.** More than one ODM contextual match: fail unless locally
handled.

**R002-34.** No ODM contextual match: fail unless locally handled.
