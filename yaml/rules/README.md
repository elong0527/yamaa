# Derivation rules

Each file defines one behavior of the derivation language. Rule IDs are stable
and do not change when files are renamed.

`normative` rules are authoritative for implementations. `draft` rules record
design intent but are not complete enough for portable implementation.

| ID | Rule | Status |
|---|---|---|
| R001 | [Execution model](R001-execution-model.md) | normative |
| R002 | [Source binding](R002-source-binding.md) | draft |
| R003 | [Cross-dataset left join](R003-cross-dataset-left-join.md) | normative |
| R004 | [Expression language](R004-expression-language.md) | draft |
| R005 | [Output contract](R005-output-contract.md) | draft |
| R006 | [Compact schema language](R006-schema-language.md) | normative |

## Rule requirements

Every rule file must contain stable metadata, intent, requirements, and errors.
Dependencies must be declared explicitly. A rule must not silently override
another rule; replacements declare `supersedes`.
