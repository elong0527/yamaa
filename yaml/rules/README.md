# Derivation rules

Each file defines one behavior of the derivation language. Rule IDs are stable
and do not change when files are renamed.

Every indexed rule is normative and authoritative for implementations. A
proposed rule stays outside this index until its schema surface, portable
semantics, error behavior, and representative examples are complete.

| ID | Rule | Status | Owns | Depends on |
|---|---|---|---|---|
| R001 | [Execution model](R001-execution-model.md) | normative | Phases, grouped row construction, dependency inference, evaluation order | R002, R003, R004, R005, R007, R008, R010, R012, R013, R015, R019 |
| R002 | [Source binding](R002-source-binding.md) | normative | Dataset declaration, name resolution | R003, R006, R008, R014, R015, R019 |
| R003 | [Cross-dataset left join](R003-cross-dataset-left-join.md) | normative | The implicit join and its right-side reduction | R002, R004, R005, R007, R008, R013, R019 |
| R004 | [Predicate language](R004-expression-language.md) | normative | The Boolean `sql` primitive | R001, R002, R006, R007, R010, R011, R016, R019 |
| R005 | [Output contract](R005-output-contract.md) | normative | Column coverage, output membership, the value lifecycle, output identity, artifact row order | R001, R002, R003, R007, R008, R009, R011, R019, R020 |
| R006 | [Compact schema language](R006-schema-language.md) | normative | Schema notation and structural validation | R019 |
| R007 | [Expression registry](R007-expression-registry.md) | normative | Registration, nesting, evaluation kinds, ordering, input types | R001, R002, R003, R004, R005, R006, R008, R010, R011, R012, R013, R014, R015, R016, R018, R019 |
| R008 | [Local error handlers](R008-local-handlers.md) | normative | The handler lifecycle | R001, R002, R003, R005, R006, R007, R011, R012, R016 |
| R009 | [Verifications](R009-verifications.md) | normative | What each assertion means, including group cardinality, and when it runs | R004, R005, R006, R011, R019 |
| R010 | [Scalar numeric computation](R010-scalar-computation.md) | normative | The `numeric_expression` primitive | R001, R004, R005, R006, R007, R011, R014, R015 |
| R011 | [Column types](R011-column-types.md) | normative | The `column_type` vocabulary, non-finite normalization, and conversion | R005, R006, R007, R008, R009, R010, R014, R016, R018, R019, R020 |
| R012 | [String templates](R012-string-templates.md) | normative | Interpolation grammar, escaping, and evaluation | R001, R002, R006, R007, R008, R019 |
| R013 | [Aggregate reduction](R013-aggregate-reduction.md) | normative | The `aggregate_expression` primitive: reducers, row-relative narrowing, grain, and empty-group results | R001, R002, R003, R004, R006, R007, R010, R011, R015, R019 |
| R014 | [Source-format ingestion](R014-source-ingestion.md) | normative | Missing recognition and field typing at the source | R002, R006, R011, R016, R019 |
| R015 | [Record lookup](R015-record-lookup.md) | normative | Looking up one record of another dataset and reading it by name | R001, R002, R003, R004, R005, R006, R007, R008, R010, R014, R019 |
| R016 | [Temporal values](R016-temporal-values.md) | normative | The `date` and `datetime` values: lexical form, zone and precision model, comparison, canonical text, and the operations over them | R005, R006, R007, R008, R010, R011, R014 |
| R017 | [Specification inheritance](R017-specification-inheritance.md) | normative | Parent resolution, shallow composition, pruning, and resolved order | R001, R002, R005, R006, R009, R015 |
| R018 | [Project function environment](R018-project-function-environment.md) | normative | Project resolution, logical function contracts, singular runtime binding, activation conformance | R001, R005, R006, R011, R016, R019, R020 |
| R019 | [Text values](R019-text-values.md) | normative | ASCII source, Unicode data, casing, equality, normalization, total order | -- |
| R020 | [Artifact serialization](R020-artifact-serialization.md) | normative | The `parquet-v1` and `csv-v1` profiles, display precision, and publication | R005, R011, R014, R016, R019 |

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
naming what it does not own, its requirements, and its errors. Dependencies are
declared in `depends_on` and name every other rule whose definitions are needed
to apply the rule. A boundary reference that only directs a topic to its owner
is not a dependency.

A rule owns its topic completely. A cross-reference names the owning rule and
stops; it does not restate or re-argue that rule's content, because a
restatement is a second place to keep correct. A rule must not silently
override another rule; replacements declare `supersedes`.
