# Derivation rules

Each file defines one behavior of the derivation language. Rule IDs are stable
and do not change when files are renamed.

`normative` rules are authoritative for implementations. `draft` rules record
design intent but are not complete enough for portable implementation.

| ID | Rule | Status | Owns | Depends on |
|---|---|---|---|---|
| R001 | [Execution model](R001-execution-model.md) | normative | Phases, dependency inference, evaluation order | R002, R003, R004, R005, R007, R008, R010, R012 |
| R002 | [Source binding](R002-source-binding.md) | normative | Dataset declaration, name resolution | R003, R006, R008 |
| R003 | [Cross-dataset left join](R003-cross-dataset-left-join.md) | normative | The implicit join and its right-side reduction | R002, R004, R005, R007, R008 |
| R004 | [Predicate language](R004-expression-language.md) | draft | The Boolean `sql` primitive | R001, R002, R006 |
| R005 | [Output contract](R005-output-contract.md) | normative | Column coverage, output membership, the value lifecycle, output identity | R001, R002, R003, R007, R008, R009, R011 |
| R006 | [Compact schema language](R006-schema-language.md) | normative | Schema notation and structural validation | — |
| R007 | [Expression registry](R007-expression-registry.md) | normative | Registration, nesting, evaluation kinds, ordering, input types | R001, R002, R003, R004, R005, R006, R008, R010, R011, R012 |
| R008 | [Local error handlers](R008-local-handlers.md) | normative | The handler lifecycle | R001, R002, R003, R005, R006, R007, R011, R012 |
| R009 | [Verifications](R009-verifications.md) | draft | What each assertion means and when it runs | R004, R005, R006 |
| R010 | [Scalar numeric computation](R010-scalar-computation.md) | normative | The `numeric_expression` primitive | R001, R004, R005, R006, R007, R011 |
| R011 | [Column types](R011-column-types.md) | normative | The `column_type` vocabulary and conversion | R005, R006, R007, R008, R010 |
| R012 | [String templates](R012-string-templates.md) | normative | Interpolation grammar, escaping, and evaluation | R001, R002, R006, R007, R008 |
| R013 | [Source-format ingestion](R013-source-ingestion.md) | normative | Missing recognition and field typing at the source | R002, R006, R011 |
| R014 | [Record selection](R014-record-selection.md) | normative | Selecting one record of another dataset and reading it by name | R001, R002, R003, R004, R006, R007, R008 |

## Draft surface

One open question accounts for every draft in the set:

- **R004** does not close the predicate grammar, coercion, collation, or the
  literal grammar. R009 is draft only because of this.

R006 is the only rule that depends on nothing. Every other rule reaches R002 or
R004 transitively, so closing R004 closes the remaining draft surface.

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
