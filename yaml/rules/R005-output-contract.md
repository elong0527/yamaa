---
id: R005
title: Output Contract
status: normative
applies_to: [root.keys, root.output, output.violation_log, root.columns,
  column.type, row.derivations, derivation]
---

# Output contract

## Intent

Ensure every declared column is derived in exactly one place. Ensure
every value passes the same ordered stages before use. Ensure the
completed dataset is uniquely identified.

## Boundaries

This rule owns which columns exist, where each column is derived, the
order of the stages a value passes through, output identity, and the
artifact's row order. This rule does not own what any stage does. R011
defines a declared type and its conversions. R008 defines the handlers.
R009 defines the assertions. R001 owns the two phases and the dependency
order within them. R007 owns what an order term means. R019 owns string
equality and order. R020 owns the bytes the artifact becomes once this
rule has completed and ordered it.

## The artifact

**R005-1.** The **primary artifact** is the dataset this specification derives.
Its columns are exactly the declared columns listed by `output.columns`, in
that order. Its rows are the rows R001 constructs. A specification may
also produce R009's governed warning-violation sidecar. That log reports
the run, not a second derivation target or source within this specification.

**R005-2.** Rows leave in the order `output.order_by` declares. Without
`output.order_by`, rows keep R001's construction order.

**R005-3.** Serialization is defined by R020. `output.path` names the primary
file and `output.violation_log`, when present, names R009's sidecar. Each
extension selects `parquet` or `csv`. R020 owns their containers, bytes, and
publication. Everything below concerns the primary values and their order,
which the profiles carry rather than decide.

## The column list is declared

**R005-4.** The artifact's columns come from the specification and from
nothing else. A source carrying more than the specification declares
does not extend the artifact, and a source carrying fewer does not
shorten the artifact. Each member of a numbered family (`SMQ01NAM`, `SMQ01CD`,
`SMQ02NAM`, and onwards, or `CRIT1` beside `CRIT1FL`) is a declared column
like any other, so the count is fixed when the specification is written. A
study whose reference data outgrows that count is re-read against the data
rather than left to fill the places it already has. A second value
competing for one declared place is the ordinary multiple-match failure
R003 defines, not a new place. The members of one family name their
grouping by position: `SMQ02NAM`, `SMQ02CD`, and `SMQ02SC` belong together
because each carries the `02`. Nothing in the schema links them beyond the
`02`. A study that wants the grouping checkable records it in the columns'
`metadata`; the schema does not.

## Column coverage

**R005-5.** Every declared column is derived in exactly one place. The five
requirements below make that precise, and they apply to internal columns
exactly as they apply to output ones.

**R005-6.** Every declared column must be derived. A column with no
derivation anywhere is an error. Implementations must not fall back to a
same-named source variable; R002 forbids that inference.

**R005-7.** A column is derived either at column level or at row level, never
both. A column declaring `derivation` must not also appear in any `rows`
entry's `derivations`. The two placements would produce the same value
twice, with no rule for which value survives.

**R005-8.** A row-derived column must be derived in every `rows` entry.
Deriving a column in only some entries leaves other constructed rows
with no value. Partial row coverage is an error, not an implied
missing value.

**R005-9.** A specification with no `rows` entry must derive every column at
column level. R005-8 is vacuous when there are no entries.

**R005-10.** A `rows` derivation must target a declared column. A key in
`derivations` that names no declared column is an error.

**R005-11.** Mixing placements across columns is normal: a specification
with `rows` typically derives the columns that distinguish its row
templates at row level and all other columns at column level.

**R005-12.** A column whose value is intentionally absent is still derived.
Write `literal: null` rather than omitting the derivation.

## Output and internal columns

**R005-13.** A column listed in `output.columns` is part of the artifact. Any
other declared column is internal: it is derived, converted, verified, and
shared with dependents exactly as an output column is, but omitted
from the artifact.

**R005-14.** Internal columns hold working values a multi-step derivation
does not publish. They do not change evaluation. R001 builds one dependency
graph over all declared columns regardless of `output`, and an output column
may depend on an internal one.

**R005-15.** Column coverage applies unchanged. An internal column still
needs a derivation in exactly one place.

**R005-16.** `keys` must name output columns only. An internal column in
`keys` is an error, because a key identifies rows in the artifact.

**R005-17.** Column verifications may be declared on an internal column and
run normally.

**R005-18.** Dataset verifications may reference an internal column. They
then assert a property of the derivation rather than of the artifact.

**R005-19.** `output.columns` must not repeat a column or name an undeclared
column. Its entries select the artifact columns and control their order.

## Derivation lifecycle

**R005-20.** Every derived value passes through the same stages in this
order. Nothing consumes a value before its lifecycle is complete. A
dependent column, an override predicate, a verification, and the artifact
all see the same converted value.

**R005-21.** Stage 1: evaluate the derivation's expression for one value,
under R007, with local handlers under R008.

**R005-22.** Stage 2: convert the result to the column's declared `type`
for one value, under R011.

**R005-23.** Stage 3: on conversion failure, substitute
`conversion_failure` and convert that, for one value, under R008.

**R005-24.** Stage 4: evaluate `override` predicates in order and convert
the first match's value, then stop, for one value, under R008.

