# Derivation rules

The rules are grouped by subject in a suggested reading order. Each contract
has one owner; blocks are navigation, not additional execution phases. Rule
IDs and file paths stay stable when the reading order changes.

Every indexed rule is normative and authoritative for implementations. A
proposed rule stays outside this index until its schema surface, portable
semantics, error behavior, and representative examples are complete.

## 1. Specification structure

How is a specification written, validated, and composed?

| ID | Rule | Status | Owns |
| --- | --- | --- | --- |
| R006 | [Compact schema language](R006-schema-language.md) | normative | Schema notation and structural validation |
| R017 | [Specification inheritance](R017-specification-inheritance.md) | normative | Parent resolution, layer composition, pruning, and resolved order |

## 2. Inputs and binding

Where does input data come from, and how are names resolved?

| ID | Rule | Status | Owns |
| --- | --- | --- | --- |
| R021 | [Project resource resolution](R021-project-resource-resolution.md) | normative | The approved project root, written path form, readable file kinds, content identity |
| R023 | [Source profile selection and delimited source profile](R023-delimited-source.md) | normative | Source-profile selection and delimited input encoding, records, fields, headers, quoting syntax |
| R027 | [Parquet source profile](R027-parquet-source.md) | normative | The self-describing field mapping, order, values, and errors of a `parquet` source |
| R014 | [Source-format ingestion](R014-source-ingestion.md) | normative | Missing recognition and field typing at the source |
| R002 | [Source binding](R002-source-binding.md) | normative | Dataset declaration, name resolution |

## 3. Values and types

What do values mean, and how are they converted and compared?

| ID | Rule | Status | Owns |
| --- | --- | --- | --- |
| R011 | [Column types](R011-column-types.md) | normative | Column types, input comparability, non-finite normalization, conversion |
| R019 | [Text values](R019-text-values.md) | normative | ASCII source, Unicode data, casing, equality, normalization, total order |
| R016 | [Temporal values](R016-temporal-values.md) | normative | The `date` and `datetime` values: lexical form, zone and precision model, comparison, canonical text, and the operations over them |

## 4. Execution and handling

When do operations run, and how are exceptional conditions handled?

| ID | Rule | Status | Owns |
| --- | --- | --- | --- |
| R001 | [Execution model](R001-execution-model.md) | normative | Phases, grouped row construction, dependency inference, evaluation order |
| R007 | [Expression registry](R007-expression-registry.md) | normative | Registration, nesting, scalar and window evaluation, shared ordering, expression types |
| R008 | [Local error handlers](R008-local-handlers.md) | normative | Handler conditions, substitutions, selection, and audit counts |

## 5. Matching and reduction

How are records matched, selected, and summarized?

| ID | Rule | Status | Owns |
| --- | --- | --- | --- |
| R003 | [Intermediate](R003-intermediate.md) | normative | Cross-dataset reads: the implicit join on applicable keys, named `intermediates`, inline `lookup`, declared-key aggregates |
| R013 | [Aggregate reduction](R013-aggregate-reduction.md) | normative | Aggregate contexts, filter scope, range narrowing, reducer grammar, keys, results |

## 6. Expression languages and extensions

What can a predicate, formula, template, pattern, or function express?

| ID | Rule | Status | Owns |
| --- | --- | --- | --- |
| R004 | [Predicate language](R004-predicate-language.md) | normative | The Boolean `predicate` primitive |
| R010 | [Scalar numeric computation](R010-scalar-computation.md) | normative | The `numeric_expression` primitive |
| R012 | [String templates](R012-string-templates.md) | normative | Interpolation grammar, escaping, and evaluation |
| R022 | [Regular expressions](R022-regular-expressions.md) | normative | The portable pattern contract and normalization, full-match and search behavior per consumer, capture-group numbering |
| R018 | [Project function environment](R018-project-function-environment.md) | normative | Project resolution, logical function contracts, singular runtime binding, activation conformance |

## 7. Validation and output

What constitutes a valid result, and how is it published?

| ID | Rule | Status | Owns |
| --- | --- | --- | --- |
| R005 | [Output contract](R005-output-contract.md) | normative | Column coverage, output membership, the value lifecycle, output identity, artifact row order |
| R009 | [Verifications](R009-verifications.md) | normative | What each assertion means, including group cardinality, and when it runs |
| R020 | [Artifact serialization](R020-artifact-serialization.md) | normative | The `parquet` and `csv` profiles, display precision, and publication |

## 8. Submission documentation

How are results described for submission?

| ID | Rule | Status | Owns |
| --- | --- | --- | --- |
| R024 | [Submission metadata](R024-submission-metadata.md) | normative | Governed dataset and column metadata: standard families, data type, length, core and mandatory, origin and what the graph refutes, methods, comments |
| R025 | [Controlled terminology](R025-controlled-terminology.md) | normative | The codelist object: identity, values, extensibility, external form, what a binding enforces, and agreement with `allowed_values` |
| R026 | [Define-XML 2.1 composition and serialization](R026-define-xml.md) | normative | The study document, composition, generated identifiers, element mapping and order, bytes, publication, and the deferred constructs |

## Rule admission

The maintained rule set has one status: normative. Design proposals may be
developed in issues or branches, but they become rules only when the repository
can validate their schema shape, their behavior is closed enough for
independent R and Python implementations, and examples exercise both success
and failure.

Normative does not mean immutable. A rule can change through the repository's
review and versioning process, but an implementation must not substitute an
open proposal for the indexed text.

Retired rules are deleted, not kept as files: the git history preserves the
superseded text. R015 (record lookup) and `mapping_from` were retired by
issue #568, which unified the three join mechanisms into this index's R003:
the implicit join stays for clear applicable keys, and one explicit lookup
covers every unclear or differing key.

## Rule requirements

Every rule file contains stable metadata, an intent, a `Boundaries` section
naming its owner and related contracts, requirements grouped by topic, an
`Errors` section, and a final non-normative `Rationale` section. New topics
belong before `Errors`; rationale explains requirements without adding any.

A rule owns its topic completely. A cross-reference names the owning rule and
stops; it does not restate or re-argue that rule's content, because a
restatement is a second place to keep correct. A rule must not silently
override another rule.

Normative keywords follow RFC 2119: `must` states an absolute requirement,
`must not` states an absolute prohibition, `should` states a strong
recommendation with a reason required to depart from it, and `may` states a
truly optional behavior. Each normative requirement carries a stable number
such as `R013-7`: the owning rule ID plus a permanent local identifier.
Identifiers are unique within a rule and do not change when sections move.
Existing letter suffixes such as `R001-12a` remain valid. New requirements
use the next unused integer; a retired identifier is never reused.

When ownership moves, retain the old numbered entry as a short reference to
the canonical requirement, and record the move in the
[ownership migration](../design-notes.md#ownership-migration). Do not copy the
contract into that reference. A negative example pins the requirement it
exercises in `expected/error.yaml`; existing citations remain valid.
Repository validation checks identifier uniqueness and citation resolution,
not numerical order.

A rule that owns a closed grammar keeps it in `../grammar/`, one file per
language. Its `## Grammar` block is rendered from that file and its closed
vocabulary is compared with the constants each parser uses, so the block is
a view of the grammar rather than a second copy of it.
