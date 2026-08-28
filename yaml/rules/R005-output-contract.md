---
id: R005
title: Output Contract
status: normative
applies_to: [root.keys, root.columns, column.type, row.derivations, derivation]
---

# Output contract

## Intent

Ensure every output value is defined, converted, verified, and uniquely
identified.

## Variable coverage

Every output column must have either a column derivation or a derivation in
every `rows` entry. Intentional missing values use `literal: null`.
Derivations targeting undeclared columns are invalid.

## Output and internal columns

A column is part of the output artifact unless it declares `output: false`.
An internal column is derived, converted, verified, and made available to
dependents exactly as an output column is; it is omitted from the artifact.

Internal columns exist so that a multi-step derivation does not have to publish
its own working values. They do not change evaluation. R001 builds one
dependency graph over all declared columns regardless of `output`, and an
output column may depend on an internal one.

The following apply to internal columns:

- `keys` must name output columns only. An internal column in `keys` is an
  error, because the key identifies rows in the artifact.
- Column verifications may be declared on an internal column and run normally.
- Dataset verifications may reference an internal column. They assert a
  property of the derivation, not of the artifact.
- Variable coverage is unchanged: an internal column still needs a derivation
  in every applicable place.
- Column declaration order controls output order among output columns.
  Internal columns are skipped without disturbing that order.

## Structural uniqueness

Implementations must reject duplicate YAML mapping keys. Dataset identifiers,
column names, row IDs, schema type names, and registry entries must be unique
within their applicable scope. Values in `keys` must not repeat.

## Type conversion and final handling

After an expression is complete, convert its result to the column's declared
`type` before dependents use it. Conversion must be deterministic and must not
silently replace errors with missing values.

A derivation that needs conversion or final handling uses the result wrapper:

```yaml
derivation:
  value:
    source: RAW.AGE
  conversion_failure: null
  override:
    - when: "USUBJID = 'SPECIAL-01'"
      value: {literal: 99}
```

`conversion_failure` is evaluated only when conversion fails. Overrides run in
listed order after conversion and use the first rule whose predicate is `TRUE`.
An override replacement is subject to the same conversion. R008 defines these
handlers.

R011 closes the column type vocabulary and defines the conversion matrix. The
decimal text an artifact writes for a `float` column is the float-to-text form
R011 defines: shortest round-trip by default, or the project's declared number
of decimal places. The artifact and a `str` column derived from the same
`float` therefore always agree, and rendering never changes a stored value.

## Output keys and verification

`keys` is an ordered list of declared output columns. A column declaring
`output: false` is not eligible. After derivation and
conversion, combined key values must be non-missing and unique. R003 may use a
subset of keys for enrichment without changing final identity.

Column declaration order controls final output order. Column and dataset
verifications run as defined by R009.

## Errors

- Missing variable coverage: fail.
- An undeclared derivation target: fail.
- An internal column named in `keys`: fail and report the column name.
- A duplicate structural identifier: fail.
- An unknown key column: fail.
- An unhandled type-conversion failure: fail.
- A missing or duplicate final key combination: fail.
- A failed verification: fail under R009.
