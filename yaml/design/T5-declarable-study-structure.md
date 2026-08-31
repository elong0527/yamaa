# T5 Design: declarable study structure

This document decides which of gaps 2, 7, 8, 9, 11, 12, and 16 the schema
closes and how. It lands before any schema change. Each closed gap names the
construct that retires it and the examples that fix its behavior; each gap left
open says what it still costs and what would change the answer.

The seven gaps share one shape: real protocol structure -- analysis windows,
treatment periods, query slots, administrations per dose, values and the
reasons behind them -- is re-expressed as filters, literals, and one row
template per slot, so the specification grows with the data instead of
describing the design. Two principles the schema already holds guide the
decision:

- **The specification is the artifact's definition.** A reviewer must be able
  to check the specification against the protocol without reading the data, and
  an implementation must be able to check the data against the specification.
  Structure that moves into data becomes uncheckable from the specification
  side; structure spelled out as literals becomes uncheckable from the data
  side. The design chooses which side each kind of structure is checked from.
- **Closed grammars over open escape hatches.** What the version 1.0 boundary
  confines to `function` stays there: no predicate, expression choice, or
  host-language code is carried as data.

## Summary of decisions

| Gap | Subject | Decision | Retires now |
|---|---|---|---|
| 2 | No interval join | Declared range matching on a record lookup | Yes |
| 7 | Protocol structure is not a concept | Fork answered: fixed at authoring time | Yes |
| 8 | Row construction cannot consume derived values | Rows drive from a completed private intermediate | Yes |
| 9 | A derivation cannot carry value and reason | A prepared selection dataset stores the rule beside its record | Yes |
| 11 | Right side cannot be narrowed by the current row | Row-narrowed matching and reduction | Yes |
| 12 | A reduction cannot consume a reduction | A named dataset at the intermediate grain | Yes |
| 16 | Row construction is append-only with a fixed count | An expansion step over a counted slot | Yes |

Two constructs carry the structural closures: **declared range matching**
(gaps 2 and 11) and **dataset-level row construction with a private
intermediate when one atomic build needs it** (gaps 8, 12, and 16). Gaps 7 and
9 need boundary decisions rather than new schema constructs.

## The constructs

### Declared range matching

A record lookup today matches by equality: applicable output keys under R003,
or declared `source` and `key` pairs under R015. An interval join is the same
lookup whose match additionally admits **range pairs**: one lower-bound and one
upper-bound column on the right, compared against one value visible to the
current row on the left.

```yaml
record_lookups:
  - id: AWIN
    dataset: AWINDOW
    source: [STUDYID]
    key: [STUDYID]
    between:
      value: ADY
      lower: AWLO
      upper: AWHI
    unmatched: missing
```

- `between.value` is any variable the current row can read -- a source field,
  a derived column, or an output key. It is a dependency of every column that
  reads the lookup, exactly as R015's `source` variables are.
- `between.lower` and `between.upper` name columns of the lookup's dataset. A
  record is eligible when `lower <= value <= upper`; the bounds are inclusive,
  matching the clinical convention that a window's endpoints belong to it.
- A missing `value` is an incomplete lookup and follows R015's `incomplete`
  policy. A missing bound on a right-side record makes that record ineligible;
  a complete value with no eligible record is `unmatched`.
- One-sided designs state one bound only. Stating `lower` alone matches
  `lower <= value`; stating `upper` alone matches `value <= upper`.

This is an interval join and nothing more. It does not express an arbitrary
predicate over both sides: the comparison is fixed, the two bounds name
right-side columns, and the value names one left-side variable. That
restriction is deliberate. A general two-sided predicate would re-open R004's
unresolved grammar over a second relation and would let a specification smuggle
a join condition past the review a declared `between` invites. The window
table, the EPOCH table, and the subject-specific cutoff below are all
expressible with one value between two bounds or against one bound.

The same pairs narrow a reduction. An `aggregate` whose identifiers are
qualified to a right side may declare `between` beside its `filter`; the bounds
narrow which right-side records enter the reduction for the current row, and
the value is again anything the current row reads. This is the form gap 11's
reduction needs:

