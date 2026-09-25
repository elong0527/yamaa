---
id: execution/ordering
title: Ordering
status: normative
---

# Ordering

## Purpose

Apply ordering terms, missing placement, stable ties, and final artifact order.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Execution lifecycle](lifecycle.md).
- [Verification](verification.md).
- [Numeric computation](../operations/computation.md).
- [Expression evaluation](../operations/expressions.md).
- [Schema language](../reference/schema-language.md).
- [Source ingestion](../storage/ingestion.md).
- [Temporal values](../values/temporal.md).
- [Text values](../values/text.md).
- [Types and conversion](../values/types.md).


## Requirements

### The artifact

<a id="req-0194"></a>

**REQ-0194.** Rows leave in the order `output.order_by` declares. Without
`output.order_by`, rows keep [Execution lifecycle](lifecycle.md)'s construction order.

### Artifact row order

<a id="req-0222"></a>

**REQ-0222.** `output.order_by` declares the order the artifact's rows are
presented in. It is optional. An artifact whose specification omits the
order keeps [Execution lifecycle](lifecycle.md)'s construction order: row-template order, and input order
or first-occurrence group order within each row template.

<a id="req-0223"></a>

**REQ-0223.** A term may name any declared column, output or internal.
A working value the artifact does not publish is admissible: a numeric
ordinal beside the text it labels, or a rank.

<a id="req-0224"></a>

**REQ-0224.** Every term must name a declared column. No variable may be
repeated. A qualified source variable is not a declared column and has no
value on a completed row to order by. A repeated term states nothing the
first term did not.

<a id="req-0225"></a>

**REQ-0225.** Rows equal on every declared term keep their construction order.
Window ordering uses the same tie-break. Thus, the order is total for every
input. No tie is an error. No comparison is undefined. No specification
declares a term merely to make the result deterministic. A specification
wanting a tie broken declares the term that breaks it.

<a id="req-0226"></a>

**REQ-0226.** Ordering is a presentation step. Ordering runs once after the
lifecycle, key validation, and every [Verification](verification.md)
verification. Ordering never changes whether a run passes or warns. Evaluation
is unchanged. [Lifecycle](lifecycle.md)'s dependency order, a window's
partitions, and the neighbours `row_value` reads are fixed before
ordering. Each uses construction order for its own tie-break.

### Ordering

<a id="req-0297"></a>

**REQ-0297.** Every field typed `list[order_by_term]` is a list of order
terms, whichever operation declares it. An order term is either a bare
variable or a mapping declaring `variable`, `direction`, and `nulls`. The
bare form is an [Schema language](../reference/schema-language.md) shorthand union, so a bare variable means
`{variable: X, direction: asc, nulls: last}`.

<a id="req-0298"></a>

**REQ-0298.** `direction` is `asc` or `desc` and defaults to `asc`.

<a id="req-0299"></a>

**REQ-0299.** `nulls` is `last` or `first` and defaults to `last`. It states
where missing values sit among the non-missing ones for that term.

<a id="req-0300"></a>

**REQ-0300.** `nulls` does not flip with `direction`. `last` means last under
`asc` and last under `desc`. An implementation applies the declared placement
rather than the engine's default.

<a id="req-0301"></a>

**REQ-0301.** Terms apply in order. Each has its own direction and placement.
Ties, totality, and tie-breaking follow REQ-0225. A row's neighbours are
determined.

<a id="req-0302"></a>

**REQ-0302.** Non-missing values use the order their type owns: numeric order
under [Numeric values](../values/numbers.md), text order under [Text values](../values/text.md), and chronological order for `date` and
`datetime` under [Temporal values](../values/temporal.md).

### Type behavior

<a id="req-0312"></a>

**REQ-0312.** Window ordering requires mutually comparable values. One order
term names one variable, and a variable has exactly one type -- [Source ingestion](../storage/ingestion.md) gives
it to a source field and [Types and conversion](../values/types.md) to a declared column -- so the values a term
compares are of one type by construction and ordering has no incomparable
case. An expression naming several variables, as `greatest` and `least` do,
is where comparability is a requirement rather than a consequence.

### Interface behavior

<a id="req-1102"></a>

**REQ-1102.** The `order_by_term` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `order_by_term` | Bare variables use ascending order with missing values last. |

<a id="req-1103"></a>

**REQ-1103.** The `order_term_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `order_term_class.variable` | Variable used as an ordering term. |
| `order_term_class.direction` | Sort direction for non-missing values. |
| `order_term_class.nulls` | Placement of missing values, independent of direction. |

## Error conditions

<a id="req-0235"></a>

**REQ-0235.** An empty `keys`, an unknown key column, or a repeated key
column: fail.

<a id="req-0236"></a>

**REQ-0236.** An `output.order_by` term naming anything but a declared
column: fail and report the term.

<a id="req-0237"></a>

**REQ-0237.** An `output.order_by` variable declared more than once: fail and
report it.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [negative-order-repeated](../../benchmarks/negative-order-repeated/README.md).
- [negative-order-unknown-column](../../benchmarks/negative-order-unknown-column/README.md).

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Apply ordering terms, missing placement, stable ties, and artifact order.
One contract lets others refer to this topic without defining a second policy.
