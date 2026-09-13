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
- Declare governed submission metadata in `submission`, never in the free-form
  `metadata` map. R024 reserves the key names it governs, so the map cannot
  become a second place a governed fact lives.
- Declare nothing a rule derives. `Mandatory` comes from `core`, a submission
  length from `max_length`, an ADaM origin source from its type, and a
  collected value's annotated-CRF reference from the document that declares
  it.
- Declare a codelist once in the study document and bind columns to its
  identifier. Terminology restated per column is the shape R025 exists to
  prevent.
- Change a closed grammar in its `grammar/` file first. The rule's grammar
  block is rendered from that file, each parser's closed vocabulary is
  compared with it, and both implementations replay its vectors, so a change
  made anywhere else fails validation. Add a vector for every behavior the
  change decides.
- Give every rule a stable ID and list it in `rules/README.md`.
- Keep repository-authored source ASCII-only. Spell non-ASCII characters by
  code point in rules, documentation, schemas, specifications, and tests;
  literal Unicode belongs only in input and expected-output data fixtures.
- Declare every source path as a relative file inside the example directory.
  An example is validated with its own directory as the only approved root
  and carries no `yamaa-project.yaml`, so a rooted path names no approved
  location here, and it belongs with a URL, a parent traversal, and a symbolic
  link in a negative example. A rooted path is for a study that declares a
  data root, not for a fixture this repository carries.
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
