# YAML configuration layer

This folder defines a language-agnostic YAML derivation specification for ODM,
SDTM, and ADaM datasets.

## Required reading

Before reviewing, implementing, or modifying this design:

1. Read `README.md` for scope and navigation.
2. Read `rules/R006-schema-language.md` for the notation used by the schema.
3. Read `schema.yaml` and follow every transitive `includes` entry needed for
   the derivation or verification vocabulary in scope.
4. Read `rules/README.md` and every rule applicable to the fields in scope.
5. Read the relevant example specification, README, input data, and expected
   output under `examples/`.

Schema comments and descriptions are authoritative for operation-local
behavior; normative rule files are authoritative for shared behavior. Example
READMEs explain fixtures but do not override either. Draft rules record intent
and must not be treated as a portable implementation contract.

## Maintenance rules

- Keep `schema.yaml` and every `schema_*.yaml` module compact and strictly valid
  YAML.
- Register every expression in an `expressions` registry and every verification
  in its applicable verification registry before an example uses it.
- Keep handler fields local to the expression or result stage that can use them;
  do not recreate a generic exception list.
- Store each cohesive semantic area in one rule file under `rules/`.
- Give every rule a stable ID and list it in `rules/README.md`.
- Do not duplicate normative behavior across schema definitions, rules, or
  examples. Keep operation-local behavior beside its schema entry and shared
  behavior in the applicable rule.
- Do not infer unspecified behavior. Record it as an unresolved design question
  or propose a new rule.
- Update or add fixtures whenever a normative rule changes behavior.
- Preserve deterministic behavior and require equivalent results from R and
  Python implementations.
