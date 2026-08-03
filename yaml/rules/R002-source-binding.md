---
id: R002
title: Source Binding
status: draft
applies_to: [root.datasets, root.base, row.dataset, derivation.source, derivation.operations]
---

# Source binding

## Intent

Bind source files, row drivers, and variable references without implicit
same-name inference.

## Dataset declarations

`datasets` maps dataset identifiers to source data paths. Identifiers are used
by `base`, `rows.dataset`, and qualified variable references. Paths are resolved
relative to the specification file.

Every referenced dataset identifier must exist in `datasets`, including the one
named by `base` and by any `rows` entry's `dataset`.

A dataset identifier must not equal the output `domain`. The dataset being
derived is addressed by unqualified variable names, so reusing the domain name
for a source dataset makes a qualified reference read as though it addressed the
output. A source of raw adverse events feeding domain `AE` is declared as
`AE_RAW`.

## Variable references

`DATASET.VARIABLE` refers to `VARIABLE` in the declared source dataset
`DATASET`. An unqualified reference such as `AVAL` refers to a variable in the
dataset currently being derived.

A qualified reference to the current row-driving dataset reads the current
source record directly. A qualified reference to another dataset follows R003.

The same binding rules apply to every `{source: VARIABLE}` expression nested
inside an operation argument. Plain strings in operation arguments are values,
not variable references.

Implementations must not infer same-named source variables when an output
variable has no derivation.

## ODM contextual references

ODM item identifiers may contain periods. `ODM.IT.LB.LBDTC` means the `Value`
whose `ItemOID` is `IT.LB.LBDTC`, resolved within the current ODM context.

The current fixtures use study, subject, event, item-group, and repeat-key
fields as context. The exact context keys and zero-match or multiple-match
behavior remain unresolved, so this rule remains draft.

## Errors

- An unknown dataset identifier or variable, including in `base` or
  `rows.dataset`: fail.
- A dataset identifier equal to the output `domain`: fail.
- A source path that cannot be resolved: fail.
- An unresolved unqualified reference: fail.
