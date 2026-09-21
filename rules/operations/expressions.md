---
id: operations/expressions
title: Expression evaluation
status: normative
---

# Expression evaluation

## Purpose

Register and dispatch expressions, restrict nesting, and define scalar selection.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Execution lifecycle](../execution/lifecycle.md).
- [Aggregation](aggregation.md).
- [Numeric computation](computation.md).
- [Project functions](functions.md).
- [Text operations](text.md).
- [Schema language](../reference/schema-language.md).
- [Source ingestion](../storage/ingestion.md).

## Requirements

### Expression evaluation

<a id="req-0045"></a>

**REQ-0045.** An expression contains exactly one keyword registered by [Expression evaluation](expressions.md).
Most keywords name their input variables directly. Resolve those variable
dependencies, then evaluate the keyword. Fields whose declared type contains
`expression` are evaluated recursively. A `source` or `literal` expression is
a leaf. YAML mapping order has no execution meaning.

<a id="req-0046"></a>

**REQ-0046.** Window expressions evaluate over the partitions declared by their
own `group_by`. Aggregate expressions evaluate in the contexts [REQ-0467](aggregation.md#req-0467) permits.
All other expressions return one value per current row.

### Registration

<a id="req-0288"></a>

**REQ-0288.** `schema_expression_*.yaml` and `schema_function.yaml` contribute
entries to the `expressions` registry under [Schema language](../reference/schema-language.md). `schema_derivation.yaml`
exposes that registry as the `expression` type.

<a id="req-0289"></a>

**REQ-0289.** Each registered keyword owns all its inputs, options, grouping,
local error handlers, and operation-local semantics. Adding a keyword
requires one complete registry entry. Unknown keywords and unknown payload
fields fail validation.

### Nesting policy

<a id="req-0290"></a>

**REQ-0290.** `source` and `literal` are expression leaves. Every other
expression names its input variables directly, except in the following fields
whose declared type contains `expression`. Each is evaluated recursively and
nests because its purpose is to select or compose expressions:

- `case` items: `case` selects among expressions, so each `then` and the
  trailing `otherwise` nests an expression.
- `str_concat.sources`: concatenation places literals beside sources.

<a id="req-0291"></a>

**REQ-0291.** `derivation` and `handled_expression_class.value` also contain
`expression`, but they hold a derivation's own top-level expression rather
than nest one inside an operation, so this policy does not restrict them.

<a id="req-0292"></a>

**REQ-0292.** Fields typed `numeric_expression`, `string_template`, and
`aggregate_expression` are leaves whose identifiers [Numeric computation](computation.md), [Text operations](text.md), and [Aggregation](aggregation.md)
resolve. Plain strings are values unless their schema field is typed as
`variable`, `function_arg`, `predicate`, or `string_template`. [Project functions](functions.md) closes
`function_arg`: a string is a variable, while string, date, and datetime
literals use their explicit tagged leaf forms.

### Type behavior

<a id="req-0310"></a>

**REQ-0310.** `greatest` and `least` require mutually comparable `sources`.

<a id="req-0315"></a>

**REQ-0315.** `source` retains its source type under [Source ingestion](../storage/ingestion.md). `literal` retains its YAML
scalar type after non-finite normalization. Mapping, conditional, coalescing,
and extreme expressions retain the selected value type, including collected
precision when the selected value is temporal. Numeric computation follows
[Numeric computation](computation.md); text operation results follow their operation contracts.

### Operation definitions

<a id="req-0318"></a>

**REQ-0318.** The schema registry owns payload shape, defaults, and structural constraints.
The owning operation contract defines evaluation, input and result semantics,
missing behavior, and failures. Schema descriptions link to that contract;
they do not define a competing operation contract or perform validation.

### Source shorthand

<a id="req-0319"></a>

**REQ-0319.** A `derivation`, `case_branch_class.then`, or
`case_otherwise_class.otherwise` written as a bare string is the source
shorthand: it desugars to `{source: <string>}` before registry dispatch. For a
`derivation`, the [REQ-0266](../reference/schema-language.md#req-0266)
handled-expression expansion then applies unchanged. The string is always a
source reference, never a literal: a bare `DM` with no variable `DM` fails
validation rather than producing the literal `"DM"`. A validated document
contains only the canonical dict form.

<a id="req-0320"></a>

**REQ-0320.** A `derivation` that is a non-string scalar is invalid; the
error names the dict form, so `derivation: 5` must be written
`{literal: 5}`. The specification reader uses YAML 1.2 core, so only
`true`/`false` spellings, numbers, and null parse as non-strings; quote a
column reference that YAML would otherwise parse as a non-string.

### Interface behavior

<a id="req-1093"></a>

**REQ-1093.** The `expressions.source` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.source` | Copies a named source or derived variable. |
| `Result` | Copies a named source or derived variable. |

<a id="req-1094"></a>

**REQ-1094.** The `expressions.literal` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.literal` | Returns the declared scalar value unchanged. |
| `Result` | Returns the declared scalar value unchanged. |

<a id="req-1095"></a>

**REQ-1095.** The `expressions.first_available` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.first_available.sources` | Variables to test in order. |
| `expressions.first_available.missing` | Value returned when every source is missing. |
| `Result` | Returns the value of the first source that is not missing. When every source is missing, returns the declared `missing`, or missing when none is declared. |

<a id="req-1096"></a>

**REQ-1096.** The `expressions.greatest` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.greatest.sources` | Mutually comparable variables to inspect. |
| `Result` | Returns the largest non-missing source, or missing when none exist. |

<a id="req-1097"></a>

**REQ-1097.** The `expressions.least` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.least.sources` | Mutually comparable variables to inspect. |
| `Result` | Returns the smallest non-missing source, or missing when none exist. |

<a id="req-1098"></a>

**REQ-1098.** The `expressions.case` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.case` | Ordered when/then items with an optional trailing otherwise; [Expression evaluation](expressions.md) defines the item order. |
| `Result` | Returns the first true branch's value, then otherwise, or missing. |

<a id="req-1099"></a>

**REQ-1099.** The `case_item_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `case_item_class` | A when/then branch, or the trailing otherwise item. |

<a id="req-1100"></a>

**REQ-1100.** The `case_branch_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `case_branch_class.when` | Condition that selects this branch when true. |
| `case_branch_class.then` | Result returned when the condition is true. |

<a id="req-1101"></a>

**REQ-1101.** The `case_otherwise_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `case_otherwise_class.otherwise` | Result used when no branch condition is true. |

## Error conditions

<a id="req-0321"></a>

**REQ-0321.** An unregistered expression keyword or invalid payload: fail
under [Schema language](../reference/schema-language.md).

<a id="req-0322"></a>

**REQ-0322.** A semantic constraint in an operation definition or applicable
rule that is not satisfied: fail.

<a id="req-0325"></a>

**REQ-0325.** A scalar or window expression that changes row count: fail
under [Execution lifecycle](../execution/lifecycle.md), which owns the phase invariant.

<a id="req-0339"></a>

**REQ-0339.** A `case` is a non-empty list of `case_item_class`. Every item
declares `when` and `then` except that one item may declare `otherwise`
instead; the `otherwise` item, when present, is the last item. A `case`
with no `when`/`then` item, more than one `otherwise` item, or an
`otherwise` item in any other position: fail.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [negative-tied-baseline](../../benchmarks/negative-tied-baseline/README.md).
- [negative-multiple-baselines](../../benchmarks/negative-multiple-baselines/README.md).
- [negative-previous-fixed](../../benchmarks/negative-previous-fixed/README.md).
- [negative-variable-nested](../../benchmarks/negative-variable-nested/README.md).

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Register and dispatch expressions, restrict nesting, and define scalar selection. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
