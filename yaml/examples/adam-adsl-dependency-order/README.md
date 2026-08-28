# ADaM ADSL dependency ordering

This focused probe answers one question: does declaration order have any effect
on evaluation?

## Rule and record grain

DM is the base, so each subject produces one ADSL row. The derivations form a
chain: `RANDDT` feeds `RANDFL`, which feeds `ITTFL`; `TRTSDT` feeds `SAFFL`;
and both flags feed `POPFL`. `AGE` feeds `AGEGR1` and `AGERNK`.

Every column is declared in the reverse of that order. `POPFL` is declared
first and depends on two columns declared after it. `STUDYID`, `USUBJID`, and
`AGE` are declared last even though nothing can be evaluated without them, and
the output keys refer to columns that appear near the end of the list.

The three subjects give the chain something to distinguish: one randomized and
treated, one randomized and treated with a different age band, and one neither
randomized nor treated.

## What the fixture proves

**Evaluation is topological, not textual.** R001 requires an implementation to
infer dependencies and evaluate in dependency order, with declaration order as
the tie-breaker only when several columns are ready at once. An implementation
that evaluated top to bottom would fail on the first column.

**Dependencies hide inside predicates.** `POPFL`, `SAFFL`, `ITTFL`, and
`RANDFL` name no variable in any expression field. Their entire dependency set
comes from identifiers inside `case.branches[].when`. R001 says a predicate
must not be treated as dependency-free, and this fixture is the case that
detects an implementation which does.

**Dependencies also hide inside window fields.** `AGERNK` depends on `AGE`
through `order_by` and on `STUDYID` through `group_by`, neither of which is an
expression.

**Layout is declaration order.** The golden file's first column is `POPFL` and
its key columns are ninth and tenth. That is deliberate: it shows that column
order in the output is the declared order and carries no evaluation meaning. A
real ADSL would not be laid out this way.

## Status and named gaps

This fixture is a **probe**. It covers the column-level half of X10 and names
what the schema cannot reach.

1. **The dataset-level graph does not exist.** X10 asks for one compact slice
   producing DM, EX, DS, AE, LB, ADSL, ADAE, and ADLB with declarations out of
   dependency order. One specification derives one dataset, so a
   cross-dataset graph cannot be declared, sorted, or cycle-checked. This
   fixture covers only the graph inside a single specification.
2. **Cycles cannot be demonstrated positively.** A cycle is a failure, and this
   suite has no negative fixtures yet. The cycle path reporting R001 requires
   is untested.
3. **There is no lineage diagnostic.** Nothing emits the dependency graph an
   implementation inferred, so two implementations could evaluate in different
   valid orders and neither could be inspected.
4. **Reuse is unobservable.** `RANDFL` is read by one column and `AGE` by three.
   Whether an implementation caches or recomputes cannot be seen from the
   output.

## Diagnostics and verifications

No handler path is declared. Rows remain in DM order; the key is
`[STUDYID, USUBJID]`; exactly three rows are expected. The analysis population
flag must imply both contributing flags.
