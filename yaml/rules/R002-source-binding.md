---
id: R002
title: Source Binding
status: draft
applies_to: [root.datasets, root.base, row.dataset, expression.source]
---

# Source binding

## Intent

Bind source files, row drivers, and source expressions without implicit
same-name inference.

## Dataset declarations

`datasets` maps dataset identifiers to source data paths. Identifiers are used
by `base`, `rows.dataset`, qualified source variables, and `mapping_from`.
Paths are resolved relative to the specification file.

Every referenced dataset identifier must exist in `datasets`. A dataset
identifier must not equal the output `domain`; unqualified names address the
dataset currently being derived, so reusing the domain would be ambiguous.

## Source expressions

The concise source form names one variable:

```yaml
source: DM.SEX
```

`DATASET.VARIABLE` refers to `VARIABLE` in the declared source dataset.
An unqualified reference such as `AVAL` refers to a variable in the output
currently being derived.

A qualified reference to the current row-driving dataset reads the current
source record. A qualified reference to another dataset follows R003.

Operations whose schema declares `source: [variable, expression]` accept the
same concise variable or a nested expression:

```yaml
mapping:
  source:
    str_extract:
      source: RAW.TEXT
      pattern: '^[A-Z]+'
  dict: {A: Alpha}
```

Plain strings outside fields typed as `variable` are literal strings.
Implementations must not infer same-named source variables when an output
column has no derivation.

## Structured source binding

The `source` expression also accepts an object containing `variable` and local
binding or join behavior:

```yaml
source:
  variable: ADSL.TRTSDT
  missing: {literal: null}
```

`filter` and `multiple_matches` are governed by R003. `missing` is governed by
R008.

## ODM contextual references

ODM item identifiers may contain periods. `ODM.IT.LB.LBDTC` means the `Value`
whose `ItemOID` is `IT.LB.LBDTC`, resolved within the current ODM context.

The exact context keys and zero-match or multiple-match behavior remain
unresolved, so this rule remains draft.

## Errors

- An unknown dataset identifier or variable: fail.
- A dataset identifier equal to the output `domain`: fail.
- A source path that cannot be resolved: fail.
- An unresolved unqualified reference: fail.
