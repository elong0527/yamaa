# Derivation contracts

Every contract indexed below is normative. Files have one semantic owner;
requirements use permanent global IDs independent of that owner's filename.
The schema owns shape, defaults, and structural constraints. Contracts own
shared and operation-local behavior. Examples demonstrate the contracts.

Read the relevant block, then follow its dependencies. The blocks are reading
order, not execution phases; [execution/lifecycle](execution/lifecycle.md)
owns the sequence of a run.

## Specification

| Contract | Status | Owns |
| --- | --- | --- |
| [Specification structure](specification/structure.md) | normative | Declare identifiers, columns, derivation coverage, and source notation. |
| [Specification composition](specification/composition.md) | normative | Resolve inherited layers into one minimal, ordered specification. |
| [Name binding](specification/binding.md) | normative | Resolve input datasets, current-output columns, and contextual ODM references. |

## Values

| Contract | Status | Owns |
| --- | --- | --- |
| [Types and conversion](values/types.md) | normative | Define column types, missing normalization, compatibility, and conversion. |
| [Numeric values](values/numbers.md) | normative | Define numeric representation, promotion, conversion, and overflow. |
| [Text values](values/text.md) | normative | Define Unicode scalar identity, preservation, equality, and order. |
| [Temporal values](values/temporal.md) | normative | Define dates, local civil datetimes, precision, canonical text, and order. |

## Execution

| Contract | Status | Owns |
| --- | --- | --- |
| [Execution lifecycle](execution/lifecycle.md) | normative | Sequence resolution, row construction, dependency evaluation, value completion, and output checks. |
| [Row construction](execution/rows.md) | normative | Construct output rows from declared input records or groups. |
| [Ordering](execution/ordering.md) | normative | Apply ordering terms, missing placement, stable ties, and final artifact order. |
| [Local handlers](execution/handlers.md) | normative | Handle conditions at their expression or conversion site and report substitutions. |
| [Verification](execution/verification.md) | normative | Apply assertions, severity, and grouped counts to completed values, and record what ran in the warning log and the verification log. |

## Operations

| Contract | Status | Owns |
| --- | --- | --- |
| [Expression evaluation](operations/expressions.md) | normative | Register and dispatch expressions, restrict nesting, and define scalar selection. |
| [Predicates](operations/predicates.md) | normative | Evaluate the closed Boolean language using three-valued logic. |
| [Numeric computation](operations/computation.md) | normative | Evaluate written arithmetic formulas without reassociation or presentation rounding. |
| [Aggregation](operations/aggregation.md) | normative | Reduce eligible records in one of the three permitted key scopes. |
| [Lookup and joins](operations/lookup.md) | normative | Match declared keys, narrow records, select a result, and answer absence. |
| [Windows](operations/windows.md) | normative | Declare complete named windows and compute partitions, ranks, neighbors, and baseline selections. |
| [Text operations](operations/text.md) | normative | Apply casing, inline mapping, templates, and portable regular expressions. |
| [Temporal operations](operations/temporal.md) | normative | Compute calendar differences, study days, date completion, and precision. |
| [Project functions](operations/functions.md) | normative | Resolve immutable runtimes and validate function inputs, results, and activation conformance. |

## Storage

| Contract | Status | Owns |
| --- | --- | --- |
| [Resource resolution](storage/resources.md) | normative | Confine declared source paths and read one immutable snapshot per physical file. |
| [Source ingestion](storage/ingestion.md) | normative | Select input profiles and assign source field types without inferring values. |
| [CSV profile](storage/csv.md) | normative | Read admitted CSV spellings and write canonical CSV bytes and display precision. |
| [Parquet profile](storage/parquet.md) | normative | Read and write the closed Parquet field and value mapping. |
| [Artifact publication](storage/publication.md) | normative | Select artifact columns and profiles and publish complete files atomically. |

## Submission

| Contract | Status | Owns |
| --- | --- | --- |
| [Submission metadata](submission/metadata.md) | normative | Govern dataset and column metadata, origin, methods, and document references. |
| [Controlled terminology](submission/terminology.md) | normative | Declare codelists once and validate their bindings and allowed values. |
| [Define-XML](submission/define-xml.md) | normative | Compose study metadata into deterministic Define-XML 2.1 documents. |
| [Dataset-JSON](submission/dataset-json.md) | normative | Write a dataset and its submission metadata as one deterministic Dataset-JSON 1.1 file. |

## Reference

| Contract | Status | Owns |
| --- | --- | --- |
| [Schema language](reference/schema-language.md) | normative | Define schema notation, registries, constraints, and canonical shorthand expansion. |

## Requirement identity

Requirements are defined once as `**REQ-0001.**`. Active IDs remain stable across
file moves and section reordering. Allocate IDs above the largest assigned number,
never renumber an existing requirement, and never fill a retired gap. Delete a
retired requirement block and redirect its historical aliases in
`migration.yaml` to the surviving requirement. Normative
`must`, `must not`, `should`, and `may` have their usual requirement meanings.
Every contract begins with Purpose and Scope and dependencies, then states
Requirements, Error conditions, Conformance examples, and Rationale. Topic
subsections live within these sections. Rationale adds no requirements.

[migration.yaml](migration.yaml) resolves every former `RNNN-n` citation to
one or more canonical requirements. It also records schema-prose provenance
and legacy source hashes for audit; it is not a second semantic contract.

The generated [requirement index](reference/requirements.md) links each active ID
and its historical aliases to the current owner.

Old diagnostic families (`R001`, etc.) in the validation-condition registry
remain compatibility names for their registered conditions. Their requirement
citations resolve through the same map. New author-facing citations use REQ IDs.

[Schema fields](reference/schema-fields.md) is generated from the current
schema: it lists shapes and defaults and links to semantic owners. Edit the
schema or owning contract and regenerate the table; do not edit the table.
The [glossary](reference/glossary.md) pins one meaning per shared clinical
term used across the contracts. It is informative, not normative: it states
what the terms mean and adds no requirements.
Closed syntax remains defined in [grammar/](yaml/grammar/README.md), with
rendered blocks in the owning contracts and shared R/Python parser vectors.

## Admission and maintenance

Proposals stay outside this index until schema shape, portable behavior,
errors, and representative examples are complete. Unspecified behavior must
be recorded explicitly rather than inferred. An editorial move must preserve
behavior and failure conditions; a semantic change needs its own conformance
evidence.
