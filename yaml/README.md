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
