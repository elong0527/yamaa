---
id: R003
title: Cross-Dataset Left Join
status: normative
applies_to: [expression.source, expression.aggregate]

---

# Cross-dataset left join

## Intent

Enrich constructed rows from another dataset without repeating join keys.

## Boundaries

This rule owns the implicit join from a qualified cross-dataset source and
the source's right-side reduction. R007 owns `mapping_from`, whose keys are
declared, not derived from output `keys`, and window or row-construction
`filter` uses. R015 owns a named record lookup that reaches one record for
several columns. The named record lookup performs the join and has no other
way to reach a right side.

## Terminology

**R003-1.** Constructed output rows are the left side.

**R003-2.** The dataset named by a qualified source is the right side.

**R003-3.** Applicable keys (legacy mode) are output `keys` whose names also exist on the
right side.

## Rule

**R003-4.** In legacy mode, a qualified source referring to a dataset other than the
current row template's input dataset performs an automatic left
join during column derivation. The implementation must take these steps:

1. **R003-5.** Apply any right-side reduction described below.
2. **R003-6.** Select applicable keys in output `keys` order.
3. **R003-7.** Require at least one applicable key.
4. **R003-8.** Require right-side uniqueness on the applicable keys.
5. **R003-9.** Match equality on every applicable key.
6. **R003-10.** Copy the referenced value to matched left rows.
7. **R003-11.** Produce missing when a left row has no match.

**R003-12.** The join is many-to-one and preserves left row count and
order.

**R003-13.** Right records missing an applicable key cannot match. Key names
must match exactly. R019 defines string key equality.

**R003-13a.** The two sides of an applicable key must also carry mutually
comparable types under R007-31, and the match converts no operand. R014-4
gives an undeclared field of a typeless container the type `str`, so an
output key of another type matches such a field nowhere: every row would
receive missing and a complete right side would be reported as an absent
record. A key typed on one side and left to R014's default on the other is
a declaration to repair, not a comparison to widen. R007-19 states
the same between an operation's inputs, and R015-11 between a range's
operands; the join is no exception.

## Declared-key lookup

**R003-14.** `mapping_from` is not this join. Both are equality left joins
adding one column; they differ only in where the keys come from.

**R003-15.** In legacy mode, this rule's join derives keys from output `keys` that also
exist on the right side. `mapping_from` declares its pairs of source
variable and right-side column without consulting output `keys`, and so
reaches a right side keyed on something else or not unique on the
applicable keys. R007 defines the `mapping_from` semantics.

## Right-side reduction

**R003-16.** During column derivation, an aggregate expression whose
identifiers are qualified to a declared dataset reads the dataset as
the aggregate's right side.

**R003-17.** For each current row, applicable keys select a right-side
partition. R013 reduces eligible records in that partition to one value.
The reduced value joins back without changing row count.

**R003-18.** The qualifier may equal the current row template's
input dataset: a scalar source then reads the current input
record, while an aggregate reads that dataset. R007 registers
the expression and R013 defines the computation.

**R003-19.** The aggregate's optional `filter` selects which right-side
records enter that reduction:

```yaml
aggregate:
  expr: "MIN(EX.EXSTDTC)"
  filter: "EX.EXDOSE > 0"
```

**R003-20.** A reduction may declare a `group_by` coarser than the
applicable keys. The join then matches on `group_by` instead; R013
requires the columns to be output keys.

**R003-21.** A structured `source` declaring `multiple_matches` may also
declare `filter`. The `filter` selects which right-side records are
eligible before ordering, using the same evaluation as the aggregate form:

```yaml
source:
  variable: EX.EXTRT
  multiple_matches:
    order_by: [EX.EXSTDTC, EX.EXSEQ]
    keep: first
    filter: "EX.APERIOD = 1"
```

**R003-22.** In both places `filter` is a predicate over right-side
records only.

**R003-23.** A left row whose right side is empty after filtering has no
match and receives missing, as if no record existed.

**R003-24.** A qualified aggregate may also be narrowed by the current
left row.

**R003-25.** The aggregate's `between` declaration names one `value` the
current row reads and at least one `lower` or `upper` column on the right.

**R003-26.** A record is eligible when every declared comparison holds:
`lower <= value` and `value <= upper`. Every stated endpoint is
inclusive.

**R003-27.** A missing current-row value empties the aggregate's right
side for that row, and the result is missing; the missing value never
silently removes the narrowing.

**R003-28.** A right-side record missing a declared bound is ineligible.
R013 defines the aggregate contract.

**R003-29.** A reduction `filter` is not `row.filter`. R001 makes an
ungrouped row filter select input records before row derivation and a
grouped row filter select completed candidate groups; neither is a
right-side reduction filter.

## Multiple matches

**R003-30.** By default, multiple right-side matches fail.

**R003-31.** A structured source may declare `multiple_matches` as the
local, explicit relaxation defined by R008. The `order_by` uses the
order terms defined by R007, so a right-side selection declares
direction and null placement as a window does.

**R003-32.** An aggregate does not declare `multiple_matches`.

## Rationale

Join keys come from output `keys` so specifications do not repeat keys
for each cross-dataset reference, while `mapping_from` declares separate
pairs for right sides keyed on something else. Reduction yields at most
one record per grouping key, so an aggregate never meets multiple
matches. A reduction `group_by` coarser than the applicable keys changes
what the join matches on, which is why R013 requires those columns to be
output keys.

