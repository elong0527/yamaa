---
id: execution/ordering
title: Ordering
status: normative
---

# Ordering

## Requirements

### Artifact row order

<a id="req-0222"></a>

**REQ-0222.** `output.order_by` declares the order the artifact's rows are
presented in. It is optional. An artifact whose specification omits the
order keeps [Execution lifecycle](lifecycle.md)'s construction order: row-template order, and base-record order
or first-occurrence group order within each row template.

<a id="req-0223"></a>

**REQ-0223.** A term may name any declared column, output or internal.
A working value the artifact does not publish is admissible: a numeric
ordinal beside the text it labels, or a rank.

<a id="req-0224"></a>

**REQ-0224.** Every term must name a declared column. No variable may be
repeated. Validation reports an unknown or repeated term. A qualified source
variable is not a declared column and has no
value on a completed row to order by. A repeated term states nothing the
first term did not.

<a id="req-0226"></a>

**REQ-0226.** Ordering is a presentation step. Ordering runs once after the
lifecycle, key validation, and every [Verification](verification.md)
verification. Ordering never changes whether a run passes or warns. Evaluation
is unchanged. [Lifecycle](lifecycle.md)'s dependency order, a window's
partitions, and the neighbours `row_value` reads are fixed before
ordering. Each uses construction order for its own tie-break.
