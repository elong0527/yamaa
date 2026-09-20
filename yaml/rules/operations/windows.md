---
id: operations/windows
title: Windows
status: normative
---

# Windows

## Purpose

Partition completed output rows and compute ranks, neighbors, and baseline selections.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Aggregation](aggregation.md).
- [Expression evaluation](expressions.md).
- [Temporal values](../values/temporal.md).

## Requirements

### Evaluation kinds

<a id="req-0293"></a>

**REQ-0293.** Scalar expressions return one value per row. Window expressions
partition constructed output rows by their `window` specification's
`group_by` and preserve row count. Omitting `group_by` creates one
partition. Within a declared group, missing values equal other missing
values. Rows with equal present values and equal missing group positions
share one partition. A window partition is the KRC
section: a group of rows.

<a id="req-0294"></a>

**REQ-0294.** A window whose `window` declares `filter` still preserves row
count: an excluded row receives missing rather than being dropped. A window
that reads another row of its partition returns missing when that row does
not exist, the same result as for a neighbouring row with a missing value.

<a id="req-0296"></a>

**REQ-0296.** A window's `window.filter` narrows constructed output rows.
Aggregate filter scope is defined by [REQ-0471](aggregation.md#req-0471), permitted contexts by
[REQ-0467](aggregation.md#req-0467), and `between` applicability by [REQ-0474](aggregation.md#req-0474).

### Ordering

<a id="req-0303"></a>

**REQ-0303.** The tie-break settles positions, not equality. `row_number`,
`row_value`, `previous_non_missing`, and right-side selection read the
positions themselves, so a tie changes which row they reach. `rank` compares
only the declared terms. Records equal on all declared terms receive a
single number rather than the distinct numbers their positions would give.
The `competition` method leaves the positions occupied by a tie out of the
subsequent numbers. The `dense` method numbers distinct values
consecutively. A specification that wants a tie broken declares the term
that breaks it, whichever method it uses.

### Type behavior

<a id="req-0311"></a>

**REQ-0311.** `row_value` requires an integer `offset`; it and
`previous_non_missing` accept any `source` type and perform no coercion.

<a id="req-0316"></a>

**REQ-0316.** `row_number` and `rank` return integers; `baseline_flag` returns a string.
`row_value` and `previous_non_missing` retain the selected
value type and preserve a selected temporal value's collected precision.
Scalar selection follows [REQ-0315](expressions.md#req-0315); temporal operation results follow [REQ-0590](temporal.md#req-0590).

### Interface behavior

<a id="req-1122"></a>

**REQ-1122.** The `window_spec` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `window_spec.group_by` | Variables defining partitions; omission uses one partition. |
| `window_spec.order_by` | Terms ordering rows within each partition. |
| `window_spec.filter` | Predicate selecting rows before partitioning; excluded rows receive missing. |

<a id="req-1123"></a>

**REQ-1123.** The `expressions.row_number` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.row_number.window` | Partition, ordering, and row selection for the window. |
| `Result` | Numbers eligible rows from 1 within each partition. Filtering happens before partitioning; excluded rows receive missing. Ordering ties preserve row-template order, then base-record order. |

<a id="req-1124"></a>

**REQ-1124.** The `expressions.rank` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.rank.method` | Tie numbering; competition leaves position gaps and dense does not. |
| `expressions.rank.window` | Partition, ordering, and row selection for the window. |
| `Result` | Numbers eligible rows from 1 within each partition, sharing a number across ties. Rows equal on every order term take one number. With competition, they take the lowest position they occupy and the next distinct value takes its own position, so intervening numbers are skipped. With dense, the next distinct value takes the following number and no number is skipped. Two missing values are equal for this purpose, whatever nulls places them among. Filtering happens before partitioning; excluded rows receive missing. |

<a id="req-1125"></a>

**REQ-1125.** The `expressions.row_value` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.row_value.source` | Variable read from the offset row. |
| `expressions.row_value.offset` | Rows to move along the declared order; nonzero. |
| `expressions.row_value.window` | Partition, ordering, and row selection for the window. |
| `Result` | Returns the source value from another row of the ordered partition. offset counts along the declared order: positive moves toward later rows and negative toward earlier ones, so reversing every order_by direction and negating offset gives the same result. A row with fewer than \|offset\| rows on that side of it within its partition yields missing, which does not distinguish an absent row from a present row whose value is missing. |

<a id="req-1126"></a>

**REQ-1126.** The `expressions.previous_non_missing` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.previous_non_missing.source` | Variable searched for the closest earlier non-missing value. |
| `expressions.previous_non_missing.window` | Partition and ordering for the window. |
| `Result` | Returns the source from the closest strictly earlier non-missing row. The current row is never a candidate. Missing source values are skipped, so one result can cross any number of consecutive gaps. A row with no earlier non-missing source in its partition yields missing. |

<a id="req-1127"></a>

**REQ-1127.** The `expressions.baseline_flag` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.baseline_flag.date` | Candidate baseline date. |
| `expressions.baseline_flag.reference_date` | Inclusive upper bound for the baseline date. |
| `expressions.baseline_flag.window` | Baseline-selection partitions for the window. |
| `Result` | Returns Y for the unique latest eligible row, missing elsewhere. An eligible row has a non-missing date at or before reference_date. A tie for the latest eligible date is an error. |

<a id="req-1129"></a>

**REQ-1129.** The `module` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `Scope` | This contract defines partitioning and filtering; [Ordering](../execution/ordering.md) defines the ordering one window_spec declares; each window expression below nests it as `window:` and adds only its own specific fields. |

## Error conditions

<a id="req-0326"></a>

**REQ-0326.** A window expression used during row construction: fail.

<a id="req-0327"></a>

**REQ-0327.** A `window.filter` that is not a Boolean predicate over
current-output columns: fail.

<a id="req-0328"></a>

**REQ-0328.** A `row_value` whose `offset` is zero: fail. The current row's
own value is `source`, and a window must not be a second spelling of it.

<a id="req-0340"></a>

**REQ-0340.** `row_number`, `rank`, `row_value`, and `previous_non_missing`
require `window.order_by`: without a declared order the window has no
positions to number or to move along. Omitting it is a validation error.

<a id="req-0341"></a>

**REQ-0341.** `baseline_flag` does not take
`window.order_by`: it locates the baseline row by date, not by a
declared order. Declaring it is a validation error rather than silently
ignored.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [negative-row-no-prior](../../../benchmark/negative-row-no-prior/README.md).

The [execution manifest](../../../benchmark/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Partition completed output rows and compute ranks, neighbors, and baseline selections. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
