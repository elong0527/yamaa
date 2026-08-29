# Design decision: one `aggregate` expression over a closed reducer grammar

```text
status:      decided; schema change not yet written
decision:    replace min, max, sum, and count with one aggregate expression
relates_to:  issue #30, plan.md T5, plan.md T8, plan.md T11
depends_on:  R001, R002, R003, R004, R005, R006, R007, R008, R010, R011
```

## Question

Should the schema gain a grouping pipeline of the kind polars and dplyr
provide — narrow the records, partition them, evaluate an expression per
partition — and if so, in what shape? Issue #30 proposes one form of it: a
single `aggregate` entry whose reducer is a closed text grammar.

## Decision

**Adopt the reducer grammar.** One `aggregate` expression replaces `min`,
`max`, `sum`, and `count`, and one rule modelled on R010 closes its grammar,
its reducer vocabulary, and its result semantics. A fifth reducer becomes one
row in a table rather than a fifth registry entry, and arithmetic over
right-side records inside a reduction — the capability issue #30 names as its
real motivation — becomes expressible.

This is the same move `compute` made when it replaced `add`, `subtract`,
`multiply`, and `percent_change`, and `plan.md` records that outcome as a
lesson worth repeating.

The decision is deliberately narrower than the question. A reducer grammar
computes values; it does not produce rows, so it closes neither the fold nor
every grain limitation issue #30 lists alongside its proposal. Those stay open
as T11 and are stated in full below, so that a later reader does not mistake
the reducer work for the pipeline work.

## What the pipeline is, verb by verb

| dplyr / polars | Schema today | After this change |
|---|---|---|
| `filter()` on the source table | `row.filter` on the row driver | unchanged |
| `filter()` on a joined-in table | aggregate `filter`, `multiple_matches.filter` (R003) | unchanged |
| `filter()` after computing a column | — | still absent (gap 13) |
| `mutate()` | a column with a scalar expression | unchanged |
| `mutate(.by=)` / `.over()` | window `group_by`, aggregate context 2 (R007) | unchanged |
| `summarise()` after `group_by()` | — | still absent (T11) |
| implicit `summarise()` at join time | R003 right-side reduction at the applicable keys | **gains a reducer body** |
| `select()` / `rename()` | `columns`, `output: false` | unchanged |
| chained frames | one specification, one dataset | still absent (gap 10) |
| `pl.col("a").mul("b").sum()` | — | **closed** |

Two observations stand behind the decision's narrowness.

**The grouped mutate is already there.** R007's aggregate context 2 partitions
constructed output rows and broadcasts the reduction back to each of them,
which is `mutate(x = sum(y), .by = g)`. Whatever is missing, it is not grouping
as such.

**Row construction can fan out but never fold.** R001 phase 1 already permits a
row count change, and `sdtm-relrec-many-to-many` and `adam-adlb-bds` use it to
emit several rows per driver record. No form of it emits one row per group. The
direction is available; only one sign of it is. That is T11, not this change.

## The chosen design

### Registration

```yaml
expressions:
    aggregate: # Reduces records to one value under R013.
        type: [aggregate_expression, aggregate_class]

aggregate_class:
    - expr:
        type: aggregate_expression
        required: true
        description: Closed reducer expression over one relation.
    - group_by:
        type: "list[variable]"
        description: Grain of the reduction; defaults to the applicable keys.
    - filter:
        type: sql
        description: Predicate selecting records before reduction.

aggregate_expression:
    type: str
    min_length: 1
    description: Reducer expression in the portable grammar defined by R013.
```

R006's shorthand union carries the common case, exactly as it does for
`source`: a union of a non-class type with a class whose only required field
has that type accepts either form. `filter` stays a separate field typed `sql`,
so R004 keeps the predicate and only the reducer body becomes a new grammar.

```yaml
DOSECUM:  {aggregate: "SUM(EX.EXDOSE)"}
RFXSTDTC: {aggregate: {expr: "MIN(EX.EXSTDTC)", filter: "EX.EXSTDTC IS NOT NULL"}}
DOSEWTD:  {aggregate: "SUM(EX.EXDOSE * EX.EXDUR)"}
RDI:      {aggregate: "100 * SUM(EX.EXDOSE) / SUM(EX.PLANDOSE)"}
```

