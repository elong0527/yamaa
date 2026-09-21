---
id: execution/lifecycle
title: Execution lifecycle
status: normative
---

# Execution lifecycle

## Purpose

Sequence resolution, row construction, dependency evaluation, value completion, and output checks.

## Scope and dependencies

The run follows this sequence: resolve and validate the specification and its
resources, construct rows, derive columns in dependency order, complete column
verification, validate output keys, run dataset verification, order the final
artifact, then serialize and publish it. Within either derivation phase, the
value stages below complete before a dependent reads that value. Row filters
and whole-column verification follow the timing specified below.

This contract owns the requirements below. Related contracts:

- [Local handlers](handlers.md).
- [Verification](verification.md).
- [Aggregation](../operations/aggregation.md).
- [Numeric computation](../operations/computation.md).
- [Expression evaluation](../operations/expressions.md).
- [Lookup and joins](../operations/lookup.md).
- [Predicates](../operations/predicates.md).
- [Text operations](../operations/text.md).
- [Specification composition](../specification/composition.md).
- [Types and conversion](../values/types.md).

## Requirements

### Phases

<a id="req-0031"></a>

**REQ-0031.** Derivation has two phases:

<a id="req-0032"></a>

**REQ-0032.** Row construction evaluates `rows` entries and may change row
   count.

<a id="req-0033"></a>

**REQ-0033.** Column derivation enriches constructed rows and must not
   change row count.

<a id="req-0044"></a>

