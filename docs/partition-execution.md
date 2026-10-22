# Partitioned execution design

This is the execution inventory and whole-table equivalence contract for
[#2683](https://github.com/elong0527/yamaa/issues/2683). It guides the later
partitioned and streaming executors; it does not add either executor or change
language behavior. Existing language requirements remain authoritative.

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
  execution refuses. Keep phase entry/exit, outcome and refusal identity.
  Elapsed time and allocation measurements need not be equal. Incidental
  allocator/OS exhaustion cannot be made identical by a scheduling contract;
  it must still refuse without exposing a partial accepted dataset.

At the adapter boundary, the same accepted dataset must produce the same saved
artifact bytes under the same writer configuration. A failed attempt exposes no
accepted output. Cancellation is a distinct control-flow outcome; partitioning
must not convert it into a semantic diagnostic or publish provisional output.
The checkpoint and cleanup implementations follow in
[#2684-#2688](https://github.com/elong0527/yamaa/issues/2684); this document
does not supply them. Comparing successful rows alone does not prove equivalence.

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
| Plan/schema admission, function binding, source geometry and fixed limits | Global | Admit once in the current order before data reads; retain complete-table row/cell/column counts and source-list positions. |
| `Source`, `Column`, `Literal`; `Compute`, `Round`, `Temporal`, `Imputation`, `Extreme`; string template, extract, case and pad | Safe | Complete operands first; retain conversion/handler order and exact arithmetic. Regex matching is linear and cannot be refused. A scalar wrapping a lookup or window inherits that dependency. |
| `Case`, `FirstAvailable`, `Flag`, `SourceBinding` and bound predicates | Safe | Keep authored operand order, three-valued truth, lazy branch/operand evaluation and binding-handler ownership. Never evaluate a suppressed alternative to fill a batch. |
| `MappingCut` (dictionary mapping and interval cut) | Safe | Admit dictionaries/breaks once; select independently from each completed source value with exact mixed-number comparison, then convert and record missing/unmapped handlers in order. |
| ODM reads (`Odm`) | Safe-with-combine-step | Use the same complete captured ODM relation and ordinal/provenance coordinates. Reconcile record eligibility and multiplicity before selecting a scalar; a chunk of XML or records is not the complete read scope. |
| Plain record templates and named/secondary drivers (`Driver`, `IntermediateDriver`) | Safe-with-combine-step | Enumerate drivers in original order and reassemble template-first candidates. Finish assignments before their pre-window filter, retaining failures on candidates later filtered out. Named drivers require their complete materialized relation. |
| Group templates (`Groups`) and `table_grouping::partition` | Safe-with-combine-step | Merge equal keys across physical boundaries, retain the minimum source position for group order and every increasing member position. Missing keys compare equal; text is exact; float zero signs and temporal precision do not distinguish grouping keys. |
| Key-grain rows (`Keys`) and `Collect` | Safe-with-combine-step | Finish the entire source filter before key lifecycles. Merge equal complete keys and distinct present raw readings across batches before conversion/choice. Preserve each missing-key record separately and the full feeding membership. |
| `Joined`, `SourceLookup`, `JoinedSource` | Global | Match against the complete donor relation, detect multiplicity across batches, and apply eligibility, stable order/keep and no-match behavior in the assignment's original phase. A local match cannot prove global uniqueness. |
| Named `Intermediate` reads, correlated derivations, range/filter selection | Global | Preserve the full eligible donor set and per-candidate choice/materialization. Resolve expression keys and correlated current values at their existing checkpoints; share a chosen record across reads without replaying handlers. |
| `SELF` intermediates and template-to-template dependencies | Global | Later templates observe the completed prior-template relation. Publish/invalidate `SELF` state only at the existing template barrier; do not expose a partially constructed prior template. |
| Aggregate `COUNT` | Safe-with-combine-step | Combine exact present-value or record counts over the complete selected relation, with checked capacity and the distinction between no records (missing) and existing all-missing records (zero for value counts). |
| Aggregate `SUM`, `MEAN` | Global | Collect arguments before folding; replay the current ordered fold, including type/arithmetic error argument positions, missing handling, integer-to-float transitions and signed zero. Partial sums/means cannot be merged without changing semantics. |
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
   Save. Streaming batches must stay provisional until all preceding barriers
   succeed. Publication/cancellation cleanup belongs to the follow-up contract.

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
