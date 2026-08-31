---
id: R002
title: Source Binding
status: normative
applies_to: [root.datasets, root.derived, root.base, row.dataset,
  expression.source, string_template]
depends_on: [R003, R006, R008, R014, R015]
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
type it carries, is R014.

## Dataset declarations

`datasets` maps source dataset identifiers to data declarations. Identifiers
are used by `base`, `rows.dataset`, qualified source variables, and
`mapping_from`. A source declaration is a path, or a path with the types its
fields carry; R014 owns that reading and the shorthand between the two forms.
Paths are resolved relative to the specification file.

Every referenced dataset identifier must exist in `datasets` or name one entry
under `derived`. Source and derived identifiers share one namespace and must be
unique. An identifier must not equal the output `domain`; unqualified names
address the dataset currently being derived, so reusing the domain would be
ambiguous.

A finished dataset an earlier run produced is an ordinary source and is
declared under a name of its own. A **derived dataset** -- one the
specification itself builds before the artifact, under `derived` -- is also an
ordinary source once built: a qualified source, an aggregate, or a record
lookup reads every column it declares exactly as it reads a declared dataset,
and a `rows` entry may drive from it. R001 owns when a derived dataset is built
relative to its readers, and R005's column coverage and key identity apply to
it as they do to the artifact. Reaching a sibling record of the dataset this
run is building is a different thing, and no keyed construct reaches it; R001
owns what happens when a column reaches its own value through the rows of its
partition. Within one dataset, addressing a sibling record by key stays open
work, because a derived dataset is built before its readers and is not such a
sibling.

## Source expressions

The concise source form names one variable:

```yaml
source: DM.SEX
```

`DATASET.VARIABLE` refers to `VARIABLE` in the declared source dataset.
An unqualified reference such as `AVAL` refers to a variable in the output
currently being derived. A qualifier is a dataset identifier or a record lookup
identifier, which share one namespace; R015 owns what a record lookup resolves
to.

A qualified reference to the current row-driving dataset reads the current
source record. On the grouped row construction R001 defines, it may read only
a base variable named by `group_by`, whose value is constant for the group. A
qualified reference to another dataset follows R003.

Operation operand fields typed as `variable` accept a concise source or current
output variable. Compose operations through an explicitly named derived column:

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

An operation cannot place an arbitrary nested expression in a variable field.
This keeps each operation self-contained and makes dependencies visible.

A placeholder in a `string_template` is also a variable reference. R012 owns
the braces and escaping; the placeholder's complete name binds here exactly as
if it appeared in a field typed as `variable`. Text outside placeholders is
literal text.

Plain strings outside fields typed as `variable` are literal strings.
Implementations must not infer same-named source variables when an output
column has no derivation.

## Structured source binding

The `source` expression also accepts an object containing `variable` and local
binding or join behavior:

```yaml
source:
  variable: ADSL.TRTSDT
  missing: null
```

`missing` and `multiple_matches` are handlers; R008 defines them and R003
defines the join uniqueness that `multiple_matches` relaxes.

## ODM contextual references

ODM item identifiers may contain periods. `ODM.IT.LB.LBDTC` means the `Value`
whose `ItemOID` is `IT.LB.LBDTC`, resolved within the current ODM context.

An ODM context is the current row's values for the following columns, in this
order, when those columns exist in the declared ODM projection:

1. `StudyOID`;
2. `MetaDataVersionOID`;
3. `SubjectKey`;
4. `StudyEventOID`;
5. `StudyEventRepeatKey`;
6. `FormOID`;
7. `FormRepeatKey`;
8. `ItemGroupOID`;
9. `ItemGroupRepeatKey`.

Resolution first matches every available context column and then matches the
complete `ItemOID`. A projection may omit a context column only when its source
does not carry that level. In particular, a projection that carries `FormOID`
must use it: identical item identifiers in two forms are different contextual
values and must not be collapsed.

No contextual match is an absent item. It fails unless a structured source
declares `missing`, under R008. More than one match after applying every
available context column is a multiple right-side match. It fails unless a
structured source declares `multiple_matches`, also under R008. A present
matched row whose `Value` is missing returns missing and does not invoke the
absent-item handler.

## Errors

- An unknown dataset identifier or variable, including an internal column of a
  derived dataset read from outside that dataset: fail.
- A dataset identifier equal to the output `domain`: fail.
- A source path that cannot be resolved: fail.
- An unresolved unqualified reference: fail.
- An ODM contextual reference with no available context column: fail.
- More than one ODM contextual match: fail unless locally handled.
- No ODM contextual match: fail unless locally handled.
