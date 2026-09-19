---
title: Rules
---

# Rules (R001-R027)

The normative derivation rules live in
[`yaml/rules/`](https://github.com/elong0527/yamaa/tree/main/yaml/rules) --
one file per rule, each owning its topic completely. Rule IDs are stable and
do not change when files are renamed. This page is an index only; for the
authoritative text, follow the links.

| ID | Rule | Owns |
|---|---|---|
| R001 | [Execution model](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R001-execution-model.md) | Phases, grouped row construction, dependency inference, evaluation order |
| R002 | [Source binding](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R002-source-binding.md) | Dataset declaration, name resolution |
| R003 | [Lookup](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R003-lookup.md) | Cross-dataset reads: the implicit join on applicable keys, named `lookups`, inline `lookup`, declared-key aggregates |
| R004 | [Predicate language](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R004-predicate-language.md) | The Boolean `sql` primitive |
| R005 | [Output contract](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R005-output-contract.md) | Column coverage, output membership, the value lifecycle, output identity, artifact row order |
| R006 | [Compact schema language](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R006-schema-language.md) | Schema notation and structural validation |
| R007 | [Expression registry](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R007-expression-registry.md) | Registration, nesting, evaluation kinds, ordering, input types |
| R008 | [Local error handlers](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R008-local-handlers.md) | The handler lifecycle |
| R009 | [Verifications](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R009-verifications.md) | What each assertion means, including group cardinality, and when it runs |
| R010 | [Scalar numeric computation](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R010-scalar-computation.md) | The `numeric_expression` primitive |
| R011 | [Column types](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R011-column-types.md) | The `column_type` vocabulary, non-finite normalization, and conversion |
| R012 | [String templates](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R012-string-templates.md) | Interpolation grammar, escaping, and evaluation |
| R013 | [Aggregate reduction](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R013-aggregate-reduction.md) | The `aggregate_expression` primitive: reducers, row-relative narrowing, grain, and empty-group results |
| R014 | [Source-format ingestion](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R014-source-ingestion.md) | Missing recognition and field typing at the source |
| R016 | [Temporal values](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R016-temporal-values.md) | The `date` and `datetime` values: lexical form, zone and precision model, comparison, canonical text, and the operations over them |
| R017 | [Specification inheritance](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R017-specification-inheritance.md) | Parent resolution, layer composition, pruning, and resolved order |
| R018 | [Project function environment](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R018-project-function-environment.md) | Project resolution, logical function contracts, singular runtime binding, activation conformance |
| R019 | [Text values](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R019-text-values.md) | ASCII source, Unicode data, casing, equality, normalization, total order |
| R020 | [Artifact serialization](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R020-artifact-serialization.md) | The `parquet` and `csv` profiles, display precision, and publication |
| R021 | [Project resource resolution](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R021-project-resource-resolution.md) | The approved project root, written path form, readable file kinds, content identity |
| R022 | [Regular expressions](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R022-regular-expressions.md) | The pinned engine and flag set, full-match and search behavior per consumer, capture-group numbering |
| R023 | [Source profile selection and delimited source profile](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R023-delimited-source.md) | Source-profile selection and the `csv` source syntax: encoding, records and fields, header shape, and delivered quoting |
| R024 | [Submission metadata](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R024-submission-metadata.md) | Governed dataset and column metadata: standard families, data type, length, core and mandatory, origin and what the graph refutes, methods, comments |
| R025 | [Controlled terminology](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R025-controlled-terminology.md) | The codelist object: identity, values, extensibility, external form, what a binding enforces, and agreement with `allowed_values` |
| R026 | [Define-XML 2.1 composition and serialization](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R026-define-xml.md) | The study document, composition, generated identifiers, element mapping and order, bytes, publication, and the deferred constructs |
| R027 | [Parquet source profile](https://github.com/elong0527/yamaa/blob/main/yaml/rules/R027-parquet-source.md) | The self-describing field mapping, order, values, and errors of a `parquet` source |

A rule owns its topic completely. A cross-reference names the owning rule and
stops; it does not restate that rule's content. Normative keywords follow
RFC 2119: `must` states an absolute requirement, `must not` an absolute
prohibition, `should` a strong recommendation with a reason required to depart
from it, and `may` a truly optional behavior.

For worked precedents of each rule in action, see the
[examples walkthrough](../articles/yaml-examples-walkthrough.md), which maps
constructs and questions to runnable examples, and the
[benchmark](../benchmark/index.md), which shows each one's input against its
output.
