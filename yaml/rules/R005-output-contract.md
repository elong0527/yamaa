---
id: R005
title: Output Contract
status: normative
applies_to: [root.keys, root.output, root.columns, column.type,
  row.derivations, derivation]
---

# Output contract

## Intent

Ensure every declared column is derived in exactly one place, that every value
passes through the same ordered stages before anything consumes it, and that
the completed dataset is uniquely identified.

## Boundaries

This rule owns which columns exist, where each one is derived, the order of the
stages a value passes through, output identity, and the order the artifact's
rows are presented in. It does not own what any stage does: R011 defines a
declared type and its conversions, R008 defines the handlers, R009 defines the
assertions, and R001 owns the two phases and the dependency order within them.
R007 owns what an order term means. R019 owns string equality and order.
R020 owns the bytes the artifact becomes once this rule has completed and
ordered it.

## The artifact

**R005-1.** The **artifact** is the single dataset this specification
produces. Its columns are exactly the declared columns listed by
`output.columns`, in that order, and its rows are the rows R001 constructs.

**R005-2.** Its rows leave in the order `output.order_by` declares, and in
R001's construction order when it is omitted.

**R005-3.** Its serialization is defined by R020. `output.path` names the
file this specification produces and its extension selects one of two
profiles, `parquet` or `csv`. That rule owns the container, the bytes each
value becomes, the distinction between a missing value and a collected empty
string, the one display precision a `float` may take, and the replacement of
that file by a completed artifact. Everything below concerns the values
themselves and their order, which the profiles carry rather than decide.

## The column list is declared

**R005-4.** The artifact's columns come from the specification and from
nothing else. A source carrying more of something than the specification
declares does not extend the artifact, and one carrying fewer does not
shorten it. Each member of a numbered family (`SMQ01NAM`, `SMQ01CD`,
`SMQ02NAM`, and onwards, or `CRIT1` beside `CRIT1FL`) is a declared column
like any other, so the count is fixed when the specification is written. A
study whose reference data outgrows that count is re-read against the data
rather than left to fill the places it already has, and a second value
competing for one declared place is the ordinary multiple-match failure R003
defines rather than a new place. The members of one family name their
grouping by position: `SMQ02NAM`, `SMQ02CD`, and `SMQ02SC` belong together
because each carries the `02`, and nothing in the schema links them beyond
it. A study that wants the grouping checkable records it in the columns'
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
entry's `derivations`, because the two would produce the same value twice
with nothing to say which one survives.

**R005-8.** A row-derived column must be derived in every `rows` entry.
Deriving it in some entries and not others leaves the remaining constructed
rows with no value for it, so partial row coverage is an error rather than
an implied missing value.

**R005-9.** A specification with no `rows` entry must derive every column at
column level. Requirement R005-8 is vacuous when there are no entries, so
this states the base-driven case directly.

**R005-10.** A `rows` derivation must target a declared column. A key in
`derivations` that names no declared column is an error.

**R005-11.** Mixing the two placements across different columns is normal
and expected: a specification with `rows` typically derives the columns that
distinguish its row templates at row level and the rest at column level.

**R005-12.** A column whose value is intentionally absent is still derived.
Write `literal: null` rather than omitting the derivation.

## Output and internal columns

**R005-13.** A column listed in `output.columns` is part of the artifact. Any
other declared column is internal: it is derived, converted, verified, and
made available to dependents exactly as an output column is, but is omitted
from the artifact.

**R005-14.** Internal columns exist so that a multi-step derivation does not
have to publish its own working values. They do not change evaluation. R001
builds one dependency graph over all declared columns regardless of
`output`, and an output column may depend on an internal one.

**R005-15.** Column coverage applies unchanged; an internal column still
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
order. Nothing consumes a value before its lifecycle is complete, so a
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
under R009.

**R005-26.** Stages 1 to 4 run on each value, in whichever phase its
derivation belongs to. Stage 5 runs once, after every row holds that
column's final value.

**R005-27.** A row-level derivation therefore completes stages 1 to 4
during row construction, and a column derivation that depends on it reads a
converted value of the declared type. This matters because R007 permits no
implicit conversion between operation inputs: an operation consuming a
row-derived column must be able to rely on its declared type.

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
which joins on the output keys a right side also carries. That is a subset
used for enrichment and does not change the identity asserted here.

## Artifact row order

**R005-33.** `output.order_by` declares the order the artifact's rows are
presented in. It is optional, and an artifact whose specification omits it
keeps R001's construction order: row-template order, and driver or
first-occurrence group order within each template.

**R005-34.** Its terms are R007's order terms, so a bare variable is
ascending with missing values last, `direction` and `nulls` are declared per
term, `nulls` does not flip with `direction`, and each non-missing value
takes the order its type owns.

**R005-35.** A term may name any declared column, output or internal,
because a submission order often rests on a working value the artifact does
not publish: a numeric ordinal beside the text it labels, or a rank.

**R005-36.** Every term must name a declared column, and no variable may be
repeated. A qualified source variable is not a declared column and has no
value on a completed row to order by; a repeated term states nothing the
first one did not.

**R005-37.** Rows equal on every declared term keep their construction
order, which is the tie-break R007 already applies to window ordering. The
order is therefore total for every input: no tie is an error, no comparison
is undefined, and no specification declares a term merely to make the result
deterministic. One that wants a particular tie broken declares the term
that breaks it.

**R005-38.** Ordering is presentation. It happens once, after every value
has completed the lifecycle above, after key validation, and after every
verification R009 runs, so it cannot change whether a run passes. It changes
nothing about evaluation either: R001's dependency order, a window's
partitions, and the neighbours `row_value` reads are all fixed before this
order is applied, and each keeps construction order for its own tie-break.

## Specification-wide uniqueness

**R005-39.** Within one specification, implementations must reject duplicate
YAML mapping keys, and dataset identifiers, column names, and row IDs must
each be unique. R006 owns the corresponding requirements for the schema
bundle.

**R005-40.** R017 matches identifiers across inheritance layers before this
rule applies, so a later layer may refine one inherited declaration.
Duplicate identifiers within a single layer remain an error. The resolved
specification contains one declaration for each identifier.

## Rationale

A fixed, specification-declared column list keeps a dictionary that
outgrows its declared places as a loud specification-data mismatch instead
of a silent, data-dependent artifact schema. A key over such a list would
not be an identity a reviewer can state in advance, and family grouping by
name position stays a property of the study's design rather than of the
derivation because no portable construct can state it. Allowing an internal
column in an order term trades a small, stated cost -- a reader cannot
always reproduce the artifact order from the artifact alone -- for not
having to publish a column that exists only to withhold. Presentation
ordering comes last so that it settles only the sequence a consumer
receives; R001 drives row construction in base-record order, and R014's
producing-specification link makes a stored artifact the source another
specification reads that way, so a declared order is what lets a
two-specification workflow reproduce one result instead of leaving the
sequence to whichever runtime wrote the file.

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
offending rows. Under the R001 key-table grain the no-`rows` path emits at
most one row per key combination, so a duplicate key in the final table means
row templates emitted the same combination more than once.

**R005-53.** A failed verification: fail under R009.
