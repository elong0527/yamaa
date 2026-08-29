---
id: R015
title: Record Selection
status: normative
applies_to: [root.selections, selection_class, expression.source]
depends_on: [R001, R002, R003, R004, R005, R006, R007, R008, R014]
---

# Record selection

## Intent

Select one record of another dataset once, and read several of its columns,
so that the values a specification takes from that record are known to have
come from the same one.

## Boundaries

This rule owns the `selections` declaration: how a record is matched and
chosen, what its name means, and what an unmatched left row receives. R003
owns the implicit join a qualified source performs on its own, and R007 owns
`mapping_from` and the per-column `multiple_matches` relaxation. Neither
changes here: a selection reaches the same records by the same means and
differs only in being named once and read many times.

## Why a name

An expression returns one value, so every expression that reads another
dataset reaches its own record. Two columns that must describe one record —
a date and the sequence number identifying the record it came from, a value
and the unit it was measured in — therefore state their match twice and agree
only by construction. A reviewer cannot see the agreement, and an edit to one
statement and not the other breaks it silently.

A selection states the match once and gives the chosen record a name. The
columns that read it are then plainly reading one record.

## Declaration

Each entry of `selections` names a record:

```yaml
selections:
  - id: LASTEX
    dataset: EX
    filter: "EX.EXENDTC IS NOT NULL"
    order_by: [EX.EXENDTC, EX.EXSEQ]
    keep: last
```

`id` is a name in the same namespace as dataset identifiers, so it must not
equal a dataset identifier, another selection's `id`, or the output `domain`.

## Matching

A selection matches its `dataset` against each constructed output row:

1. `filter` selects eligible records. It is a predicate over records of the
   selection's dataset only, evaluated exactly as R003 evaluates the filter of
   a right-side reduction.
2. Eligible records are matched. When `source` and `key` are declared, they
   pair by position and match by equality, exactly as `mapping_from` does
   under R007. When neither is declared, the applicable output keys match,
   exactly as R003 defines them, and at least one is required.
3. When `order_by` and `keep` are declared, the surviving records are ordered
   by R007's order terms and `first` or `last` is retained; remaining ties are
   resolved by record order. When they are not declared, more than one
   surviving record fails.

`source` and `key` are declared together or not at all, and so are `order_by`
and `keep`.

## Reading a selection

A variable qualified by a selection `id` reads that column of the selected
record, in any field typed as `variable`:

```yaml
- name: RFXENDTC
  type: date
  derivation:
    source: LASTEX.EXENDTC
- name: EXDOSE0
  type: float
  derivation:
    source: LASTEX.EXDOSE
```

The named column must exist in the selection's dataset, and the value carries
the type R014 gives that field.

A selection is not evaluated ahead of the columns that read it. It resolves
where they do, so a column reading one depends on the selection's own `source`
variables under R001, exactly as a column using `mapping_from` depends on its
source variables. A selection's `filter` and `order_by` name records of its own
dataset and contribute no output-column dependency. Its `source` variables do
contribute dependencies, so R001 detects a cycle when a column reads a
selection whose match depends directly or indirectly on that column.

## When no record is selected

Two conditions leave a left row with no record, and they stay disjoint the way
R008 keeps them disjoint for `mapping_from`: an incomplete match value is
answered before any record is looked for, and an unmatched key is answered
after.

`incomplete` answers the first. A declared `source` whose value is missing
cannot be matched with anything, and the default is `fail`, because a lookup
that quietly returns nothing for an uncollected key reports an absent record
that was never looked for. It applies only where `source` is declared: output
keys are never missing, as R005 requires.

`unmatched` answers the second: a complete match value that no record carries.
`missing` gives every column that reads the selection a missing value, and
`fail` rejects the run.

Omitting `unmatched` keeps the behavior of the match the selection performs,
so replacing an existing derivation with a selection never changes what an
absent record does:

- matching on output keys defaults to `missing`, because R003 treats an absent
  right-side record as an ordinary missing enrichment;
- matching on a declared `source` and `key` defaults to `fail`, because R007
  makes an unmatched lookup key fatal unless the specification answers for it.

A selection that matched a record whose value is missing is a different case
from one that matched nothing. The first is a collected blank and the second is
an absent record, and `unmatched` answers only for the second.

## Errors

- A selection `id` equal to a dataset identifier, another selection `id`, or
  the output `domain`: fail.
- `source` without `key`, or `key` without `source`: fail.
- `order_by` without `keep`, or `keep` without `order_by`: fail.
- `source` and `key` lists of different lengths: fail under R007, which owns
  that pairing.
- No applicable key when neither `source` nor `key` is declared: fail under
  R003.
- More than one surviving record with no `order_by`: fail, as an unhandled
  multiple match under R003.
- A variable qualified by a selection `id` naming a column its dataset does not
  have: fail under R002.
- A missing declared `source` value where `incomplete` resolves to `fail`:
  fail, reporting the selection and the source that is missing.
- An unmatched left row where `unmatched` resolves to `fail`: fail, reporting
  the selection and the offending keys.