```yaml
- name: NADIR
  type: float
  derivation:
    aggregate:
      expr: "MIN(ASSESS.AVAL)"
      filter: "ASSESS.ANL01FL = 'Y'"
      group_by: [ASSESS.STUDYID, ASSESS.USUBJID]
      between:
        value: ADT
        lower: ASSESS.ADT
```

`MIN(ASSESS.AVAL)` over the current subject's completed assessments dated on or
before the current row's analysis date is the RECIST nadir that
`adam-adtr-sum-of-target-diameters` cannot name today. The reduction joins
ASSESS on the applicable keys as R003 already defines; `between` only narrows
which of the subject's records enter it.

### Dataset grain and a private intermediate

R013 closes nesting because reducing at one grain and reducing that result at
another needs an intermediate relation no expression can name. The default
boundary for a durable or reused relation is another specification: its
materialized artifact is a declared source of each downstream specification.
That keeps independent datasets independently readable, testable, and
releasable. Pipeline orchestration supplies their execution order; source paths
do not imply it.

Some intermediates have no independent contract. They exist only so one atomic
artifact build can complete a private row grain before its next reduction or
row-construction step. The design names that case with a **derived dataset**:
a dataset built before the artifact, read like any declared dataset, and never
serialized. `adam-adtr-sum-of-target-diameters` needs one private `ASSESS`
grain because the artifact reduces completed assessments to the current nadir:

```yaml
datasets:
  TRVISIT: input/trvisit.csv
  TR: input/tr.csv

derived:
  - id: ASSESS
    base: TRVISIT
    keys: [STUDYID, USUBJID, AVISIT]
    columns:
      - {name: STUDYID, type: str, derivation: {source: TRVISIT.STUDYID}}
      - {name: USUBJID, type: str, derivation: {source: TRVISIT.USUBJID}}
      - {name: AVISIT, type: str, derivation: {source: TRVISIT.AVISIT}}
      - name: AVAL
        type: float
        derivation:
          aggregate:
            filter: "TR.TRGRPID = 'TARGET' AND TR.TRTESTCD = 'LDIAM'"
            expr: "SUM(TR.TRSTRESN)"
```

A derived dataset runs the same two phases as the artifact, in dependency
order before the columns that read it, and answers to R005's column coverage
and key identity exactly as the artifact does. Reading it is not a new join: a
qualified source into it is the ordinary R003 join, an aggregate over it is
the ordinary R013 reduction, and a record lookup over it is the ordinary R015
match. If `ASSESS` acquires another artifact consumer, it should be promoted to
its own specification instead of copied or retained as private work.

The artifact and a derived dataset have the same three mutually exclusive
row-construction forms. Ordinary `rows` behaves as before. `group_by`
constructs one row per distinct tuple of base variables, in first-occurrence
order; grouped base variables are the scalar fields the row carries while
reductions read the group's base records. `expand` is the counted form below.
With none of the three, a dataset has one row per base record. Making these
forms available on the artifact avoids a derived wrapper when the constructed
grain is already the result the specification exists to produce.

Two uses beyond gap 12 matter here, and they are what retire gaps 8 and 16.

**Row construction that consumes derived values (gap 8).** A `rows` entry's
`filter` sees only the row driver, so a logically removed record cannot be
dropped. With a derived dataset, the removal decision is a column on the
derived grain -- `sdtm-ae-effective-transaction`'s `TXNTYPE` -- and the
artifact's `rows` filter reads it:

```yaml
derived:
  - id: AE_EFFECTIVE
    base: AE_REC
    keys: [STUDYID, USUBJID, AESEQ]
    columns:
      # ... TXNTYPE from the EFFECTIVE record lookup, as today ...

rows:
  - id: kept
    dataset: AE_EFFECTIVE
    filter: "AE_EFFECTIVE.TXNTYPE <> 'REMOVE'"
    derivations: {}
```

The removed record never reaches the artifact, rather than being committed and
annotated. The rule this needs is that a `rows` filter over a derived dataset
reads that dataset's columns, which is true of any dataset the row driver
carries; the change is that the derived dataset exists to be driven from.

**Counted expansion (gap 16).** A `rows` entry appends one row per driver
record. The artifact can instead declare a counted number of rows per base
record directly:

