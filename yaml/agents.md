# YAML configuration layer

This folder defines a language-agnostic YAML derivation specification for ODM,
SDTM, and ADaM datasets.

## Required reading

Before reviewing, implementing, or modifying this design:

1. Read `README.md` for scope and navigation.
2. Read `rules/R006-schema-language.md` for the notation used by the schema.
3. Read `schema.yaml` for the allowed structure.
4. Read `rules/README.md` and every rule applicable to the fields in scope.
5. Read the relevant example specification, README, input data, and expected
   output under `examples/`.

Normative rule files are authoritative. Example READMEs explain fixtures but do
not override rules. Draft rules record intent and must not be treated as a
portable implementation contract.

## Maintenance rules

- Keep `schema.yaml` compact and strictly valid YAML.
- Store each cohesive semantic area in one rule file under `rules/`.
- Give every rule a stable ID and list it in `rules/README.md`.
- Do not duplicate normative behavior across rule or example files; link to the
  authoritative rule instead.
- Do not infer unspecified behavior. Record it as an unresolved design question
  or propose a new rule.
- Update or add fixtures whenever a normative rule changes behavior.
- Preserve deterministic behavior and require equivalent results from R and
  Python implementations.