### The four rules that close it

1. **A closed reducer table.** `SUM`, `COUNT`, `MIN`, and `MAX`, plus
   `COUNT(DATASET.*)` for records rather than values. Any other name is a
   validation error. Widening requires amending the table, which is what makes
   a fifth reducer cheap and a second grammar unnecessary.
2. **No nesting; arithmetic only outside a reducer.** Operators and functions
   are R010's, by reference. An expression that is a single reducer call
   retains that reducer's result type, so `MIN(EX.EXSTDTC)` still returns a
   `date`; an expression using any operator requires numeric operands and
   inherits R010's promotion rules and failure conditions.
3. **The grain rule.** Every identifier must appear inside a reducer call
   unless it is one of the `group_by` columns. `SUM(a) / SUM(b)` is legal;
   `SUM(a) + b` fails unless `b` is grouped on. This is a parse-time check, and
   it answers issue #30's third objection.
4. **One relation per expression.** Every identifier is qualified to one
   dataset, which reduces that right side before the R003 join, or every
   identifier is unqualified, which reduces constructed output rows and
   broadcasts under R007 context 2. Mixing the two fails. Cross-grain
   arithmetic stays `compute` over named columns, so no formula performs a
   join and R010's ban on qualified identifiers is not reopened.

Rule 4 also removes any need for a dataset-typed field on the entry: the
relation comes from qualification, as it does everywhere else in the schema,
and `COUNT(EX.*)` gives the one expression that names no column a relation to
belong to.

### Semantics R013 pins

`SUM(EXDOSE)` in a string answers none of these, so the rule states each one.
The values are the schema's existing positions, carried over unchanged from the
four entries being replaced.

| Condition | Result |
|---|---|
| No matching record after `filter` | missing, as R003's absent match |
| Group whose values are all missing — `SUM`, `MIN`, `MAX` | missing, never zero |
| Group whose values are all missing — `COUNT` | `0`, because the records exist |
| `COUNT(D.*)` over a non-empty group | count of records |
| Integer overflow in `SUM` or in arithmetic | fail, as R010 |
| Result type | `COUNT` returns `int`; `SUM` retains its numeric type; `MIN` and `MAX` retain the reduced type |
| Input type | `SUM` requires numeric; `MIN` and `MAX` require mutually comparable values; `COUNT` accepts any type |

Null placement and collation, two rows of issue #30's comparison table, drop
out of the problem: a reducer ignores missing values and imposes no order.
Ordering stays with windows and `multiple_matches`, where R007 already owns it.

### Two deliberate deletions

**The aggregate `missing:` handler goes.** Under R008 it covers a source
variable that does not exist in context, and issue #30 correctly observes that
it has no operand to attach to once the body is an expression. No committed
example uses it, and an absent variable is already R002's unresolved-variable
error. R008's lifecycle table loses its aggregate row.

**`min`, `max`, `sum`, and `count` are removed, not deprecated.** R007's
aggregate paragraph names the single `aggregate` keyword instead, and its type
table keeps the same per-reducer statements against the new table.

### What the other rules gain

- **R001** adds `aggregate_expression` to the grammars an implementation must
  parse for identifier extraction, one line beside R010 and R012.
- **R003** keeps its right-side reduction unchanged; only the reducer's
  spelling moves. Its `filter` example is restated in the new form.
- **R007** replaces four entries with one in its aggregate paragraph, its type
  table, and its error list.
- **R013** is new and owns the grammar, the reducer table, the pinned
  semantics, and the failure conditions.

### Errors

- An `aggregate_expression` that does not parse: fail.
- A reducer name outside the table, or a wrong argument count: fail.
- A nested reducer call: fail.
- An identifier outside every reducer call that is not a `group_by` column:
  fail, reporting the identifier.
- Identifiers from more than one relation, or a mix of qualified and
  unqualified identifiers: fail.
- A `group_by` finer than the applicable keys in a right-side reduction: fail,
  because the result could not join back many-to-one.
- `SUM` over a non-numeric argument, or arithmetic over a non-numeric operand:
  fail.
- An aggregate outside its two permitted contexts: fail, as R007 already
  states.
- Any R010 failure condition reached through the arithmetic: fail.

## Migration

