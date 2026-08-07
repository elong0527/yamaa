---
id: R005
title: Output Contract
status: draft
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

The closed type vocabulary and conversion matrix remain unresolved.

## Output keys and verification

`keys` is an ordered list of declared output columns. After derivation and
conversion, combined key values must be non-missing and unique. R003 may use a
subset of keys for enrichment without changing final identity.

Column declaration order controls final output order. Column and dataset
verifications run as defined by R009.

## Errors

- Missing variable coverage: fail.
- An undeclared derivation target: fail.
- A duplicate structural identifier: fail.
- An unknown key column: fail.
- An unhandled type-conversion failure: fail.
- A missing or duplicate final key combination: fail.
- A failed verification: fail under R009.
