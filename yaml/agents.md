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
behavior; indexed rule files are normative for shared behavior. Example
READMEs explain examples but do not override either. Proposed rules remain
outside the rule index until their contracts and example coverage are complete.

## Maintenance rules

- Keep `schema.yaml` and every `schema_*.yaml` module compact and strictly valid
  YAML.
- Register every expression in an `expressions` registry and every verification
  in its applicable verification registry before an example uses it.
- Keep handler fields local to the expression or result stage that can use them;
  do not recreate a generic exception list.
- Store each cohesive semantic area in one rule file under `rules/`.
- Give every rule a stable ID and list it in `rules/README.md`.
- Keep repository-authored source ASCII-only. Spell non-ASCII characters by
  code point in rules, documentation, schemas, specifications, and tests;
  literal Unicode belongs only in input and expected-output data fixtures.
- Declare every source path as a relative file inside the example directory.
  R021 confines what a run may read, so a rooted path, a URL, a parent
  traversal, or a symbolic link belongs only in a negative example.
- Write every fixture under R023's source profile: UTF-8 without a byte-order
  mark, comma-separated fields, a named non-empty field per column, and the
  same field count in every record. A file that departs from it belongs only
  in a negative example that declares the condition it provokes.
- Do not duplicate normative behavior across schema definitions, rules, or
  examples. Keep operation-local behavior beside its schema entry and shared
  behavior in the applicable rule.
- Do not infer unspecified behavior. Record it as an unresolved design question
  or propose a new rule.
- Update or add examples whenever a normative rule changes behavior.
- Preserve deterministic behavior and require equivalent results from R and
  Python implementations.