**REQ-0044.** A column derivation must yield exactly one value per row, and
the derivation counts values rather than the records carrying them:
repeated readings of one value are that one value, and two input records
of one key combination carrying different present values are two values; that
outcome fails under [REQ-0075](lifecycle.md#req-0075). A source `filter` decides which of those
records the derivation reads before that count, which [REQ-0131](../operations/lookup.md#req-0131) defines. A
missing result is still the row's one value but never creates a second
value for the [REQ-0075](lifecycle.md#req-0075) count. In a specification without `rows`, a key
column derivation must not depend on a non-key output column ([REQ-0074](lifecycle.md#req-0074));
keys are derived before any row logic runs.

### Dependency execution

<a id="req-0048"></a>

**REQ-0048.** Implementations must infer dependencies to validate declaration
order and detect cycles. Recursively traverse each expression and collect:

<a id="req-0049"></a>

**REQ-0049.** every unqualified output variable referenced by `source`;

<a id="req-0050"></a>

**REQ-0050.** the `key_base` and `between.value` variables of a named lookup read by a
qualified reference, or the applicable output keys when matching fields
are omitted, under [Lookup and joins](../operations/lookup.md);

<a id="req-0051"></a>

**REQ-0051.** variables in `group_by`, `order_by`, and other fields typed as
  `variable`;

<a id="req-0052"></a>

**REQ-0052.** variables passed as leaves in `function.args`;

<a id="req-0053"></a>

**REQ-0053.** variables referenced by fields whose type contains nested
  `expression`;

<a id="req-0054"></a>

**REQ-0054.** current-output identifiers used by a `predicate` field;

<a id="req-0055"></a>

**REQ-0055.** current-output identifiers used by a `numeric_expression`;

<a id="req-0056"></a>

**REQ-0056.** identifiers used by an `aggregate_expression`;

<a id="req-0057"></a>

**REQ-0057.** variables used as placeholders in a `string_template`.

<a id="req-0058"></a>

**REQ-0058.** Predicates include `case` items' `when`,
`row.filter`, aggregate `filter`, and window `filter`. An ungrouped
`row.filter` resolves only qualified variables of that row template's input
dataset and runs before that row template's derivation graph. A grouped
`row.filter` resolves unqualified columns derived by that row template and
runs after that graph completes. Identifier extraction requires parsing the
predicate under the [Predicates](../operations/predicates.md) grammar, the numeric expression under the [Numeric computation](../operations/computation.md)
grammar, the string template under the [Text operations](../operations/text.md) grammar, and the reducer
expression under the [Aggregation](../operations/aggregation.md) grammar. An implementation must not treat any of
those four as dependency-free.

<a id="req-0059"></a>

**REQ-0059.** For each row template, evaluate row derivations using a
dependency graph. Row derivations cannot depend on values produced only
during the column phase. Every unqualified identifier in a grouped
`row.filter` must resolve to a column derived by that same row template. That
grouped `filter` is not a derivation, adds no graph edge between columns,
and runs only after all columns have completed.

<a id="req-0060"></a>

**REQ-0060.** After row construction, build the column dependency graph. Every
dependency must refer to a column declared earlier. Evaluate columns in
declaration order. When a specification declares `parents`, [Specification composition](../specification/composition.md) composes,
prunes, and orders the resolved columns before [Execution lifecycle](lifecycle.md) applies.
Declaration order is the resolved order.

<a id="req-0061"></a>

**REQ-0061.** The column dependency graph is over columns, not over rows. A
column reading another row in its own window partition depends on the whole
named column. A column that reaches its own value through another row
is a cycle rather than an iteration. `previous_non_missing` crosses any
number of missing rows by searching a separate completed source column.
Conventional carry-forward coalesces the current source with that search
result. Searching the column being derived remains a cycle rather than an
instruction to iterate.

### Derivation lifecycle

<a id="req-0211"></a>

**REQ-0211.** Every derived value passes through the same stages in this
order. Nothing consumes a value before its lifecycle is complete. A
dependent column, a verification, and the artifact
all see the same converted value.

<a id="req-0212"></a>

**REQ-0212.** Stage 1: evaluate the derivation's expression for one value,
under [Expression evaluation](../operations/expressions.md), with local handlers under [Local handlers](handlers.md).

<a id="req-0213"></a>

**REQ-0213.** Stage 2: convert the result to the column's declared `type`
for one value, under [Types and conversion](../values/types.md).

<a id="req-0214"></a>

**REQ-0214.** Stage 3: apply conversion-failure handling under [REQ-0359](handlers.md#req-0359)
for one value.

<a id="req-0215"></a>

**REQ-0215.** Stage 4: run the column's verifications over the whole column,
under [Verification](verification.md). An error stops execution. Warnings accumulate without changing
the column.

<a id="req-0216"></a>

**REQ-0216.** Stages 1 to 3 run on each value, in whichever phase its
derivation belongs to. Stage 4 runs once, after every row holds that
column's final value.

<a id="req-0217"></a>

**REQ-0217.** A row-level derivation therefore completes stages 1 to 3
during row construction, and a column derivation that depends on it reads a
converted value of the declared type. The declared type matters. [REQ-0004](../values/types.md#req-0004)
permits no implicit conversion between operation inputs, so an operation
consuming a row-derived column must rely on the column's declared type.

<a id="req-0218"></a>

**REQ-0218.** For a grouped row template, [Execution lifecycle](lifecycle.md) evaluates its `filter` after
stages 1 to 3 complete for every value on the candidate row. A discarded
candidate never enters the completed dataset, so stage 4 column
verifications do not include it. An error reached while deriving the
candidate still fails the run; the filter does not retroactively hide a
failed derivation.

<a id="req-0219"></a>

**REQ-0219.** A derivation that needs stage 3 wraps its expression
in `value`:

```yaml
derivation:
  value:
    source: RAW.AGE
  missing:
```

## Error conditions

<a id="req-0069"></a>

**REQ-0069.** A row dependency on a later-phase value: fail.

<a id="req-0070"></a>

**REQ-0070.** An unresolved variable or predicate reference: fail.

<a id="req-0071"></a>

**REQ-0071.** A reference to a later declared column: fail and report both
  columns.

<a id="req-0072"></a>

**REQ-0072.** A dependency cycle: fail and report the cycle path.

<a id="req-0073"></a>

**REQ-0073.** An expression that changes row count during column derivation:
  fail.

<a id="req-0074"></a>

**REQ-0074.** In a specification without `rows`, a key column derivation
  depending on a non-key output column: fail. Keys are derived before any row
  logic runs.

<a id="req-0075"></a>

**REQ-0075.** A column derivation yielding more than one value for one key
  combination: fail and report the column, how many values it yielded, and
  the keys. Missing results are excluded from the count. A source declaring
  `multiple_matches` keeps one of the records carrying those values instead
  of failing, which [REQ-0353](handlers.md#req-0353) defines.

<a id="req-0241"></a>

**REQ-0241.** A failed error-level verification: fail under [Verification](verification.md). A
warning-level violation leaves the primary artifact intact and enters [Verification](verification.md)'s
violation log.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [negative-adlb-computed-param](../../benchmarks/negative-adlb-computed-param/README.md).
- [negative-forward-reference](../../benchmarks/negative-forward-reference/README.md).
- [negative-first-available-self](../../benchmarks/negative-first-available-self/README.md).
- [negative-keys-conflict](../../benchmarks/negative-keys-conflict/README.md).

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Sequence resolution, row construction, dependency evaluation, value completion, and output checks. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
