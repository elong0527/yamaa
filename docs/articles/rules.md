# Derivation contracts

Every contract indexed below is normative. Files have one semantic owner;
requirements use permanent global IDs independent of that owner's filename.
The schema owns shape, defaults, and structural constraints. Contracts own
shared and operation-local behavior. Examples demonstrate the contracts.

Read the relevant block, then follow its dependencies. The blocks are reading
order, not execution phases; [execution/lifecycle](https://github.com/elong0527/yamaa/blob/main/rules/execution/lifecycle.md)
owns the sequence of a run.

## Specification

| Contract | Status | Owns |
| --- | --- | --- |
| [Specification structure](https://github.com/elong0527/yamaa/blob/main/rules/specification/structure.md) | normative | Declare identifiers, columns, derivation coverage, and source notation. |
| [Specification composition](https://github.com/elong0527/yamaa/blob/main/rules/specification/composition.md) | normative | Resolve inherited layers into one minimal, ordered specification. |
| [Name binding](https://github.com/elong0527/yamaa/blob/main/rules/specification/binding.md) | normative | Resolve input datasets, current-output columns, and contextual ODM references. |

## Values

| Contract | Status | Owns |
| --- | --- | --- |
| [Types and conversion](https://github.com/elong0527/yamaa/blob/main/rules/values/types.md) | normative | Define column types, missing normalization, compatibility, and conversion. |
| [Numeric values](https://github.com/elong0527/yamaa/blob/main/rules/values/numbers.md) | normative | Define numeric representation, promotion, conversion, and overflow. |
| [Text values](https://github.com/elong0527/yamaa/blob/main/rules/values/text.md) | normative | Define Unicode scalar identity, preservation, equality, and order. |
| [Temporal values](https://github.com/elong0527/yamaa/blob/main/rules/values/temporal.md) | normative | Define dates, local civil datetimes, precision, canonical text, and order. |

## Execution

| Contract | Status | Owns |
| --- | --- | --- |
| [Execution lifecycle](https://github.com/elong0527/yamaa/blob/main/rules/execution/lifecycle.md) | normative | Sequence resolution, row construction, dependency evaluation, value completion, and output checks. |
| [Row construction](https://github.com/elong0527/yamaa/blob/main/rules/execution/rows.md) | normative | Construct output rows from declared input records or groups. |
| [Ordering](https://github.com/elong0527/yamaa/blob/main/rules/execution/ordering.md) | normative | Apply ordering terms, missing placement, stable ties, and final artifact order. |
| [Local handlers](https://github.com/elong0527/yamaa/blob/main/rules/execution/handlers.md) | normative | Handle conditions at their expression or conversion site and report substitutions. |
| [Verification](https://github.com/elong0527/yamaa/blob/main/rules/execution/verification.md) | normative | Apply assertions, severity, grouped counts, and warning logs to completed values. |

## Operations

| Contract | Status | Owns |
| --- | --- | --- |
| [Expression evaluation](https://github.com/elong0527/yamaa/blob/main/rules/operations/expressions.md) | normative | Register and dispatch expressions, restrict nesting, and define scalar selection. |
| [Predicates](https://github.com/elong0527/yamaa/blob/main/rules/operations/predicates.md) | normative | Evaluate the closed Boolean language using three-valued logic. |
| [Numeric computation](https://github.com/elong0527/yamaa/blob/main/rules/operations/computation.md) | normative | Evaluate written arithmetic formulas without reassociation or presentation rounding. |
| [Aggregation](https://github.com/elong0527/yamaa/blob/main/rules/operations/aggregation.md) | normative | Reduce eligible records in one of the three permitted key scopes. |
| [Lookup and joins](https://github.com/elong0527/yamaa/blob/main/rules/operations/lookup.md) | normative | Match declared keys, narrow records, select a result, and answer absence. |
| [Windows](https://github.com/elong0527/yamaa/blob/main/rules/operations/windows.md) | normative | Partition completed output rows and compute ranks, neighbors, and baseline selections. |
| [Text operations](https://github.com/elong0527/yamaa/blob/main/rules/operations/text.md) | normative | Apply casing, inline mapping, templates, and portable regular expressions. |
| [Temporal operations](https://github.com/elong0527/yamaa/blob/main/rules/operations/temporal.md) | normative | Compute calendar differences, study days, date completion, and precision. |
| [Project functions](https://github.com/elong0527/yamaa/blob/main/rules/operations/functions.md) | normative | Resolve immutable runtimes and validate function inputs, results, and activation conformance. |

## Storage

| Contract | Status | Owns |
| --- | --- | --- |
| [Resource resolution](https://github.com/elong0527/yamaa/blob/main/rules/storage/resources.md) | normative | Confine declared source paths and read one immutable snapshot per physical file. |
| [Source ingestion](https://github.com/elong0527/yamaa/blob/main/rules/storage/ingestion.md) | normative | Select input profiles and assign source field types without inferring values. |
| [CSV profile](https://github.com/elong0527/yamaa/blob/main/rules/storage/csv.md) | normative | Read admitted CSV spellings and write canonical CSV bytes and display precision. |
| [Parquet profile](https://github.com/elong0527/yamaa/blob/main/rules/storage/parquet.md) | normative | Read and write the closed Parquet field and value mapping. |
| [Artifact publication](https://github.com/elong0527/yamaa/blob/main/rules/storage/publication.md) | normative | Select artifact columns and profiles and publish complete files atomically. |

## Submission

| Contract | Status | Owns |
| --- | --- | --- |
| [Submission metadata](https://github.com/elong0527/yamaa/blob/main/rules/submission/metadata.md) | normative | Govern dataset and column metadata, origin, methods, and document references. |
| [Controlled terminology](https://github.com/elong0527/yamaa/blob/main/rules/submission/terminology.md) | normative | Declare codelists once and validate their bindings and allowed values. |
| [Define-XML](https://github.com/elong0527/yamaa/blob/main/rules/submission/define-xml.md) | normative | Compose study metadata into deterministic Define-XML 2.1 documents. |
| [Dataset-JSON](https://github.com/elong0527/yamaa/blob/main/rules/submission/dataset-json.md) | normative | Write a dataset and its submission metadata as one deterministic Dataset-JSON 1.1 file. |

## Reference

| Contract | Status | Owns |
| --- | --- | --- |
| [Schema language](https://github.com/elong0527/yamaa/blob/main/rules/reference/schema-language.md) | normative | Define schema notation, registries, constraints, and canonical shorthand expansion. |


[Requirement and compatibility index](https://github.com/elong0527/yamaa/blob/main/rules/reference/requirements.md).
