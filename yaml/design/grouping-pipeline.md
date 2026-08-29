# Design assessment: a `filter` → `group_by` → expression pipeline

```text
status:      assessment; no schema change proposed yet
relates_to:  issue #30, plan.md T5, plan.md T8, gaps 5, 6, 10, 11, 13
depends_on:  R001, R002, R003, R005, R007, R010
```

## Question

Should the schema gain a grouping pipeline of the kind polars and dplyr
provide — narrow the records, partition them, evaluate an expression per
partition — and if so, in what shape? Issue #30 proposes one form of it: a
single `aggregate` entry whose reducer is a text grammar. This document
assesses that proposal against the alternatives and against what the example
suite actually shows.

## Verdict

The schema already has the `filter` and `group_by` legs of the pipeline. It
does not have the third one, and the third one is not the reducer expression:
it is **the fold — a construct that produces rows at a new grain**. Every
limitation issue #30 lists, and five open gaps in `plan.md`, are consequences
of a single missing concept: a specification can name exactly one grain, its
own output, and one implicit derived grain, the right side of an R003 join
reduced at exactly the applicable output keys.

The recommendation is therefore:

1. **Do not adopt the reducer text grammar as the first move.** It addresses
   composition inside one reduction while leaving the grain problem untouched,
   and it pays the four costs issue #30 already priced.
2. **Assess a declared view** — a named, grouped, filtered derived relation
   inside the specification — as the primitive that closes the pipeline. It
   adds no expression semantics: every reduction stays a registered entry, and
   R003, R005, R007, and R011 apply to a view unchanged.
3. **Fold the assessment into T8 rather than issue #30.** A view and the
   multi-dataset manifest T8 needs are the same feature seen from two
   distances, and deciding them separately will produce two spellings of one
   idea, which is what the `lookup`/`mapping_from` lesson in `plan.md` warns
   against.

Nothing below is a schema proposal. It is the design work the acceptance rule
requires before one.

## What the pipeline is, verb by verb

