# YAML derivation specification

This folder defines a language-agnostic specification for ODM-to-SDTM and
SDTM-to-ADaM derivations. The design is under active development.

## Start here

The [rule index](rules/README.md) organizes the specification into eight
logical blocks. Read the block relevant to the work, then follow its owning
rules and schema entries.

| Block | Subject |
| --- | --- |
| [1. Specification structure](rules/README.md#1-specification-structure) | Schema notation and inheritance |
| [2. Inputs and binding](rules/README.md#2-inputs-and-binding) | Resources, CSV and Parquet, ingestion, name resolution |
| [3. Values and types](rules/README.md#3-values-and-types) | Conversion, comparison, text, temporal values |
| [4. Execution and handling](rules/README.md#4-execution-and-handling) | Dependencies, evaluation, ordering, local handlers |
| [5. Matching and reduction](rules/README.md#5-matching-and-reduction) | Joins, intermediates, lookups, aggregates |
| [6. Expression languages and extensions](rules/README.md#6-expression-languages-and-extensions) | Predicates, computation, templates, regex, functions |
| [7. Validation and output](rules/README.md#7-validation-and-output) | Result contracts, verifications, serialization |
| [8. Submission documentation](rules/README.md#8-submission-documentation) | Metadata, terminology, Define-XML |

## Sources of authority

The schema defines structure and operation-local behavior through adjacent
comments and parameter descriptions. Descriptions do not themselves perform
validation. Rule files define shared behavior. Examples demonstrate both
without redefining them. The [design notes](design-notes.md) explain design
choices and are non-normative.

A [replacement rule set](rules-next/README.md) is being drafted under
[issue #606](https://github.com/elong0527/yamaa/issues/606). Its contracts
and migration map are non-normative until the reviewed cutover.

Closed grammars are defined once in [grammar/](grammar/README.md). Their rule
blocks are generated views checked against the grammar files; both
implementations replay the same vectors.

## Contents

| Location | Purpose |
| --- | --- |
| [schema.yaml](schema.yaml) | Specification entry point and shared structure |
| [schema_environment.yaml](schema_environment.yaml) | Separate project function environment entry point |
| [schema_define.yaml](schema_define.yaml) | Separate study-document entry point |
| [schema_metadata.yaml](schema_metadata.yaml) | Governed submission metadata |
| [schema_derivation.yaml](schema_derivation.yaml) | Expression modules and derivation wrappers |
| [schema_verification.yaml](schema_verification.yaml) | Column and dataset verification registries |
| [schema_function.yaml](schema_function.yaml) | Project function calls |
| [rules/](rules/README.md) | Shared normative contracts, grouped by subject |
| [../benchmark/](../benchmark/README.md) | Specifications, inputs, exact expected outputs, and the validation manifest |
| [conformance/](conformance/) | Language-wide fixtures shared by implementations |
| [grammar/](grammar/README.md) | Closed grammars and parser vectors |
| [agents.md](agents.md) | Agent discovery and maintenance instructions |

## Execution overview

This is a reading guide; the linked rules define the actual contracts.

1. Resolve inherited specifications under
   [R017](rules/R017-specification-inheritance.md) and validate their
   structure under [R006](rules/R006-schema-language.md).
2. Resolve resources, decode source records, and bind names using the
   [input contracts](rules/README.md#2-inputs-and-binding).
3. Construct rows and derive columns in
   [R001](rules/R001-execution-model.md) dependency order. Each value follows
   the [R005 lifecycle](rules/R005-output-contract.md#derivation-lifecycle),
   with handlers under [R008](rules/R008-local-handlers.md).
4. Check the completed result, apply artifact ordering, and serialize it
   using the [output contracts](rules/README.md#7-validation-and-output).

Submission-document generation is a separate workflow under
[R026](rules/R026-define-xml.md); it composes resolved specifications and
metadata without running their derivations.

## Review workflow

1. Read [R006](rules/R006-schema-language.md) for schema notation, then the
   relevant entry point and its transitive schema includes.
2. Review the owning rules in the [rule index](rules/README.md).
3. Review a positive example, its input data, and its expected output.
4. Add or update examples when behavior changes, including negative examples
   for error conditions.
5. Require R and Python to produce equivalent outputs and errors.

Unspecified behavior is an unresolved design question or a proposed rule;
implementations must not infer it.