```yaml
base: EX
expand:
  count: EX.EXDOSCNT
  as: ADOSEN
keys: [STUDYID, USUBJID, EXSEQ, ADOSEN]
columns:
  - {name: STUDYID, type: str, derivation: {source: EX.STUDYID}}
  - {name: USUBJID, type: str, derivation: {source: EX.USUBJID}}
  - {name: EXSEQ, type: int, derivation: {source: EX.EXSEQ}}
  - {name: ADOSEN, type: int}
```

`expand.count` is a variable resolving to a non-negative integer on each driver
record; the dataset holds that many rows per record, and the column
`expand.as` names carries the 1-based index of each, so `ADOSEN` runs 1 to the
count without a second derivation. A record whose count is missing fails during
row construction; a record whose count is zero contributes no row, which is
how a removed record and an unexpanded one stay distinct. The negative
example's failure -- a record holding more administrations than the templates
declare -- cannot recur, because the count is read from the record rather than
written out in advance.

The opposite motion in gap 16, adding a row where an expected combination has
none, is the same step with the sides reversed: the artifact expands its
schedule spine and then matches the collected records.
`adam-advs-once-measured-carry-forward` carries a value forward to an
unattended visit by constructing the visit row from the spine and letting the
collected value be missing there, rather than by repeating a collected record.

### Selection is an explicit dataset contract

Gap 9 is the decision that must produce a value, its reason, and the record
supporting both. That decision has independent clinical meaning and is better
expressed as a prepared dataset than embedded as branch language inside a
record lookup.

`adam-adrs-best-response-selection` prepares every assessment with `BORCAT`,
the response category it can support; `BORPRI`, that category's clinical
priority; and `BORSEQ`, its subject-level order by priority, date, and sequence.
The selected record is the ordinary stored record where `BORSEQ = 1`. The
downstream endpoint uses an ordinary lookup:

```yaml
datasets:
  ADRSSEL: input/adrs_selection.csv

record_lookups:
  - id: RESPONSE
    dataset: ADRSSEL
    filter: "ADRSSEL.BORSEQ = 1"
    unmatched: missing

columns:
  - name: AVALC
    type: str
    derivation: {source: RESPONSE.BORCAT}
  - name: ADT
    type: date
    derivation: {source: RESPONSE.ADT}
```

The category and date cannot drift because the upstream artifact stores them
on one identified assessment record. Its separate specification makes the
priority independently testable and reusable, while R015 remains one
operation: select one already prepared record.

`adam-adrs-composite-response` does not select among records once its decision
inputs are bound. One internal `RULE` column therefore uses the existing
`case`, and both `AVALC` and `ARSN` map from it. This keeps a same-row decision
in the artifact and a record-selection decision in its own dataset, without
adding a second conditional language to R015.

## Gap 7: the family fork is answered

Gap 7 asks whether a declared family's members are fixed at authoring time or
taken from data. **The decision is fixed at authoring time.**

R005 already states the artifact's column list is declared, and that a CDISC
numbered family such as `SMQ01NAM`, `SMQ01CD`, `SMQ02NAM` is a set of declared
columns whose count is fixed when the specification is written. The fork asked
whether that stays true. It does, for the reason R005 gives and the negative
example demonstrates: how many query slots a study needs is a property of its
query dictionary, and `negative-query-slot-overflow` shows that a dictionary
outgrowing the declared slots has no correct answer inside the run -- choosing
either query drops the other, and which was dropped would depend on storage
order. Letting the family's members come from data would move that collision
from a loud specification-dictionary mismatch into a silent data-dependent
column list, which is exactly what the declared-column contract exists to
prevent.

The fork's other half is what a study does when its dictionary outgrows the
declaration. The answer is the one R005 already names: the specification is
re-read against the data, and the new member is declared as a column like any
other. This is a deliberate cost. It keeps the artifact's schema checkable
against the protocol without running the derivation, and it keeps R005's key
identity well-defined, because a key over a data-dependent column list is not
an identity a reviewer can state.

What gap 7 retires beyond the fork is smaller than the gap statement implies.
Conditional applicability (`sdtm-lb-conditional-compartments`) and relationship
degree (`sdtm-relrec-many-to-many`) are real protocol structure, but the suite
shows they are adequately expressed as row templates and filters once the
counted-expansion step exists for the data-dependent cases. The remaining
naming problem -- that nothing links `TRT02A` to `TR02SDT` except the `02` --
is a documentation convention, not a schema concept: a study that wants the
grouping checkable declares a `metadata` note on the period's columns, and no
portable construct can do better, because the grouping is a property of the
study's design rather than of the derivation. Closing gap 7 means stating this
explicitly in R005 rather than leaving it as open text.

