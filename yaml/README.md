# YAML derivation specification

This folder defines the language-agnostic specification for ODM-to-SDTM and
SDTM-to-ADaM derivations. The design is under active development.

## Start here

The [contract index](../rules/README.md) organizes the language by semantic owner:

| Block | Subject |
| --- | --- |
| [Specification](../rules/README.md#specification) | Structure, composition, and binding |
| [Values](../rules/README.md#values) | Types, numbers, text, and temporal values |
| [Execution](../rules/README.md#execution) | Lifecycle, rows, ordering, handlers, and verification |
| [Operations](../rules/README.md#operations) | Expressions, languages, lookup, windows, and functions |
| [Storage](../rules/README.md#storage) | Resources, ingestion, CSV, Parquet, and publication |
| [Submission](../rules/README.md#submission) | Metadata, terminology, and Define-XML |
| [Reference](../rules/README.md#reference) | Schema notation for maintainers |

## Sources of authority

Schemas own structure, defaults, and structural constraints. Indexed contracts
own shared and operation-local behavior. Schema descriptions link to the owning
requirement. Examples demonstrate those contracts without redefining them.

Closed grammars are defined once in [grammar/](grammar/README.md). Their
contract blocks are generated views checked against those files; R and Python
replay the same parser vectors.

## Contents

| Location | Purpose |
| --- | --- |
| [schema.yaml](schema.yaml) | Specification entry point |
| [schema_environment.yaml](schema_environment.yaml) | Project function environment entry point |
| [schema_define.yaml](schema_define.yaml) | Study-document entry point |
| [rules/](../rules/README.md) | Normative contracts |
| [Schema fields](../rules/reference/schema-fields.md) | Generated shapes, defaults, and semantic links |
| [Requirement index](../rules/reference/requirements.md) | Canonical IDs and historical aliases |
| [benchmarks/](../benchmarks/README.md) | Specifications, inputs, and expected outcomes |
| [conformance/](conformance/) | Shared language fixtures |
| [grammar/](grammar/README.md) | Closed grammars and parser vectors |
| [agents.md](agents.md) | Maintenance instructions |

## Execution overview

The [lifecycle](../rules/execution/lifecycle.md) owns execution order. Read it
first when implementing a runner, then follow the contract for each stage.
[Define-XML generation](../rules/submission/define-xml.md) is a separate workflow
that composes specifications and metadata without running their derivations.

## Review workflow

1. Read the [schema notation](../rules/reference/schema-language.md), the relevant
   schema entry point, and its transitive includes.
2. Read the owning contracts and their dependencies.
3. Inspect representative positive and negative specifications, input data,
   and expected outcomes.
4. Update fixtures when behavior changes and require equivalent R/Python
   outcomes within their implemented coverage.
5. Run repository, migration, grammar, documentation, and conformance checks.

Unspecified behavior remains an explicit design question. Implementations
must not infer a new contract from a host default.

## Reusing a complete window

A specification can declare the same grouping, ordering, and selection once:

```yaml
windows:
  RESPONSE_ORDER:
    group_by: [STUDYID, USUBJID]
    order_by: [ADT, RSSEQ]
```

Use `window: RESPONSE_ORDER` in each expression that shares those settings.
An inline mapping remains valid. References use the whole definition and
accept no extra fields or overrides. Omitted settings keep their usual
meaning, and each use operates on its caller's rows. See the
[window contract](../rules/operations/windows.md#req-1251) and the
[confirmed-response benchmark](../benchmarks/adam-adrs-confirmed-response/spec.yaml).

## Carry-forward and donor-record selection

`locf` takes a completed `source` variable and a `window` with `order_by`.
It preserves a current value and fills gaps from earlier non-missing values
within the partition; it never constructs additional rows.

A named intermediate or inline `lookup` can compare a donor field with the
current driver's field in its `filter`, for example
`OBS.AVISITN < PLAN.AVISITN`. The driver is root `base` without row templates,
or the explicit template's `dataset`. `order_by` and `keep` select one donor
from the surviving candidates. Use an explicit planning input for visits
that do not yet have observed records. See the [window contract](../rules/operations/windows.md)
and [lookup contract](../rules/operations/lookup.md).
