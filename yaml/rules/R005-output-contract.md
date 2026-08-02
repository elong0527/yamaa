---
id: R005
title: Output Contract
status: draft
applies_to: [root.keys, root.columns, column.type, row.derivations]
---

# Output contract

## Intent

Ensure every output value is defined, typed, and uniquely identified.

## Variable coverage

Every output column must have either a column-level derivation or a derivation
in every `rows` entry. Intentional missing values use `literal: null`.

Derivations targeting undeclared output columns are invalid.

## Structural uniqueness

Implementations must reject duplicate YAML mapping keys. Dataset identifiers,
column names, and row IDs must be unique within their respective mappings or
lists. Values in `keys` must not repeat.

## Type conversion

After a value is derived, convert it to the column's declared `type` before it
is used by dependent derivations. Conversion must be deterministic and must not
replace errors with missing values.

The closed type vocabulary and conversion matrix remain unresolved.

## Output keys

`keys` is an ordered list of declared output columns. After all derivations and
type conversions, combined key values must be non-missing and unique. R003 may
use the subset of keys present in a referenced source dataset without changing
final output identity.

Column declaration order controls final output column order.

## Errors

- Missing variable coverage: fail.
- An undeclared derivation target: fail.
- A duplicate YAML key, dataset identifier, column name, row ID, or `keys`
  value: fail.
- An unknown key column: fail.
- A type conversion failure: fail.
- A missing or duplicate final key combination: fail.
