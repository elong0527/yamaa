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

After the last operation in a derivation, convert its result to the column's
declared `type` before it is used by dependent derivations. Conversion must be
deterministic and must not replace errors with missing values, unless the
derivation declares a `conversion_failure` exception, which R008 permits to
relax this requirement.

A `final` stage exception under R008 runs after conversion and replaces the
converted value. Its replacement is subject to the same conversion.

The closed type vocabulary and conversion matrix remain unresolved.

## Output keys

`keys` is an ordered list of declared output columns. After all derivations and
type conversions, combined key values must be non-missing and unique. R003 may
use the subset of keys present in a referenced source dataset without changing
final output identity.

Column declaration order controls final output column order.

Output rows are ordered by `keys`, ascending, after all derivations and type
conversions. Missing sorts last. This is separate from R001 construction order,
which appends rows in `rows` specification order: construction order determines
which record a window operation sees as first, while this ordering determines
only how the finished dataset is laid out.

Without this, a specification with several `rows` entries has no defined output
order. An ADLB built from one row template per parameter emits every record of
the first parameter before any record of the second, which is a legal but
unconventional layout, and two implementations could disagree.

## Errors

- Missing variable coverage: fail.
- An undeclared derivation target: fail.
- A duplicate YAML key, dataset identifier, column name, row ID, or `keys`
  value: fail.
- An unknown key column: fail.
- A type conversion failure with no `conversion_failure` exception: fail.
- A missing or duplicate final key combination: fail.

## Serialization

Comparing an implementation against an expected CSV requires a shared rendering.
An integer renders without a decimal point, a float renders without trailing
zeros, missing renders as an empty field, and a boolean renders as `TRUE` or
`FALSE`. The full serialization contract, including precision and how `date`
renders, is unresolved along with the type vocabulary above.