**R005-25.** Stage 5: run the column's verifications over the whole column,
under R009. An error stops execution. Warnings accumulate without changing
the column.

**R005-26.** Stages 1 to 4 run on each value, in whichever phase its
derivation belongs to. Stage 5 runs once, after every row holds that
column's final value.

**R005-27.** A row-level derivation therefore completes stages 1 to 4
during row construction, and a column derivation that depends on it reads a
converted value of the declared type. The declared type matters. R007
permits no implicit conversion between operation inputs, so an operation
consuming a row-derived column must rely on the column's declared type.

**R005-28.** For a grouped row template, R001 evaluates its `filter` after
stages 1 to 4 complete for every value on the candidate row. A discarded
candidate never enters the completed dataset, so stage 5 column
verifications do not include it. An error reached while deriving the
candidate still fails the run; the filter does not retroactively hide a
failed derivation.

**R005-29.** Conversion must be deterministic and must not silently replace
an error with a missing value. A conversion failure with no
`conversion_failure` handler fails the run.

**R005-30.** A derivation that needs stage 3 or stage 4 wraps its expression
in `value`:

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

**R005-31.** `keys` is an ordered list of columns named in
`output.columns` and must name at least one. A column must not be listed
twice.

**R005-32.** Once every column's lifecycle is complete, the combined key
values of each row must be non-missing and unique across the artifact. Key
validation happens before dataset verifications, which R009 runs last.
String key values use R019 equality. Key order is significant to R003,
which joins on the output keys a right side also carries. The applicable
keys are used for enrichment and do not change the identity asserted here.

## Artifact row order

**R005-33.** `output.order_by` declares the order the artifact's rows are
presented in. It is optional. An artifact whose specification omits the
order keeps R001's construction order: row-template order, and input order
or first-occurrence group order within each row template.

**R005-34.** Its terms are R007's order terms. A bare variable is ascending
with missing values last. `direction` and `nulls` are declared per term.
`nulls` does not flip with `direction`. Each non-missing value takes the
order its type owns.

**R005-35.** A term may name any declared column, output or internal,
because a submission order often rests on a working value the artifact does
not publish: a numeric ordinal beside the text it labels, or a rank.

**R005-36.** Every term must name a declared column. No variable may be
repeated. A qualified source variable is not a declared column and has no
value on a completed row to order by. A repeated term states nothing the
first term did not.

**R005-37.** Rows equal on every declared term keep their construction order.
R007 applies the same tie-break to window ordering. The order is therefore
total for every input. No tie is an error. No comparison is undefined. No
specification declares a term merely to make the result deterministic. A
specification wanting a tie broken declares the term that breaks it.

**R005-38.** Ordering is presentation. It runs once, after the derivation
lifecycle, key validation, and every R009 verification, so it cannot
change whether a run passes or warns. It changes nothing about evaluation
either. R001's dependency order, a window's partitions, and the neighbours
`row_value` reads are all fixed before this order is applied. Each keeps
construction order for its own tie-break.

## Specification-wide uniqueness

**R005-39.** Within one specification, implementations must reject duplicate
YAML mapping keys. Dataset identifiers, column names, and row IDs must
each be unique. R006 owns the corresponding requirements for the schema
bundle.

**R005-40.** R017 matches identifiers across inheritance layers before this
rule applies, so a later layer may refine one inherited declaration.
Duplicate identifiers within a single layer remain an error. The resolved
specification contains one declaration for each identifier.

## Rationale

A fixed column list exposes excess dictionary entries as a specification-data
mismatch. The list does not silently change the artifact schema. The fixed list
lets a reviewer state key identity in advance. Family grouping by name position
remains a study design property. No portable construct can state the grouping.
An internal order term lets a specification withhold a working column. A reader
cannot always reproduce artifact order from the artifact alone.
Presentation ordering runs last and changes only the sequence a consumer
receives. R001 orders construction in base-record order. R014 makes a stored
artifact a source for another specification. A declared order lets a
two-specification workflow reproduce the same result rather than rely on the
runtime's file order.

## Errors

**R005-41.** A declared column with no derivation: fail and report the
column name.

**R005-42.** A column derived both at column level and in a `rows` entry:
fail.

**R005-43.** A column derived in some `rows` entries but not all: fail and
report the entries that omit it.

**R005-44.** A `rows` derivation naming an undeclared column: fail.

**R005-45.** An internal column named in `keys`: fail and report the column
name.

**R005-46.** A missing `output.columns`, a duplicate entry, or an entry
naming an undeclared column: fail and report the column name.

**R005-47.** An empty `keys`, an unknown key column, or a repeated key
column: fail.

**R005-48.** An `output.order_by` term naming anything but a declared
column: fail and report the term.

**R005-49.** An `output.order_by` variable declared more than once: fail and
report it.

**R005-50.** A duplicate YAML mapping key, dataset identifier, column name,
or row ID: fail.

**R005-51.** A conversion failure with no `conversion_failure` handler:
fail.

**R005-52.** A missing or duplicate combined key value: fail and report the
offending rows. A specification without `rows` emits one row per key
combination under R001-12, so a duplicate key can come only from row
templates emitting one combination more than once, under R001-12a.

**R005-53.** A failed error-level verification: fail under R009. A
warning-level violation leaves the primary artifact intact and enters R009's
violation log.