Twenty call sites across eight examples. Every one of them uses only `source`
and `filter`, so the rewrite is mechanical and no golden output moves:

| Today | After |
|---|---|
| `min: {source: EX.EXSTDTC, filter: P}` | `aggregate: {expr: "MIN(EX.EXSTDTC)", filter: P}` |
| `sum: {source: EX.EXDOSE}` | `aggregate: "SUM(EX.EXDOSE)"` |
| `count: {source: EX.EXSEQ}` | `aggregate: "COUNT(EX.EXSEQ)"` |

No committed example declares `group_by` on an aggregate, so nothing depends on
the field whose meaning this change makes explicit.

## What this change does not close

Stated plainly so it is not mistaken for solved. A reducer grammar computes
values; only a fold produces rows.

| Item | Status after this change |
|---|---|
| Issue #30's motivating case, `SUM(EXDOSE * EXDUR)` | **closed** |
| Issue #30 limitation 1, a reduction coarser than the output grain | **closed** by `group_by` |
| Issue #30 limitation 2, two-level reduction | open, T11 |
| Output rows at a derived grain — `adam-adex-cumulative-dose` still needs `input/subject_treatment.csv`, and four examples still start from a `*_pre.csv` | open, T11 |
| Gap 5, a value and its identity from one record | open, T5 |
| Gaps 10 and 13, chained stages and a filter after derivation | open, T8 |

T11 records the shapes considered for the fold and not taken now: adding
`group_by` to `row_class`, so the output itself is a grouped grain; a
summarise-only view, a named relation whose columns are all reductions; and a
full view, which also maps at record grain and is the only shape that reaches
gaps 5 and 13. This decision is compatible with all three, because each of them
reduces with whatever the registry offers. The reducer work is a down payment
on the fold rather than a detour.

## Options considered and not taken

- **Keep the per-reducer registry.** Rejected: a fifth reducer costs a fifth
  entry, and arithmetic inside a reduction stays unreachable.
- **Widen each entry with join keys instead**, as `join_on` over a subset of
  the applicable keys. Rejected: it buys only issue #30 limitation 1, it needs
  a field name that does not collide with the existing `group_by`, and the
  grammar's `group_by` subsumes it.
- **The fold, in any of its three shapes.** Deferred to T11 rather than
  rejected. It answers a different question, it is a larger change than the
  current evidence justifies, and the acceptance rule wants a second example
  first.
- **An embedded JSON payload.** Rejected: YAML 1.2 is a superset of JSON, so a
  string adds no expressiveness while costing R006's closed-field validation,
  R001's identifier extraction, and reviewable diffs. Every string-typed field
  the design has is a closed grammar owned by a rule, never a serialization
  escape.

## Negative examples this change requires

The acceptance rule needs failure behavior fixed before the feature lands.

| Example | Provokes |
|---|---|
| an `expr` naming a reducer outside the table | R013's closed vocabulary |
| a nested reducer, `MAX(SUM(EX.EXDOSE))` | R013's no-nesting rule, and the absence of two-level reduction |
| `SUM(EX.EXDOSE) + EX.EXSEQ` with `EXSEQ` not grouped on | R013's grain rule |
| an `expr` mixing `EX.` and `DS.` identifiers | R013's one-relation rule |
| an `expr` mixing a qualified identifier with an unqualified output column | the same rule across contexts |
| `SUM` over a string column | R013's input types, replacing the entry-level test |
| a `group_by` finer than the applicable keys | the join-back constraint |
| a window or `CASE` construct inside `expr` | R013's boundary against R004 and R007 |

Two contracts already listed in `plan.md` are re-pointed rather than added:
`sum` over a non-numeric source, and integer overflow.

## References

- issue #30 — the proposal, its cost table, and the deferral this decision
  reverses
- `yaml/rules/R010-scalar-computation.md` — the closed-grammar precedent this
  rule is modelled on
- `yaml/rules/R007-expression-registry.md` — the two aggregate contexts
- `yaml/rules/R003-cross-dataset-left-join.md` — the right-side reduction and
  its grain
- `yaml/rules/R006-schema-language.md` — the shorthand union carrying the
  one-line form
- `yaml/examples/plan.md` — T11 for the fold, T5 and T8 for the rest
