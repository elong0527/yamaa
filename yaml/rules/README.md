# Derivation rules

Each file defines one behavior of the derivation language. Rule IDs are stable
and do not change when files are renamed.

`normative` rules are authoritative for implementations. `draft` rules record
design intent but are not complete enough for portable implementation.

Two open questions account for every draft in the set, and for every draft
dependency a normative rule still carries:

- **R002** does not fix the ODM context keys, or the behavior when a contextual
  reference matches zero or several items.
- **R004** does not close the predicate grammar, coercion, collation, or the
  literal grammar. R009 is draft only because of this.

R005, R006, and R011 are normative and depend on no draft. R001, R003, and R008
reach draft text only through R002; R007 and R010 only through R004.

| ID | Rule | Status |
|---|---|---|
| R001 | [Execution model](R001-execution-model.md) | normative |
| R002 | [Source binding](R002-source-binding.md) | draft |
| R003 | [Cross-dataset left join](R003-cross-dataset-left-join.md) | normative |
| R004 | [Predicate language](R004-expression-language.md) | draft |
| R005 | [Output contract](R005-output-contract.md) | normative |
| R006 | [Compact schema language](R006-schema-language.md) | normative |
| R007 | [Expression registry](R007-expression-registry.md) | normative |
| R008 | [Local error handlers](R008-local-handlers.md) | normative |
| R009 | [Verifications](R009-verifications.md) | draft |
| R010 | [Scalar numeric computation](R010-scalar-computation.md) | normative |
| R011 | [Column type vocabulary and conversion](R011-column-types.md) | normative |

## Rule requirements

Every rule file must contain stable metadata, intent, requirements, and errors.
Dependencies must be declared explicitly. A rule must not silently override
another rule; replacements declare `supersedes`.
