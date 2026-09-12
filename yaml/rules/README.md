# Derivation rules

Each file defines one behavior of the derivation language. Rule IDs are stable
and do not change when files are renamed.

Every indexed rule is normative and authoritative for implementations. A
proposed rule stays outside this index until its schema surface, portable
semantics, error behavior, and representative examples are complete.

| ID | Rule | Status | Owns |
| --- | --- | --- | --- |
| R001 | [Execution model](R001-execution-model.md) | normative | Phases, grouped row construction, dependency inference, evaluation order |
| R002 | [Source binding](R002-source-binding.md) | normative | Dataset declaration, name resolution |
| R003 | [Cross-dataset left join](R003-cross-dataset-left-join.md) | normative | The implicit join and its right-side reduction |
| R004 | [Predicate language](R004-predicate-language.md) | normative | The Boolean `sql` primitive |
| R005 | [Output contract](R005-output-contract.md) | normative | Column coverage, output membership, the value lifecycle, output identity, artifact row order |
| R006 | [Compact schema language](R006-schema-language.md) | normative | Schema notation and structural validation |
| R007 | [Expression registry](R007-expression-registry.md) | normative | Registration, nesting, evaluation kinds, ordering, input types |
| R008 | [Local error handlers](R008-local-handlers.md) | normative | The handler lifecycle |
| R009 | [Verifications](R009-verifications.md) | normative | What each assertion means, including group cardinality, and when it runs |
| R010 | [Scalar numeric computation](R010-scalar-computation.md) | normative | The `numeric_expression` primitive |
| R011 | [Column types](R011-column-types.md) | normative | The `column_type` vocabulary, non-finite normalization, and conversion |
| R012 | [String templates](R012-string-templates.md) | normative | Interpolation grammar, escaping, and evaluation |
| R013 | [Aggregate reduction](R013-aggregate-reduction.md) | normative | The `aggregate_expression` primitive: reducers, row-relative narrowing, grain, and empty-group results |
| R014 | [Source-format ingestion](R014-source-ingestion.md) | normative | Missing recognition and field typing at the source |
| R015 | [Record lookup](R015-record-lookup.md) | normative | Looking up one record of another dataset and reading it by name |
| R016 | [Temporal values](R016-temporal-values.md) | normative | The `date` and `datetime` values: lexical form, zone and precision model, comparison, canonical text, and the operations over them |
| R017 | [Specification inheritance](R017-specification-inheritance.md) | normative | Parent resolution, shallow composition, pruning, and resolved order |
| R018 | [Project function environment](R018-project-function-environment.md) | normative | Project resolution, logical function contracts, singular runtime binding, activation conformance |
| R019 | [Text values](R019-text-values.md) | normative | ASCII source, Unicode data, casing, equality, normalization, total order |
| R020 | [Artifact serialization](R020-artifact-serialization.md) | normative | The `parquet` and `csv` profiles, display precision, and publication |
| R021 | [Project resource resolution](R021-project-resource-resolution.md) | normative | The approved project root, written path form, readable file kinds, content identity |
| R022 | [Regular expressions](R022-regular-expressions.md) | normative | The pinned engine and flag set, full-match and search behavior per consumer, capture-group numbering |
| R023 | [Delimited source profile](R023-delimited-source.md) | normative | The `csv` source syntax: encoding, records and fields, header shape, and delivered quoting |

## Rule admission

The maintained rule set has one status: normative. Design proposals may be
developed in issues or branches, but they become rules only when the repository
can validate their schema shape, their behavior is closed enough for
independent R and Python implementations, and examples exercise both success
and failure.

Normative does not mean immutable. A rule can change through the repository's
review and versioning process, but an implementation must not substitute an
open proposal for the indexed text.

## Rule requirements

Every rule file contains stable metadata, an intent, a `Boundaries` section
naming what it does not own, its requirements, a non-normative `Rationale`
section explaining why the requirements hold, and its errors.

A rule owns its topic completely. A cross-reference names the owning rule and
stops; it does not restate or re-argue that rule's content, because a
restatement is a second place to keep correct. A rule must not silently
override another rule.

Normative keywords follow RFC 2119: `must` states an absolute requirement,
`must not` states an absolute prohibition, `should` states a strong
recommendation with a reason required to depart from it, and `may` states a
truly optional behavior. Each normative requirement carries a stable number
such as `R013-7`: the owning rule ID plus a sequence that runs from 1 in
document order. A negative example pins the requirement it exercises in the
`requirement` field of its `expected/error.yaml`.

A rule that owns a closed grammar keeps it in `../grammar/`, one file per
language. Its `## Grammar` block is rendered from that file and its closed
vocabulary is compared with the constants each parser uses, so the block is
a view of the grammar rather than a second copy of it.
