# Partitioned execution design

This is the execution inventory and whole-table equivalence contract for
[#2683](https://github.com/elong0527/yamaa/issues/2683), and the partitioned
executor and the streamed runs built on it (#3697), with their end states, its
key-bounded partitions (#3763), and its run holding one decoded partition at a
time, with its confinement gate (#3830).
None of them changes language behavior; existing language requirements remain
authoritative.

Here a **data partition** is a physical subset of an immutable source snapshot,
with original source coordinates retained. In the current engine,
`table_grouping::partition` instead constructs **logical groups** by equal keys.
Its groups follow first source occurrence and their members retain source order.
Splitting a table into batches does not make those batches logical groups.

## Whole-table equivalence

Fix an admitted `ResolvedPlan`, ordered primary and secondary snapshots (including
schemas and missing values), terminology, function contracts, and the same
callback responses or failures at the same logical invocations. Let `W` be the
current `ResolvedPlan::execute` attempt. For every physical division of those
snapshots, including empty batches and a logical group spanning batches, a
partitioned attempt `P` must have the same observable result as `W`:

- **Values and schema:** identical column names, order and logical types, even
  for empty output; identical row multiplicity and typed cell values. Preserve
  integer precision, finite binary64 bits (including signed zero), text bytes,
  temporal values and retained precision, and missing versus present values.
  No tolerance or alternative floating-point reduction order is allowed.
- **Order:** retain template order, first-occurrence logical-group order, source
  member order, and stable donor/window tie breaking. Apply the final declared
  output order only after verification, using construction order to break ties.
  Verification row positions and key identities refer to the same pre-sort
  candidates as today; concatenating independently sorted batches is insufficient.
- **Diagnostics and evidence:** identical success/failure classification and
  ordered diagnostics, including condition/category, rule, ordered paths,
  exact structured context, optional source span and operand route, and original
  source/output coordinates. Preserve all `CheckRecord` counts, offending rows
  and values, codelist/inventory/count observations, handler declaration order
  and counts (including zeros), and completed checks retained before failure.
  Preserve the first fatal failure and the checks it prevents from executing.
- **Effects:** keep callback invocation order, arguments, cardinality, original
  exception identity and interrupt propagation. Do not evaluate unused branches,
  unused intermediates, or later callbacks speculatively. Equivalent execution
  must not turn an unobserved failure into an observed one.
- **Boundaries:** admission and limits apply to the same complete logical inputs
  and outputs. A batch cannot reset a table cap or admit a table that whole-table
  execution refuses. There are two exceptions, both D264's: a driver input that
  a partitioned run reads by partition (#3792), and the output of a partitioned
  run whose driver input is CSV or Parquet and whose plan divides every step
  (#3793). The row and cell limits bound each of their partitions instead of
  the whole table, so such a run can admit an input or an output that
  whole-table execution refuses. A SAS or ODM driver input and its output stay
  bounded whole.
  Keep phase entry/exit, outcome and refusal identity.
  Elapsed time and allocation measurements need not be equal. Incidental
  allocator/OS exhaustion cannot be made identical by a scheduling contract;
  it must still refuse without exposing a partial accepted dataset.

At the adapter boundary, the same accepted dataset must produce the same saved
artifact bytes under the same writer configuration. A failed attempt exposes no
accepted output. Cancellation is a distinct control-flow outcome; partitioning
must not convert it into a semantic diagnostic or publish provisional output.
[The partitioned executor](#the-partitioned-executor) states where it polls
and what it discards. Comparing successful rows alone does not prove
equivalence.

## Classification

**Safe** means row-local evaluation can be divided once all operands are complete,
with results and effects committed in whole-table order. **Safe-with-combine-step**
means local work requires the named ordered/global reconciliation before a result
or finding is committed. **Global** means the full logical relation or its ordered
state must remain available at that checkpoint; independent batch evaluation is
not valid. These are dependency classifications, not thread-safety promises.
An expression inherits the strongest dependency of its operands. Callbacks and
lazy reads also inherit the effect-order constraints above.

## Execution families

The inventory follows `Expression`, `RowMode` and the execution stages in
`rust/crates/yamaa-core/src/dataset/`.

| Family | Class | Required scope or reconciliation |
| --- | --- | --- |
| Plan/schema admission, function binding, source geometry and fixed limits | Global | Admit once in the current order before data reads, a driver input read by partition a partition at a time (#3792); retain complete-table row/cell/column counts and source-list positions. |
| `Source`, `Column`, `Literal`; `Compute`, `Round`, `Temporal`, `Imputation`, `Extreme`; string template, extract, case and pad | Safe | Complete operands first; retain conversion/handler order and exact arithmetic. Regex matching is linear and cannot be refused. A scalar wrapping a lookup or window inherits that dependency. |
| `Case`, `FirstAvailable`, `Flag`, `SourceBinding` and bound predicates | Safe | Keep authored operand order, three-valued truth, lazy branch/operand evaluation and binding-handler ownership. Never evaluate a suppressed alternative to fill a batch. |
| `MappingCut` (dictionary mapping and interval cut) | Safe | Admit dictionaries/breaks once; select independently from each completed source value with exact mixed-number comparison, then convert and record missing/unmapped handlers in order. |
| ODM reads (`Odm`) | Safe-with-combine-step | Use the same complete captured ODM relation and ordinal/provenance coordinates. Reconcile record eligibility and multiplicity before selecting a scalar; a chunk of XML or records is not the complete read scope. |
| Plain record templates and named/secondary drivers (`Driver`, `IntermediateDriver`) | Safe-with-combine-step | Enumerate drivers in original order and reassemble template-first candidates. Finish assignments before their pre-window filter, retaining failures on candidates later filtered out. Named drivers require their complete materialized relation. |
| Group templates (`Groups`) and `table_grouping::partition` | Safe-with-combine-step | Merge equal keys across physical boundaries, retain the minimum source position for group order and every increasing member position. Missing keys compare equal; text is exact; float zero signs and temporal precision do not distinguish grouping keys. |
| Key-grain rows (`Keys`) and `Collect` | Safe-with-combine-step | Finish the entire source filter before key lifecycles. Merge equal complete keys and distinct present raw readings across batches before conversion/choice. Preserve each missing-key record separately and the full feeding membership. |
| `Joined`, `SourceLookup`, `JoinedSource` | Global | Match against the complete donor relation, detect multiplicity across batches, and apply eligibility, stable order/keep and no-match behavior in the assignment's original phase. A local match cannot prove global uniqueness. |
| Named `Intermediate` reads, correlated derivations, range/filter selection | Global | Preserve the full eligible donor set and per-candidate choice/materialization. Resolve expression keys and correlated current values at their existing checkpoints; share a chosen record across reads without replaying handlers. |
| Pair relations (`inner_join`) | Global | Materialize matching pairs before intermediate verification, even when both sides are intermediates over one physical input. Strict consecutive and key-bounded partitions refuse this step; the whole-table fallback admits the input whole before materializing it. |
| `SELF` intermediates and template-to-template dependencies | Global | Later templates observe the completed prior-template relation. Publish/invalidate `SELF` state only at the existing template barrier; do not expose a partially constructed prior template. |
| Aggregate `COUNT` without `distinct` | Safe-with-combine-step | Combine exact present-value or record counts over the complete selected relation, with checked capacity and the distinction between no records (missing) and existing all-missing records (zero for value counts). |
| Aggregate `SUM`, `MEAN` | Global | Collect arguments before folding; replay the current ordered fold, including type/arithmetic error argument positions, missing handling, integer-to-float transitions and signed zero. Partial sums/means cannot be merged without changing semantics. |
| Aggregate `STR_JOIN`, and `COUNT(x)` with `distinct` | Global | Sort the complete selected relation by the declared order, ties in relation order, before joining; `distinct` keeps a text's first occurrence, or counts a value once, across the whole relation. Partial joins or distinct counts cannot be merged without changing semantics. |
| Aggregate `MEDIAN`, `Q1`, `Q3`, `QUANTILE`, `STDDEV`, `VARIANCE`, `GEOMEAN` | Global | Collect arguments before reducing. A quantile sorts the complete group's present values stably, equal values in relation order; the deviations replay the ordered `MEAN` and then the ordered sum of squared deviations, and the geometric mean the ordered sum of logarithms, keeping type/arithmetic error argument positions. Partial sorts or sums cannot be merged without changing semantics. |
| Aggregate `MIN`, `MAX`, `ONLY` and aggregate expressions | Global | Preserve relation order, original selected value/type on ties, first incompatible argument, and `ONLY` record cardinality including missing-valued records. Retain aggregate selection/filter and donor-derived dependencies before reduction. |
| `Window`, `ContextWindow`, nested and row-template windows | Global | Finish the complete logical window group in its declared scope before evaluation. Preserve pre-window versus post-window filters and group order; see the window inventory below. |
| `ProjectFunction` | Global | Use the same activated function port and ordered invocation/conversion sequence. Callbacks may be stateful or interrupt; do not assume purity, regroup calls or move them to host-incompatible threads. Chunked invocation must preserve its all-or-nothing cancellation contract. |
| Output key presence and uniqueness | Safe-with-combine-step | Combine missing-key rows and duplicate groups across all candidates, including templates; retain first group representatives and current key-failure order before dataset verification. |
| Final `output_order` | Global | Stable merge/sort the complete accepted relation after verification with the original construction position as final tie breaker. |

### Windows

All current `WindowKind` variants are global over their logical group. A complete
group may be scheduled as a unit once its input columns and required effects are
complete; a physical batch is not such a unit.

| Kind | Required complete-group state |
| --- | --- |
| `RowNumber` | Eligible member order and global numbering within that group. |
| `Competition`, `Dense` | Complete ordering, tie runs crossing batch boundaries, and preceding ranks. |
| `RowValue` | The donor at the signed offset, including donors outside the current batch. |
| `Locf` | Ordered preceding present donor; `include_current` decides whether the current row's own present value answers first. |
| `BaselineFlag` | All eligible candidate dates and reference dates, including the complete ambiguity count and original offending identity. |

Column windows observe the constructed output relation. Row-template windows
observe their template context, including grouped pre/post-window eligibility.
`ContextWindow` and windows used in intermediate match keys must preserve that
scope. `Run::begin_columns` invalidates affected choices/materializations when
key windows move from template scope to complete-output scope; carrying an old
batch cache across that barrier would be incorrect.

## Verification inventory

This lists every current variant of `dataset::Check`, including compiler
checkpoints. Row-local tests are classified safe-with-combine-step because the
observable check record is global. Combine in declaration/binding order only
after output keys and the where columns of value-level checks are complete, at
the existing column-prefix or dataset checkpoint. Preserve severity and prior
completed records on refusal; a combined report is not permission to defer a
fatal check past later derivations.

| `Check` variant | Class | Combine or global rule |
| --- | --- | --- |
| `NotMissing` | Safe-with-combine-step | Sum evaluated/failed counts and concatenate original offending identities in candidate order. |
| `AllowedValues` | Safe-with-combine-step | Same, preserving typed membership and missing-value rules. |
| `Codelist` | Safe-with-combine-step | Same, retaining terminology id and every offending raw value in order. |
| `Mandatory` | Safe-with-combine-step | Like `NotMissing`, over the rows an optional scope selects by value-level where clauses over completed columns. |
| `DataType` | Safe-with-combine-step | Check each present value's ISO 8601 form over the rows an optional scope selects, and combine ordered offenders as `NotMissing` does. |
| `ValueCodelists` | Safe-with-combine-step | Wait for the final where columns, then combine separately in binding order with unchanged default/override selection and codelist observations. |
| `Inventory` | Safe-with-combine-step | Check membership in the declared inventory, select an optional scope by the entries' where clauses over completed columns, and retain all offending values/identities. This does not infer an inventory from per-batch distinct sets. |
| `ValueMaxLength` | Safe-with-combine-step | Wait for the final where columns and preserve include/exclude selection and exact authored bound. |
| `ValueEntries` | Safe-with-combine-step | Wait for the final where columns, then combine ordered offenders with the entry ids each offending row matches. |
| `ValueEntryMatch` | Safe-with-combine-step | The artifact is the check's one unit: the entry matches when any partition holds a row its where clause selects. |
| `Range` | Safe-with-combine-step | Preserve exact numeric bounds/comparison and ordered offenders. |
| `MaxLength` | Safe-with-combine-step | Count Unicode scalars and combine ordered offenders. |
| `WideMaxLength` | Safe-with-combine-step | Same, comparing against the exact wide bound without machine-integer narrowing. |
| `Matches` | Safe-with-combine-step | Use the portable regex as written and combine ordered offenders; matching has no step budget. |
| `InvalidDiagnostic` | Global | Emit the stored diagnostic once at its declared checkpoint, preserving earlier records and preventing later work. |
| `InvalidDeclaration` | Global | Same, preserving the original code, reason and path. |
| `Assert` | Safe-with-combine-step | Validate both predicates even for empty output; evaluate `require` even when `when` is not true. Combine offenders/counts; intermediate reads retain their global selection dependency. |
| `PredicateDeclaration` | Global | Validate the declaration once at its checkpoint, including empty output, without emitting a check record or selecting donors. |
| `Unique` | Safe-with-combine-step | Merge complete typed tuples across batches, then emit repeated groups in first-occurrence order and every offending member in group order. Local uniqueness proves nothing about other batches. |
| `AllOrNone` | Safe-with-combine-step | Test presence together per row; combine counts and identities in candidate order. |
| `RowCount` | Safe-with-combine-step | Sum complete output cardinality and compare once; the record's evaluated count is one, including empty output. |
| `WideRowCount` | Safe-with-combine-step | Same, retaining exact authored bounds without narrowing. |
| `ScopedRowCount` | Safe-with-combine-step | Establish groups from all output rows before filtering. Combine selected counts, full denominators and whether any member binds `when`; preserve filtered-empty groups, exact fractional/count comparisons, first member identity and group order. Without `group_by`, empty output still has the whole-output group. |

## Execution seams and barriers

These are implementation seams for subsequent work, not new callable APIs.

1. **Admission to derivation:** `ResolvedPlan::execute` admits function
   bindings, schemas and complete source geometry before entering derivation.
   Capture stable source coordinates here; never bind schemas independently
   from incomplete batches. Each step of the resolved schedule carries its
   class from this inventory (`StepInfo::partition`).
2. **Driver/group construction:** the row stage of each template
   (`process_template` in `dataset/columnar/mod.rs`), `table_grouping::partition`
   and `keygrain::construct` separate driver membership from value evaluation.
   A future coordinator can combine memberships here, retaining first
   occurrences and then scheduling whole logical groups.
3. **Intermediate boundaries:** `relations::State` holds narrowed records, key
   indexes, choices and materialized donors for one attempt. Keep these
   attempt-local and use global selection identities, not batch-local
   counters. Coordinate complete donor relations and correlated materializations;
   retain lazy reads, `verify`, `publish_self` and `begin_columns` barriers.
4. **Template/window completion:** a template's window stage and post-window
   filter must finish before publishing its `SELF` relation or starting a
   dependent template. A window run requires complete memberships, order and
   eligible rows before answers become visible.
5. **Column completion:** `complete_columns` completes each binding over
   candidates and advances `check_columns` only through the available declared
   prefix. Local scalar evaluation can be divided inside a column, but its
   ordered effects and checks must finish before dependent/later work. Preserve
   the final unobserved-intermediate filter-type check, including empty output.
6. **Output keys to verification to ordering:** `verify_output` checks all key
   presence/uniqueness before dataset checks, and evaluates the predicate and
   scoped-count checks; `CheckRecord` retains global observations.
   `checks::output_order` runs last. This is the final reconciliation barrier
   before an accepted `Dataset` can leave core.
7. **Adapter result and Save:** core returns one `ExecutionAttempt`; adapters
   retain success/failure evidence and publish held bytes only through explicit
   Save. A streamed run builds its first batch only once all preceding barriers
   succeed ([Streamed runs](#streamed-runs)). [The end states](#end-states) fix
   what a cancelled or failed run and save leave behind.

## The partitioned executor

`ResolvedPlan::execute_partitioned` runs a plan over the partitions of its
driver input, consecutive physical subsets of one snapshot in order, which a
`PartitionSource` decodes one at a time, and `ResolvedPlan::execute_divided`
runs it over the partitions of an input it holds whole. The facade's
`EngineOptions::with_partitions` divides each build's driver input into
partitions of a given number of records (#3697), or into key-bounded
partitions ([below](#key-bounded-partitions), #3763). It is an engine mode
with no host surface, which every client sets (#3794, D264): the Python and R
bindings and the command line run every build over
`Partitions::by_keys(2, 624_640)` with the whole-table fallback, key-bounded
partitions of at most `Limits::PARTITION_ROWS` records over
`Limits::PARTITION_KEYS` leading keys, so that a run whose steps group by
subject divides and every other run runs whole as it would without
partitions. A run's measured observations name how it divided its driver
input: `whole_table`, `consecutive` or `key_bounded`, the leading keys its
partitions keep whole, the most records each holds and how many partitions
it ran. The saved run report does not carry it (P10). An embedder's engine
that sets no partitions runs every build whole.

- **One partition at a time.** The run decodes a partition, runs it through
  every derivation step as a whole-table run over its own records (its row
  templates or its key grain, then its column phase), keeps its candidate
  rows, and drops it before decoding the next (#3830). The output keys, the
  dataset checks and the final order run once, after the last partition,
  over every partition's candidates in the whole-table run's order: each
  template's rows, partition after partition. Every record keeps its
  coordinate in the whole input and every row its position in the whole
  output: a failure's positions, its partition's own, are moved past the
  records and rows of the partitions before it, so reads, failures and row
  identities name them as the whole-table run does. Admission checks the
  input's schema, and each partition within the row and cell limits on its
  own, before any record is read; every other source is admitted whole. A
  partition decoded with another schema fails the run.
- **Reading by partition.** A run whose specification reads one input, CSV
  or Parquet, and whose plan is safe under its partitions reads that input
  by partition (#3792). A scan decodes every partition once, in order, and
  drops it before decoding the next (#3830). The CSV reader scans every
  record, then decodes and converts one partition of records after another.
  The Parquet reader divides the file into runs of whole row groups, a row
  group of more records than a partition being a partition of its own, and
  decodes one run after another. Every finding is the whole read's, at the
  same record, row and field: the whole read runs each check over every
  partition before the next check, so the scan reports the finding whose
  check, then column, then partition comes first. Partitions cut between
  runs, key-bounded ones ([below](#key-bounded-partitions)) and consecutive
  ones under the confinement gate, are cut as the scan reads the records;
  other consecutive ones are the scan's own.
  Each partition is admitted on its own, then decoded again from the held
  bytes when the run reaches it, and dropped before the next is decoded; its
  generated ordinal counts records in the whole input. The run still holds
  the input's bytes, and its whole output (#3870). The observations'
  copy of the input holds no record: it is decoded again, a partition at a
  time, whenever the observations are serialized. Every other run reads
  each input whole and admits it whole, a run with a lookup, join or named
  relation's source included, so a plan with a step that needs the whole
  table, run with the fallback or not, refuses an input beyond the limits as
  an unpartitioned run does.
- **The whole-table first failure.** Every partition's run passes the same
  numbered marks in the same order, whatever its records hold
  (`columnar/partitions.rs`): one for each part of a step whose failures the
  whole-table run orders apart, such as a template's driver and row stage,
  the preparation and the evaluation of each of its windows, its post-window
  filter, and each column binding over each template's rows, in batch and
  replayed row by row after a failure, as the whole-table run does (#2829).
  The whole-table run finishes each part over every record before the next,
  so its first failure is at the earliest mark any partition fails at, in the
  earliest such partition: the earliest failing step and binding, then the
  earliest row in base-record order. Once a partition fails, every later
  partition runs only the marks before it. Rows of a template that does not
  read the driver input, driven by a secondary input or by a named
  intermediate over one, belong to no partition: the first partition's run
  builds them, and every later run passes their marks without building any.
  An output admitted whole, as a SAS or ODM driver input's is (D264), is
  checked as the whole-table run checks it: each partition's run records the
  rows its capacity checks see, and the run adds every partition's up at
  each check, so it refuses at the template, and with the count, the
  whole-table run refuses at.
- **Combine state.** Only the state this inventory marks combinable crosses
  partitions: the candidate rows the output is built from, appended
  partition after partition; the handler counts (`columnar/handlers.rs`),
  which each partition's run records at the marks where they change, so the
  run adds up the counts the whole-table run holds when it fails, the
  earlier partitions' through the failing mark and the later ones' before
  it; the template rows, added up; and the check records
  (`columnar/checks.rs`), which each check builds once, after the last
  partition, over every partition's candidates in candidate order, up to the
  first failure. The key groups of group templates and the key grain
  (`columnar/group.rs`, `table_grouping::Key`) never cross: each partition
  forms its own, and the confinement gate (below) keeps each group in one
  partition.
- **Global steps.** `ResolvedPlan::whole_table_step` names the first step
  the schedule marks `Global` whose evaluation reads a window, an aggregate,
  a named or joined relation, `SELF` rows or a project function, or the
  driver step of a template driven by a named intermediate whose
  derivations, filter or keys call a project function or are global
  themselves, because materializing it is. Without the
  whole-table fallback, such a plan fails at admission with
  `ExecutionError::PartitionGlobal`, which the facade reports as
  `engine_rejected` with `step: execute` and `code: partition_global` at that
  step's spec path. With the fallback, those steps run over the whole input.
  The run's own reconciliation points are not refused, because it holds the
  state they need: admission, intermediate verification without pair
  materialization, `SELF` publication,
  the final order and the declaration checks. Under key-bounded partitions a
  window or an aggregate that groups by every partition key is not such a
  step. A live pair relation materializes globally at the intermediate
  verification checkpoint and is refused at its intermediate's path under
  either strict partition mode, even when both sides read one physical input.
  The input is read and admitted whole; partitioning cannot bypass its row or
  cell limit before the join materializes it.
- **The confinement gate.** A run that holds one partition at a time
  (#3830) reads together only records one partition holds, so the records a
  grouped row template, the key grain or an ODM read of the driver input
  reads together must sit in one partition. Consecutive partitions then end
  only between runs of equal values of the driver fields every such read
  shares (`ResolvedPlan::cut_fields`): a grouped template's group fields,
  the fields the key grain's keys copy, and the fields that scope an ODM
  read. Each holds as many whole runs as fit in `rows` records, a longer run
  alone, and an input not grouped by those fields runs as one partition, as
  under [key-bounded partitions](#key-bounded-partitions); a plan with no
  such read divides every `rows` records. These steps need the whole table
  (`ResolvedPlan::whole_table_step`), so without the fallback they refuse
  the run as `partition_global`, and with it they run over the whole input:
  the driver of a template driven by a named intermediate over the driver
  input, and the verification of such an intermediate's `unique` checks,
  both at the intermediate's path, since their records span every
  partition; under key-bounded partitions, a grouped template, the key grain
  or an ODM read whose fields omit a partition field, at its `group_by`, its
  `keys` or its column's derivation, or any of them when the driver input
  does not hold every partition key; and under consecutive partitions, the
  first such read that leaves no field shared with every earlier one, as two
  templates grouped by `USUBJID` and by `SITEID` share none.
- **Cancellation.** The token is polled at admission, at every derivation
  checkpoint of each partition's run, at every checkpoint after the last
  partition, and before each batch of project-function calls. A cancelled
  run keeps nothing any partition built.

### Key-bounded partitions

`Partitions::by_keys(keys, rows)` divides each build's driver input so that no
partition splits a group of the specification's leading `keys` declared keys
(#3763). ADaM specifications declare `STUDYID` and `USUBJID` first, so two
keys make each partition a run of whole subjects.

- **The partition keys and their fields.** The partition keys are the
  leading `keys` declared keys, or every declared key when there are fewer; a
  specification that declares no key runs over consecutive partitions of
  `rows` records. The driver input holds a key when every row template that
  reads the driver
  input record by record derives it as a plain read of one driver field, the
  same field in each, whose type is the column's, so the read converts
  nothing. A grouped template's field is one of its group fields.
  `ResolvedPlan::partition_fields` names those fields; when the driver input
  does not hold every partition key, it names none.
- **The cut.** The records are grouped by the partition fields with the
  equality key groups use (`table_grouping::Key`: missing equals missing, the
  float zero signs are equal, an integer is not its decimal text). In the
  order of the records, each partition holds as many whole runs of equal
  values as fit in `rows` records, and a run longer than `rows` is a partition
  of its own. Stored order is kept (P5): the engine never reorders records.
  A run that reads its input whole makes the cut once admission has passed,
  so a run refused or cancelled at admission reads no record. A run that
  reads its input by partition makes it as its scan decodes the records,
  reading only the fields that hold the partition keys and keeping the key
  of every run that has ended, never a record (#3830); each partition is
  then admitted on its own.
- **One partition.** An input that is not grouped by its partition fields,
  where a value recurs after its run has ended, runs as one partition, as
  does an input whose partition fields cannot be read, or a specification
  whose driver input does not hold every partition key. The cut never
  selects non-contiguous records, because the first failure and the handler
  counts must follow base-record order; one partition keeps that order and
  lets a later reader decode the input by record range. A grouped
  template, the key grain or an ODM read that the partitions cannot keep
  whole refuses the run instead (the confinement gate of
  [the partitioned executor](#the-partitioned-executor)).
- **The classes.** Each step of the schedule carries its class under
  key-bounded partitions beside its class under consecutive ones
  (`StepInfo::keyed`): how many leading keys every window and aggregate it
  reads groups by, and its class when the partitions keep those keys' groups
  whole. A window or an aggregate whose grouping includes every partition
  key is `Safe` there; the step's class is then that of its other parts.
  The grouping is a window's `group_by` (WINDOW-3), the `group_by` of an
  unqualified aggregate, the group fields of the grouped row template an
  aggregate reduces, or the key of a qualified aggregate, which over the
  driver input must pair each partition key with the field that holds it, so
  every record it matches has the current row's partition keys (LOOKUP-18,
  LOOKUP-19). Each counts a key when it names the key's column or the field
  the key copies. A row of a template that does not read the driver input
  belongs to no partition, so a window or aggregate that reads one holds no
  key: a column window or an unqualified aggregate, which ranges over every
  candidate, as soon as one template does not read the driver input, and a
  row window or a qualified aggregate evaluated for such a template's rows.
  Every other step that needs the whole table stays `Global`: a named or
  joined relation, `SELF` rows, a project function, and a window or
  aggregate whose grouping omits a partition key.
- **Running a divided window or aggregate.** Each partition's run prepares
  such a window over its own rows, then evaluates them; an aggregate reads
  its partition's records. A failure while preparing a window still comes
  before any row is evaluated, in any partition, as in the whole-table run:
  the preparation and the evaluation pass marks of their own
  ([the whole-table first failure](#the-partitioned-executor)).

A window grouped by `[USUBJID, PARAMCD]` in a specification keyed `[STUDYID,
USUBJID, ...]` omits the first partition key, so it is refused under two keys
and under one: a `USUBJID` that two studies share would put one of its groups
in two partitions. Grouping it by `[STUDYID, USUBJID, PARAMCD]` admits it.

### Streamed runs

`SpecRequest::stream` runs a specification whose output its consumer reads as
a stream of Arrow batches (#3697), and the hosts' `run(stream = TRUE)` returns
such a run. The consumer's pull drives it:

- **The first pull builds the run**, whole or over the partitions, through
  every barrier above, so a batch exists only once the whole output is
  accepted.
- **Each later pull builds one batch** of the accepted rows. No batch is built
  ahead of a pull or kept once handed out, and there is no queue: the pull is
  the backpressure. A batch holds at most 65,536 records, or on an engine with
  partitions at most their `rows`, and never crosses a batch of the
  whole-table run's table, so the batches hold its values in its order
  (`public_table::PublicBatches` in the adapters builds them).
- **No table is built.** A build whose output is streamed checks that its
  table could be built, so it fails exactly when the whole-table build fails,
  and keeps only the accepted rows the batches are built from.
- **The token** is polled at every checkpoint of the schedule and before each
  batch. Releasing the reader that started the stream before its last batch
  cancels the run; a reader released after the last batch leaves the run its
  result. A batch that cannot be built ends the stream and leaves the run no
  result.
- **The host's thread.** When the run can call its host's project functions,
  only the thread that started it builds it; a pull on another thread fails
  with `host_thread` (the engine boundary design of #2870, section 3.6).

A run whose output nobody has started to read runs whole when its result is
asked for, and a reader then reads the table it keeps.

### Partitioned publication

A partitioned run whose driver input is CSV or Parquet, and whose plan has no
step that needs the whole table under its partitions, admits, encodes and
stages its output a partition at a time (#3793, D264): consecutive output
rows, in output order, each holding at most as many rows as the run's largest
driver partition (`Execution::partition_rows`). The run holds its output once
(#3870): the dataset keeps it a column at a time in the executor's typed
buffers, every present value of a column in one closed type and text in one
arena, and its encoders and its observations read that one copy. The
observations encode the output only when they are serialized. The host view,
the Arrow table a host reads, is built from it, and the encoded bytes stay
whole, since a later specification of a project reads them and a resolver's
`Stage` takes complete bytes, so the run's memory follows the output's values
in the dataset and the host view, and its encoded bytes. Wherever the output
is admitted, each partition is admitted on its own rather than the whole
output: the executor's row capacity, the output keys and the dataset checks'
groupings, the encoders, the host view, a stream's batches and the run's
observations. A fallback run that runs a whole-table step admits its output
whole, and so does a run over a SAS or ODM driver input, since the row and
cell limits bound such an input whole, and its output with it (D264).

- **CSV and Parquet, one partition at a time.** The build encodes the output
  one partition after another (`csv_artifact::render_partitions`,
  `parquet_artifact::render_partitions`), checking the saved-file limit as
  the bytes grow, and keeps where each partition's bytes end. A CSV record is
  row-local and its display precision per value, so the file is the
  whole-table render's. The Parquet writer holds rows until its row group of
  1,024 is full and writes the last group when the file ends, so the row
  groups, and the bytes D160 keeps, are the whole-table write's whatever the
  partition size.
- **The staged commit.** A save writes such an output into one staged file,
  opened once, appending each partition in turn (`file_publication::Writing`),
  and polls its token before each partition after the first; a caller's
  resolver, whose `Stage` takes complete bytes, is asked once every partition
  has passed. A cancellation or a failure there removes the partial staged
  file and every file staged before it, and publishes nothing (OUTPUT-8,
  D74).
- **The run report combined.** The report is built once, after the last
  partition, from the combined state: the handler counts and check records
  the executor combines, and the column facts, whose counts add up over the
  partitions and whose longest value is the longest in any. It is one table,
  admitted and written whole.
- **Written whole.** The run report, and a submission's SAS Transport and
  Dataset-JSON files, which state their record count in their header.

### End states

The cancel-or-fail by phase matrix is #3697's accepted contract. It derives
from D46, D74 and D186, OUTPUT-8, OUTPUT-10, PROJECT-12 and the staged commit
of `save()`.

| Phase | Cancel | Fail |
| --- | --- | --- |
| Before start, through admission | a `FailureKind::Cancelled` failure; no result, nothing staged or published | A failed result with no output, such as the `partition_global` refusal; its save writes only the report file |
| Mid-partition, through output keys and verification | a `FailureKind::Cancelled` failure; every partition's partial state is dropped; nothing staged or published | The whole-table run's first failure, handler counts and retained records; no output; its save writes only the report file |
| Between key-bounded partitions (#3763), through output keys and verification | a `FailureKind::Cancelled` failure, at every checkpoint and between the partitions of each divided unit, windows and aggregates included; every partition's partial state is dropped; nothing staged or published | The whole-table run's first failure, in base-record order whether the input divides or runs as one partition, with its handler counts and retained records; no output; its save writes only the report file |
| Mid-stream, while a reader pulls the batches of a streamed run | Releasing the reader or a token found before a batch ends production; no batch is retained; no result, so nothing published | A batch that cannot be produced, at the first pull a failed run's, ends the stream with the run's issue; its save writes only the report file |
| During publication | Polled before each partition of a run's output after the first (#3793), and once after every file is staged and before the first commit (D74): every staged file removed, nothing published | Between two partitions of the output, or before the first commit: nothing published, staged files removed. During the commit: earlier files stay replaced whole, later staged files removed, `Failure.paths` lists the files written |

In every cell a cancellation is a `FailureKind::Cancelled` failure with no
result and no issue (D186), and a failure is an issue of the run's result,
which names its phase; no target ever holds part of a file (OUTPUT-8).

## Proof obligations for a future executor

Compare both successful and failing attempts with the whole-table executor for
one-row batches, uneven batches, empty input/batches, groups and duplicate keys
split between batches, multiple templates, correlated and `SELF` reads, stable
ties, all-missing groups, and empty filtered groups. Exercise every window and
verification variant above. Use independently authored values and diagnostics,
including an ordered floating-point fold whose partial-sum merge differs, and
competing failures that reveal which checkpoint ran first. Vary batch boundaries
without changing the snapshots or expected result. Include callbacks that count
invocations, fail and interrupt, and assert no accepted/published partial output.

Existing benchmark input and expected data remain unchanged correctness oracles.
New execution strategies require these equivalence controls in addition to the
existing suites; this inventory alone does not establish an implementation.
The partitioned executor's controls are `yamaa-core`'s
`dataset/partitioned_execution.rs` and the facade's `partitioned_corpus.rs`,
which runs every single-specification benchmark over one-record partitions
and over key-bounded partitions of one key group each against its expected
files, reads each as a stream of one-record batches, and lists the ADaM
benchmarks key-bounded partitions must admit, and `end_states.rs`, one test
per cell of the end states. That a run holds one decoded partition at a time is
proved by `partitioned_execution.rs`'s partition source, which counts the
partitions it has decoded and not yet dropped, and by the facade's
`partitioned_memory.rs`, which measures a run's peak with a counting allocator
over ten partitions and over one, and shows that a run with a row per record
keeps at most twice what the same run read as a stream keeps, the dataset and
the encoded file, so it holds its output once (#3870). The partitioned
publication's are the facade's
`partitioned_publication.rs` and `partitioned_reading.rs`, the adapters'
`table_parquet_bytes.rs` and `table_arrow_table.rs` for the encoders, and
`file_application.rs`'s save that fails or is cancelled between two
partitions. The schedule's classes under key-bounded
partitions, and its confinement gate, are stated case by case in
`dataset/resolved/tests.rs`, and the facade's `partitioned_confinement.rs` runs
the gate's refusals, their fallbacks and the plans it admits. The streamed run's
are the facade's `streaming.rs`, the adapters' `table_public_batches.rs`, whose
counting source proves that a consumer that stops pulling stops production,
and the installed `streaming` suites of both hosts. The clients' default
partitions are the facade's `client_partitions.rs`, which reads each client's
values against core's, `partitioned_confinement.rs`, which reads how each run
divided its input from its measured observations, the hosts'
`longitudinal_capacity` suites, which read it for their default, and their
`partitioned_default` suites, which run 10,000,001 records under it in native
qualification.
