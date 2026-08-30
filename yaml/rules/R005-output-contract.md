---
id: R005
title: Output Contract
status: normative
applies_to: [root.keys, root.columns, column.output, column.type, row.derivations, derivation]
depends_on: [R001, R002, R003, R007, R008, R009, R011]
---

# Output contract

## Intent

Ensure every declared column is derived in exactly one place, that every value
passes through the same ordered stages before anything consumes it, and that
the completed dataset is uniquely identified.

## Boundaries

This rule owns which columns exist, where each one is derived, the order of the
stages a value passes through, and output identity. It does not own what any
stage does: R011 defines a declared type and its conversions, R008 defines the
handlers, R009 defines the assertions, and R001 owns the two phases and the
dependency order within them.

## The artifact

The **artifact** is the single dataset this specification produces. Its columns
are the declared columns that do not set `output: false`, in declaration order,
and its rows are the rows R001 constructs.

Its serialization is only partly defined. R011 fixes the text form of each
non-missing value and states that a `float` renders the same way here as it does
when converted to `str`; the schema does not select a file container. The
example suite uses CSV and represents a missing value with an empty field. Row
sequence follows R001, but the language has no separate submission-sort
control, as gap 6 under T3 records
([#52](https://github.com/elong0527/yamaa/issues/52)).

Everything below concerns the values themselves, which are fully defined, and
none of it depends on the unsettled part.

## The column list is declared

The artifact's columns come from the specification and from nothing else. A
source carrying more of something than the specification declares does not
extend the artifact, and one carrying fewer does not shorten it.

This is a decision rather than an omission, and it binds wherever a CDISC
variable is one member of a numbered family: `SMQ01NAM`, `SMQ01CD`,
`SMQ02NAM`, and onwards, or `CRIT1` beside `CRIT1FL`. How many members a study
needs is a property of its reference data, but each member is a declared column
like any other, so the count is fixed when the specification is written. A
study whose reference data outgrows that count is re-read against the data
rather than left to fill the places it already has, and a second value
competing for one declared place is the ordinary multiple-match failure R003
defines rather than a new place. Whether a family's members may instead come
from data is open, and gap 7 under T5 records it
([#50](https://github.com/elong0527/yamaa/issues/50)).

## Column coverage

Every declared column is derived in exactly one place. Five requirements make
that precise, and they apply to internal columns exactly as they apply to
output ones:

1. **Every declared column must be derived.** A column with no derivation
   anywhere is an error. Implementations must not fall back to a same-named
   source variable; R002 forbids that inference.
2. **A column is derived either at column level or at row level, never both.**
   A column declaring `derivation` must not also appear in any `rows` entry's
   `derivations`, because the two would produce the same value twice with
   nothing to say which one survives.
3. **A row-derived column must be derived in every `rows` entry.** Deriving it
   in some entries and not others leaves the remaining constructed rows with no
   value for it, so partial row coverage is an error rather than an implied
   missing value.
4. **A specification with no `rows` entry must derive every column at column
   level.** Requirement 3 is vacuous when there are no entries, so this states
   the base-driven case directly.
5. **A `rows` derivation must target a declared column.** A key in
   `derivations` that names no declared column is an error.

Mixing the two placements across different columns is normal and expected: a
specification with `rows` typically derives the columns that distinguish its
row templates at row level and the rest at column level.

A column whose value is intentionally absent is still derived. Write
`literal: null` rather than omitting the derivation.

## Output and internal columns

A column is part of the artifact unless it declares `output: false`. An
internal column is derived, converted, verified, and made available to
dependents exactly as an output column is; it is only omitted from the
artifact.

Internal columns exist so that a multi-step derivation does not have to publish
its own working values. They do not change evaluation. R001 builds one
dependency graph over all declared columns regardless of `output`, and an
output column may depend on an internal one.

- Column coverage applies unchanged; an internal column still needs a
  derivation in exactly one place.
- `keys` must name output columns only. An internal column in `keys` is an
  error, because a key identifies rows in the artifact.
- Column verifications may be declared on an internal column and run normally.
- Dataset verifications may reference an internal column. They then assert a
  property of the derivation rather than of the artifact.
- Column declaration order controls artifact column order. Internal columns are
  skipped without disturbing that order.

## Derivation lifecycle

Every derived value passes through the same stages in this order. Nothing
consumes a value before its lifecycle is complete, so a dependent column, an
override predicate, a verification, and the artifact all see the same converted
value.

| # | Stage | Scope | Defined by |
|---|---|---|---|
| 1 | Evaluate the derivation's expression | one value | R007, with local handlers under R008 |
| 2 | Convert the result to the column's declared `type` | one value | R011 |
| 3 | On conversion failure, substitute `conversion_failure` and convert that | one value | R008 |
| 4 | Evaluate `override` predicates in order; convert the first match's value and stop | one value | R008 |
| 5 | Run the column's verifications | the whole column | R009 |

Stages 1 to 4 run on each value, in whichever phase its derivation belongs to.
Stage 5 runs once, after every row holds that column's final value.

A row-level derivation therefore completes stages 1 to 4 during row
construction, and a column derivation that depends on it reads a converted
value of the declared type. This matters because R007 permits no implicit
conversion between operation inputs: an operation consuming a row-derived
column must be able to rely on its declared type.

Conversion must be deterministic and must not silently replace an error with a
missing value. A conversion failure with no `conversion_failure` handler fails
the run.

A derivation that needs stage 3 or stage 4 wraps its expression in `value`:

```yaml
derivation:
  value:
    source: RAW.AGE
  conversion_failure: null
  override:
    - when: "USUBJID = 'SPECIAL-01'"
      value: {literal: 99}
```

## Output identity

`keys` is an ordered list of declared output columns and must name at least
one. A column declaring `output: false` is not eligible, and a column must not
be listed twice.

Once every column's lifecycle is complete, the combined key values of each row
must be non-missing and unique across the artifact. Key validation happens
before dataset verifications, which R009 runs last.

Key order is significant to R003, which joins on the output keys a right side
also carries. That is a subset used for enrichment and does not change the
identity asserted here.

## Specification-wide uniqueness

Within one specification, implementations must reject duplicate YAML mapping
keys, and dataset identifiers, column names, and row IDs must each be unique.
R006 owns the corresponding requirements for the schema bundle.

## Errors

- A declared column with no derivation: fail and report the column name.
- A column derived both at column level and in a `rows` entry: fail.
- A column derived in some `rows` entries but not all: fail and report the
  entries that omit it.
- A `rows` derivation naming an undeclared column: fail.
- An internal column named in `keys`: fail and report the column name.
- An empty `keys`, an unknown key column, or a repeated key column: fail.
- A duplicate YAML mapping key, dataset identifier, column name, or row ID:
  fail.
- A conversion failure with no `conversion_failure` handler: fail.
- A missing or duplicate combined key value: fail and report the offending
  rows.
- A failed verification: fail under R009.