Requiring the two sides of an applicable key to be comparable answers the
same hazard as the key inference: a join that is well formed and unique but
still matches nothing reports a complete right side as an absent record.
Refusing the match makes the missing `types` declaration visible, where
converting an operand would hide it and leave every runtime free to convert
differently.

In new-style specs, key columns are ordinary derived columns whose derivations pin the key dataset and define the key relation directly. The same key-derivation recomputation drives filtered sources and aggregates, so cross-dataset correlation has one mechanism instead of two.

## Errors

**R003-33.** No applicable keys: fail.

**R003-34.** An applicable left key is unavailable: fail.

**R003-34a.** An applicable key whose two sides are not mutually comparable:
fail before any record is read, reporting the key, the dataset, and the type
each side declares. The condition is the incompatible-input error R007-38
owns, because the defect is a pair of operation inputs that cannot be
compared, not anything specific to this join.

**R003-35.** Multiple matches after reduction: fail unless locally
handled.

**R003-36.** No right-side match: return missing.

**R003-37.** An aggregate `between` with neither bound, a bound outside
the right-side relation, or operands whose types are not comparable:
fail under R013.

## Review

**R003-38.** In legacy mode, validation reports the inferred applicable keys for every
qualified source. A reviewer sees which same-named columns the join
matches on, the type each side declares for the columns, and the coarser
grain a reduction declared in place of the keys.

## Dataset declaration

**R003-39.** Top-level `input:` and `datasets:` are aliases for the dataset-declaration block; `input:` is canonical. Implementations accept both; exactly one must be present. Both present -> validation error; neither present -> validation error. `schema_version` stays `1.0`.

## Shape gating

**R003-40.** A spec is NEW-STYLE when every column named in top-level `keys:` carries a derivation. New-style specs use the key-dataset semantics below, and a `rows:` block is a validation error.

**R003-41.** LEGACY: any key column lacks a derivation -> legacy mode; `rows:` blocks and shared-name cross-dataset matching behave exactly as before.

## Key dataset and key relation

**R003-42.** Key columns are ordinary columns. A key derivation may be a `source`, a scalar expression (`literal`, `compute`, `case`, `mapping`, `coalesce`, `str_concat`, `cut`), or a window operation. All qualified identifiers appearing anywhere in the key derivations must name exactly one dataset S. Unqualified identifiers are permitted only when they name another key column (transitive same-row key->key references).

**R003-43.** Static enforcement: collect every qualified identifier from every key derivation. Identifiers qualified to different datasets within or across key derivations -> validation error (`key columns must all derive from a single dataset`). S is the single dataset named by those qualified identifiers. An unqualified identifier that does not name another key column is not a valid key-derivation reference; if it names a non-key output column, the legacy `key_dependency` contract (R001-43) applies instead.

**R003-44.** The key relation is the distinct tuples of the evaluated key derivations over S's rows. Each key derivation compiles once into a per-row function over S's columns; applied to all S rows, then deduped.

**R003-45.** Qualified source in a KEY column is constructive, not a join: project per row of S, then distinct.

## Structured source filter

**R003-46.** Structured `source` gains an optional `filter`: a string predicate over right-side records, evaluated exactly like the existing `multiple_matches.filter`. The structured source form (with or without `filter`) is accepted everywhere a source is accepted: `derivation.source`, `mapping.source`, `coalesce.sources[]` list items, plus the existing `multiple_matches.filter`.

**R003-47.** Evaluating `source: {filter: F, variable: D.C}`: D must equal the key dataset S, else a clear validation error, e.g. `filtered source references dataset 'D' but spec keys are derived from 'S': cross-dataset filtered sources are not supported`.

**R003-48.** The implementation filters D's rows by F, recomputes each surviving row's key tuple by applying the compiled key derivations, groups by key tuple, and left-joins to the key relation on tuple equality. Zero rows -> missing; one row -> the value; more than one -> error unless `multiple_matches` is declared.

**R003-49.** When `multiple_matches` is declared, the existing R003-30/31 `order_by`/`keep` behavior applies; its partition is now the recomputed key tuple.

**R003-50.** Unfiltered qualified source in a NON-key column (e.g. `source: ODM.SomeCol`) takes the same path with a no-op filter: more than one row per key tuple is an error. Ambiguity must be explicit.

**R003-51.** Unqualified `source: NAME` (same-row), `literal:`, `mapping:`, `coalesce:`, and scalar expressions keep existing semantics.

## Right-side reduction in new-style specs

**R003-52.** Aggregates (R003-16/17): in NEW-STYLE specs the applicable-keys partition is replaced by key-derivation recomputation -- one correlation mechanism shared with the filtered-source path. In LEGACY specs existing behavior is preserved exactly.

## Errors (new-style)

**R003-53.** Both `input:` and `datasets:` present: validation error.

**R003-54.** Neither `input:` nor `datasets:` present: validation error.

**R003-55.** `rows:` block in a new-style spec: validation error.

**R003-56.** Unqualified identifier in a key derivation that names neither another key column nor a non-key output column: validation error. Unqualified references to non-key output columns are governed by R001-43, not by this rule.

**R003-57.** Key derivations qualified to different datasets: validation error (`key columns must all derive from a single dataset`).

**R003-58.** Filtered source references dataset D but spec keys are derived from S: validation error (cross-dataset filtered sources are not supported).

**R003-59.** Unfiltered qualified non-key source with more than one row per key tuple: error unless locally handled.

## Review (new-style)

**R003-60.** In new-style specs, validation reports the key dataset, the compiled key derivations, and whether each qualified source declares a filter.