| dplyr / polars | Schema today | Status |
|---|---|---|
| `filter()` on the source table | `row.filter` on the row driver | present |
| `filter()` on a joined-in table | aggregate `filter`, `multiple_matches.filter` (R003) | present |
| `filter()` after computing a column | — | **absent** (gap 13) |
| `mutate()` | a column with a scalar expression | present |
| `mutate(.by=)` / `.over()` | window `group_by`, aggregate context 2 (R007) | present |
| `summarise()` after `group_by()` | — | **absent** |
| implicit `summarise()` at join time | R003 right-side reduction, grain fixed to applicable keys | present, unnameable |
| `select()` / `rename()` | `columns`, `output: false` | present |
| chained frames | one specification, one dataset | **absent** (gap 10) |
| `pl.col("a").mul("b").sum()` | — | **absent** (issue #30's motivating case) |

Three observations follow from the table.

**The grouped mutate is already there.** R007's aggregate context 2 partitions
constructed output rows and broadcasts the reduction back to each of them,
which is exactly `mutate(x = sum(y), .by = g)`. `plan.md` gap 2 relies on it to
flag a tied set without a `rank` entry. Whatever is missing, it is not grouping
as such.

**The reduction that changes grain is there too, but cannot be named.** R003
reduces the right side of a join before matching. That reduction has a grain —
the applicable output keys — but the specification cannot state it, cannot
choose a different one, and cannot read more than one column out of it. It is
a `summarise()` whose `by` argument is inferred and whose result is
immediately consumed.

**Row construction can fan out but never fold.** R001 phase 1 already permits a
row count change, and `sdtm-relrec-many-to-many` and `adam-adlb-bds` use it to
emit several rows per driver record. No form of it emits one row per group of
driver records. The direction is available; only one sign of it is.

## Evidence in the suite

The fold is missing in a way the examples pay for rather than report.

- **`adam-adex-cumulative-dose` buys its grain from a file.** The output is one
  row per subject and treatment, and the grain comes from
  `input/subject_treatment.csv`, a hand-built inventory that no specification
  derives. `EX` already contains the distinct subject-treatment pairs; the
  inventory exists because nothing can group `EX` to produce them. It carries
  `PLANDOSE` and `PLANCYC` as well, so it is not pure scaffolding — but a
  reviewer cannot tell from the specification which half of the file is data
  and which half is a workaround.
- **`adam-adsl-population-flags` starts from `adsl_pre.csv`**, as three other
  examples start from `*_pre.csv`. Each is an earlier pipeline stage
  materialized outside the language, so its provenance is invisible to review
  and to cycle detection.
- **`sdtm-dm-reference-dates` cannot tie a value to its record** (gap 5). It
  takes the last exposure end date with `max` and the dose given at it with an
  ordered `source`, and keeps `EXDOSE0` internal only to show that the two
  agree because they order the same way. Nothing enforces it.
- **`sdtm-lb-conditional-compartments` cannot count rows within a group**
  (gap 11), so it cannot assert that every applicable subject has both
  compartments.
- **Issue #30's own two limitations** are grain statements: a right-side
  reduction can only group by the applicable output keys, so a subject total on
  a subject-by-treatment output needs a broadcast workaround; and a two-level
  reduction — total each cycle, then take the largest — has no intermediate
  grain to name.

The broadcast workaround deserves precision, because it is genuinely available
and genuinely partial. A subject total on a subject-by-treatment output is
`sum: {source: DOSECUM, group_by: [STUDYID, USUBJID]}` — a context-2 aggregate
over constructed rows. That is correct only when the reduction decomposes over
the partition and when the output rows cover the right side exactly. `sum` and
`count` decompose; `max` does; a distinct count does not; and any output whose
rows are filtered relative to `EX` silently reduces over a subset.

## The four candidate designs

### A. Keep the per-reducer registry (today)

Four entries, each pinning its own empty-group and all-missing result. The
semantics are stated where a reviewer meets them, R008 handlers attach to a
field, and R007 type-checks the source structurally.

It cannot express: any grain other than the output grain or the applicable
keys; arithmetic over right-side records before reduction; a value and its
identity from one record; a two-level reduction. Cost of keeping it: the suite
keeps buying grains from input files.

### B. One `aggregate` entry with a reducer grammar (issue #30)

```yaml
DOSECUM:
  derivation:
    aggregate:
      source: EX
      group_by: [STUDYID, USUBJID, EXTRT]
      expr: "SUM(EXDOSE)"
      filter: "EXDOSE IS NOT NULL"
```

Assessed on its own terms, issue #30's costs stand and this document adds
nothing to them: R008 has no operand to attach to when the body is
`SUM(a) + MAX(b)`; R007's input types stop being structurally checkable; and
`SUM(a) + b` raises a grain question the entry cannot answer.

The sharper objection is what the proposal does *not* buy. Its `group_by` is
the valuable half — it names a grain — but that half is separable from the
text grammar and does not require it. The text grammar half buys arithmetic
inside a reduction, which a record-grain view buys as an ordinary typed column
with handlers, verifications, and a name a reviewer can read. Adopting B first
would settle the reducer question and leave every grain limitation in place.

**Assessment: correctly deferred, and for a reason the issue understates.** The
issue defers it for lack of a second example; it should also be deferred
because it is aimed at the smaller half of the problem.

### C. Widen the right-side reduction to a coarser grain

Let an aggregate whose source is qualified declare the keys it joins on, as a
subset of the applicable keys:

```yaml
DOSESUBJ:
  sum:
    source: EX.EXDOSE
    join_on: [STUDYID, USUBJID]   # subset of applicable keys
```

This is cheap, structurally checkable, and closes exactly one of issue #30's
two limitations — the subject total on a subject-by-treatment output —
including for the non-decomposable reductions the broadcast workaround cannot
reach. It is four field additions, one per aggregate entry.

Two objections. The field name cannot be `group_by`, which already means the
output-row partition on the same entries, so the entry grows a second grouping
concept whose meaning depends on whether `source` is qualified. And a view
subsumes it entirely: a view grouping `EX` by subject joins to a
subject-by-treatment output on the applicable keys with no new field at all.

**Assessment: a legitimate interim, not a destination.** Take it only if a
second example needs a coarser-grain reduction before the view work lands.

### D. A declared view: the fold

Name a derived relation inside the specification, give it a driver, a filter, a
grain, and columns, and let every existing rule apply to it:

```yaml
views:
  EXCYC:                                  # a dataset_id, R002 namespace
    dataset: EX
    filter: "EX.EXDOSE IS NOT NULL"
    keys: [STUDYID, USUBJID, EXCYCLE]     # one row per distinct combination
    columns:
      - name: STUDYID
        type: str
        derivation: {source: EX.STUDYID}
      - name: USUBJID
        type: str
        derivation: {source: EX.USUBJID}
      - name: EXCYCLE
        type: int
        derivation: {source: EX.EXCYCLE}
      - name: CYCDOSE
        type: float
        derivation:
          sum:
            source: EX.EXDOSE
```

`EXCYC` is then an ordinary dataset identifier. A column of the output reads it
through the R003 join it already defines, and a second view may drive off it.
The three worked cases follow.

**Two-level reduction** — the largest cycle total per subject, issue #30's
second limitation:

```yaml
- name: MAXCYCD
  type: float
  derivation:
    max:
      source: EXCYC.CYCDOSE      # applicable keys: STUDYID, USUBJID
```

**Arithmetic before reduction** — `SUM(EXDOSE * EXDUR)`, issue #30's motivating
case, as a record-grain view feeding a grouped one:

```yaml
views:
  EXR:                                # record grain: no keys, no fold
    dataset: EX
    columns:
      - name: DOSEADM
        type: float
        derivation:
          compute: {expr: "EXDOSE * EXDUR"}
      # plus the key columns and EXSEQ
  EXSUM:
    dataset: EXR
    keys: [STUDYID, USUBJID]
    columns:
      - name: DOSETOT
        type: float
        derivation:
          sum: {source: EXR.DOSEADM}
```

The product is a named, typed, verifiable column rather than a subexpression,
R008 handlers attach to it, and R010 needs no aggregate vocabulary.

**A value and its identity from one record** — gap 5, without T5's
record-returning selection. Rank the records in a view, fold to the ranked one,
and let R003 uniqueness do the work:

```yaml
views:
  EXLAST:
    dataset: EX
    filter: "EX.EXENDTC IS NOT NULL"
    keys: [STUDYID, USUBJID]
    select:                             # keep one record per key, not reduce
      order_by: [{variable: EX.EXENDTC, direction: desc}, {variable: EX.EXSEQ, direction: desc}]
      keep: first
    columns:
      - name: EXENDTC
        type: date
        derivation: {source: EX.EXENDTC}
      - name: EXDOSE
        type: float
        derivation: {source: EX.EXDOSE}
      - name: EXSEQ
        type: int
        derivation: {source: EX.EXSEQ}
```

`sdtm-dm-reference-dates` then reads `EXLAST.EXENDTC` and `EXLAST.EXDOSE` and
they provably come from one record, because the view is unique on its keys by
construction and R003 checks that. This is the same guarantee T5 wants; whether
`select` belongs to the view or stays `multiple_matches` is an open question
below, not a settled part of this sketch.

**Assessment: this is the shape the pipeline actually needs.** It adds one
primitive — row construction at a declared grain — and reuses everything else.

## What D retires

| Item | Effect |
|---|---|
| Issue #30, limitation 1 | A reduction states its own grain |
| Issue #30, limitation 2 | Two-level reduction is two views |
| Issue #30, motivating case | Per-record arithmetic is a record-grain column |
| Gap 5 / T5 | Value and identity come from a view row R003 checks for uniqueness |
| Gap 6 | An empty view row and a present row of missing values are distinguishable, because the view row can carry a `count` |
| Gap 10 / T8 | Pipeline stages become declarable; the `*_pre.csv` inputs become derivable |
| Gap 11 (part) | Group counts are a view column, and a view can carry verifications |
| Gap 13 | A downstream stage filters on an upstream stage's derived columns |

Two cautions on that table. Gap 10 is only partly retired: views are internal
to one specification and do not by themselves let one run emit two artifacts,
which is the rest of T8. And gap 11's row-order half is untouched.

## The entry price: semantics that must be pinned

Issue #30 prices its own proposal at four decisions. A view prices out at eight,
and each is a rule sentence rather than a research question.

1. **A missing group key.** A driver record whose grain column is missing must
   fail, not form a group and not be dropped. R005 already requires non-missing
   key values, and R003 already states that a right record with a missing
   applicable key cannot match, so such a group would be unreachable. The
   specification filters those records explicitly or the run fails.
2. **An empty driver, or a filter that empties it.** Zero view rows. Downstream
   joins then see R003's absent match and yield missing, which is the behavior
   `count` already documents for an empty group.
3. **What a fold template may derive.** Only columns constant within the group —
   the grain columns and literals. Anything else is a column-phase reduction.
   This is structurally checkable: `source: EX.X` where `X` is not a grain
   column is an error.
4. **View row order.** First appearance of each group in driver order. Sorting
   by key would require a collation, and R004 leaves collation open.
5. **Where the filter lives.** A view's `filter` selects driver records; a
   reduction inside the view still declares its own `filter` if it needs one.
   Stating a narrowing twice is the cost of keeping each expression
   self-contained, and it is a place two statements can drift apart.
6. **Whether a view may select rather than reduce.** The `EXLAST` sketch above
   assumes it may. The alternative is to leave selection to
   `multiple_matches` and keep views purely reductive. This is the one decision
   that overlaps T5 rather than merely helping it.
7. **View verifications and internal status.** A view is never part of the
   artifact, so `output: false` has no meaning on its columns and R005's
   artifact section does not apply; its keys, lifecycle, and verifications do.
8. **Depth and cycles.** A view may drive off another view. R001 already builds
   one dependency graph and reports cycles; the graph gains a node per view.

Packaging is a ninth decision and belongs to T8: an inline `views:` block keeps
the reviewer in one document and needs no manifest or cross-file cycle
reporting, while a separate specification per stage is reusable across
specifications. The shape above is deliberately close to `root_class`, so a
view can be hoisted into its own specification later without rewriting it.

## What D does not solve

- **Row order in the artifact** (gap 11) and **multiple artifacts per run**
  (the rest of gap 10 and T8).
- **The reducer vocabulary.** Views make grains nameable; they add no
  reduction. `mean`, `median`, and a distinct count remain registry questions,
  and issue #30's semantic table — empty group, all-missing, null placement,
  overflow — must be pinned per entry either way.
- **Interval joins** (gap 4) and the structural items in group F.
- **Cost.** Every pipeline stage a reviewer must follow is a stage the current
  design does not have. A specification that grows three views to derive four
  columns is worse than one that buys a grain from an input file, and the
  acceptance rule exists to keep that from happening on speculation.

## Recommendation and sequencing

1. **Retarget issue #30.** Record that the reducer grammar stays deferred and
   that the grain question, not the reducer body, is the thing to decide. The
   issue's own "what would justify revisiting" already points here.
2. **Open T11 in `plan.md`** for the grouping pipeline and dataset grain, owned
   jointly with T5 and T8, with this document as its design work.
3. **Write the examples the acceptance rule requires before any field lands.**
   Positive: `adam-adex-cumulative-dose` rewritten so the subject-treatment
   grain is derived from `EX` rather than supplied by
   `input/subject_treatment.csv`; a two-level reduction; a per-record product
   reduced by a grouped view; a group count for
   `sdtm-lb-conditional-compartments`. Negative: a missing grain key with no
   filter; a fold template deriving a non-constant column; a view cycle; a
   downstream join to a view on a proper subset of its keys, which is the
   existing `negative-source-duplicate-right-key` contract generalized.
4. **Decide decision 6 with T5**, not before it. If selection returns a record,
   the view sketch simplifies; if it does not, views stay reductive and T5
   keeps its own construct.
5. **Take option C only under evidence**, as an interim, and never alongside D.

## References

- `yaml/rules/R001-execution-model.md` — the two phases; row construction may
  change row count, which is where a fold would live
- `yaml/rules/R003-cross-dataset-left-join.md` — the implicit reduction and its
  fixed grain
- `yaml/rules/R007-expression-registry.md` — the two aggregate contexts and the
  ordering terms a view would reuse
- `yaml/rules/R010-scalar-computation.md` — the closed-grammar precedent and its
  refusal of aggregates and qualified identifiers
- `yaml/examples/plan.md` — gaps 5, 6, 10, 11, 13; items T5 and T8; the
  acceptance rule
- issue #30 — the reducer-grammar proposal and its cost table
