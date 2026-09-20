---
id: execution/handlers
title: Local handlers
status: normative
---

# Local handlers

## Purpose

Handle conditions at their expression or conversion site and report substitutions.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Execution lifecycle](lifecycle.md).
- [Aggregation](../operations/aggregation.md).
- [Lookup and joins](../operations/lookup.md).
- [Schema language](../reference/schema-language.md).
- [Name binding](../specification/binding.md).
- [Execution lifecycle](lifecycle.md).
- [Temporal values](../values/temporal.md).
- [Types and conversion](../values/types.md).

## Requirements

### Handler sites

<a id="req-0342"></a>

**REQ-0342.** Each handler applies at the site below. These sites identify
conditions within expression evaluation or result handling; the ordered
derivation lifecycle is [REQ-0211](lifecycle.md#req-0211) through [REQ-0218](lifecycle.md#req-0218). Each handler uses a literal
unless its behavior says otherwise:

| Stage | Local declaration | Behavior |
|---|---|---|
| bind | `source.missing` | Absent source variable or ODM item |
| join | `source.multiple_matches` | Choose one `source.filter` result |
| mapping | `missing` | Missing mapping input |
| mapping | `unmapped` | Non-missing value with no mapping |
| cut | `missing` | Missing numeric input |
| extract | `missing` | Missing string input |
| extract | `no_match` | Non-missing string does not match |
| template | `missing` | Any placeholder value is missing |
| impute | `date_impute.missing`, `date_precision.missing` | See [Temporal values](../values/temporal.md) |
| impute | `date_impute.invalid`, `date_precision.invalid` | See [Temporal values](../values/temporal.md) |
| convert | `conversion_failure` | Failed output conversion |

<a id="req-0343"></a>

**REQ-0343.** Handlers are substituted only when their condition occurs; every
handler value is a literal.

<a id="req-0344"></a>

**REQ-0344.** Omitting an applicable handler field makes its condition
fatal.

### What `missing` means, by stage

<a id="req-0345"></a>

**REQ-0345.** `missing` names two related conditions, distinguished by
where it is declared. On a `source` binding, it applies when the
variable or ODM item **does not exist in context**. It does not apply
when the variable exists and holds a missing value.

<a id="req-0346"></a>

**REQ-0346.** On every other expression, `missing` applies when the named
**input value is missing**.

### Present but unusable

<a id="req-0347"></a>

**REQ-0347.** `unmapped`, `no_match`, and `invalid` fire only when every
input is present.

<a id="req-0348"></a>

**REQ-0348.** The owning rule states which values an operation cannot use.
[Temporal values](../values/temporal.md) states them for the two operations on the `impute` stage.

<a id="req-0349"></a>

**REQ-0349.** One stage name serves several operations when their
conditions coincide. `date_impute` and `date_precision` read the same
source and answer the same two conditions about it, so both use
`impute` in structured errors.

<a id="req-0350"></a>

**REQ-0350.** Where an operation takes several inputs, as `lookup`
does, `missing` fires when any input is missing. The
present-but-unusable handler fires only when all inputs are present.

### Source handlers

<a id="req-0351"></a>

**REQ-0351.** Under the `source` expression, the concise `source:
DATASET.VARIABLE` form has no handler. Use structured source binding
when handling is required:

```yaml
source:
  variable: RAW.AGE
  missing: null
```

<a id="req-0352"></a>

**REQ-0352.** Other expressions type their `source` as a variable or a
variable with a `filter`, and declare their own handler fields alongside
it. The binding handlers are not theirs to declare: a source they name
reaches its records through [Lookup and joins](../operations/lookup.md) and answers to its own handlers once
it holds a value.

<a id="req-0353"></a>

**REQ-0353.** `multiple_matches` relaxes right-side uniqueness wherever one
source reaches several records: [Lookup and joins](../operations/lookup.md)'s matched records, an ODM item's
contextual matches, and the records a key combination was derived from.
Disagreement among those records is otherwise fatal under [REQ-0075](lifecycle.md#req-0075).

<a id="req-0354"></a>

**REQ-0354.** The source's optional `filter` selects the eligible
right-side records first. Sort those survivors by the `order_by` terms and
retain `first` or `last`. Remaining ties are resolved by right-side record
order.

<a id="req-0355"></a>

**REQ-0355.** An empty filtered result is not a handled condition. The
result is an absent match under [Lookup and joins](../operations/lookup.md) and yields missing.

<a id="req-0356"></a>

**REQ-0356.** The handler count reports only records where more than
one match survived the filter.

<a id="req-0357"></a>

**REQ-0357.** An aggregate declares no handler at all. A variable it
names that does not exist is [Name binding](../specification/binding.md)'s unresolved reference, a right side
that reduces to no matching record is [Lookup and joins](../operations/lookup.md)'s absent match, and a group
whose records all hold missing values is neither condition. [Aggregation](../operations/aggregation.md) states
what each reducer returns for such a group.

### Result handlers

<a id="req-0358"></a>

**REQ-0358.** A derivation with conversion handling uses `value`
to hold its normal expression. A bare expression is the [Schema language](../reference/schema-language.md) shorthand
for that wrapper, so every derivation carries its expression in `value`
once expanded.

<a id="req-0359"></a>

**REQ-0359.** `conversion_failure` supplies a literal replacement only
when conversion to the declared column type fails. Convert the replacement
to that same column type. [Types and conversion](../values/types.md) defines which
conversions fail and states that a missing input is not converted at
all, so `conversion_failure` never fires for one.

### Dependencies and audit

<a id="req-0360"></a>

**REQ-0360.** Literal handlers add no dependencies.

<a id="req-0361"></a>

**REQ-0361.** Implementations must report each handler path's record count.
A handler firing zero times is reportable and is not an error.

## Error conditions

<a id="req-0334"></a>

**REQ-0334.** An unhandled local missing, mapping, or extraction condition:
fail under [Local handlers](handlers.md).

<a id="req-0362"></a>

**REQ-0362.** A handler field on an expression that does not register it:
schema failure.

<a id="req-0363"></a>

**REQ-0363.** A result wrapper with no `conversion_failure`: fail.

<a id="req-0364"></a>

**REQ-0364.** A handler literal incompatible with its result context:
fail with both the handler and original context.

<a id="req-0365"></a>

**REQ-0365.** `multiple_matches.keep` outside `first` or `last`: schema
failure.

<a id="req-0366"></a>

**REQ-0366.** A conversion replacement that cannot be converted: fail.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [negative-mapping-unmapped-value](../../../benchmark/negative-mapping-unmapped-value/README.md).

The [execution manifest](../../../benchmark/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Handle conditions at their expression or conversion site and report substitutions. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