## What each gap costs after this design

- **Gap 2** retires. `sdtm-vs-visit-study-day` assigns `EPOCH` to an
  unscheduled visit by range-matching the study-day against the epoch table;
  `adam-advs-analysis-window-table` reads its window table rather than restating
  it as literals.
- **Gap 7** retires as a fork. The family's members stay fixed; the naming
  problem is documented as a convention.
- **Gap 8** retires. The logically removed record is filtered from a derived
  dataset before the artifact is constructed.
- **Gap 9** retires. A prepared selection dataset stores the clinical priority
  and result on the supporting record; an ordinary lookup reads that record.
  A same-row computed endpoint continues to name an internal rule with the
  existing `case` and `mapping` operations.
- **Gap 11** retires. The subject-specific cutoff narrows the reduction through
  `between`, and the analysis-window table matches through the same pairs.
- **Gap 12** retires. The intermediate grain is a named dataset: a separate
  specification when durable or reused, or a private derived dataset when one
  atomic artifact build consumes it.
- **Gap 16** retires. Expansion reads the count from the record; fill-in
  constructs rows from the spine.

## Worked examples

Each closed gap needs one positive example that needs the construct and one
negative or edge example fixing its failure behavior, per the acceptance rule
in #49. The examples below pair each construct with a data edge or an expected
failure.

| Construct | Positive example | Negative / edge example |
|---|---|---|
| Range matching | `sdtm-vs-visit-study-day` assigns `EPOCH` to an unscheduled visit and `adam-advs-analysis-window-table` reads the protocol windows | A missing study day follows the lookup's declared incomplete policy |
| Row-narrowed reduction | `adam-adtr-sum-of-target-diameters` derives the RECIST nadir | A reduction narrowed against a missing cutoff returns missing, never the whole right side |
| Prepared selection dataset | `adam-adrs-best-response-selection` declares the response order and `adam-adrs-best-overall-response` reads its selected record | A subject with no prepared record follows `unmatched` |
| Internal rule id | `adam-adrs-composite-response` maps value and reason from one `case` result | The final `otherwise` branch makes the default policy explicit |
| Private derived dataset | `adam-adtr-sum-of-target-diameters` names its assessment grain | `negative-adtr-duplicate-assessment` rejects a repeated intermediate key |
| Counted expansion and fill-in | `adam-adex-single-dose-expansion` expands every administration; `adam-advs-once-measured-carry-forward` builds the planned spine | `negative-adex-missing-dose-count` fails, while a zero count contributes no row |

## What this design does not do

- It does not widen the predicate grammar. R004 stays draft; `between` is a
  declared pair of bounds, not a predicate over two relations.
- It does not make column lists data-dependent. Gap 7's fork is answered the
  other way.
- It does not add a general loop or a host-language escape. Counted expansion
  is a declared count over one record, and what it cannot express stays behind
  `function`.
- It does not let a column read another row of the dataset being built. The
  derived dataset is built before its readers, so R001's no-sibling-reference
  rule is unchanged within each dataset.
- It does not retire gap 1. Literal operands are T1's question (#55), sequenced
  after this one; if a second use justifies a portable construct, it will be a
  keyed band table with the comparison still in the specification, and nothing
  here precludes it.

## Rule changes this design requires

- **R005** states the fork's answer where it now says the question is open,
  records the naming convention for numbered families, and treats an expansion
  index as a row-phase derivation.
- **R013** drops the sentence that an intermediate grain cannot be named, and
  points to a source or private derived dataset as that name.
- **R002** admits a derived dataset as a source its readers bind to, and states
  when it is built.
- **R003 and R015** admit `between` pairs beside equality keys, with the
  narrowing semantics above.
- **R001** gives artifacts and derived datasets the same row-construction
  modes, defines the private derived-dataset boundary, and adds its build to
  phase order and dependency inference.
- **R007** admits a qualified aggregate over the base group while a grouped
  artifact or intermediate is being built.

No rule is superseded. The changes are amendments that close what the rules
currently state as open.
