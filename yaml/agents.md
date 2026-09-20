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
   output under `../benchmark/`.

Schema comments and descriptions are authoritative for operation-local
behavior; indexed rule files are normative for shared behavior. Example
READMEs explain examples but do not override either. Proposed rules remain
outside the rule index until their contracts and example coverage are complete.

## Terminology

One term per concept across all rules, schema comments, and messages:

- **specification** - the YAML file under review. `study` and `document` stay
  project-level (R021, R024, R026), never names for the YAML.
- **input dataset** / **output dataset** - state the role whenever it matters.
  Bare `dataset` appears only as a key name. `table` means a documentation
  table, never data.
- **row template** - one `rows` entry, building a group of output rows. Bare
  `template` means a string template (R012).
- **record** - input side (a file or input dataset row). **row** - output side
  (an output record under construction or built).
- **column** - dataset level. **field** - file and schema level (R014, R023).
  **variable** - an expression-level name (R006).
- **path notation** - `row.dataset`, `root.input`, `expression.source`
  (R002 style). Never `row_class.` or `root_class.`.
- **driver** - banned from normative text, schema descriptions, and
  user-facing messages. Use input dataset or `row.dataset`.

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
- Give every rule a stable ID and list it in the appropriate logical block
  of `rules/README.md`. Follow its section order and preserve requirement IDs
  when reorganizing text; an ownership move retains a numbered reference to
  the canonical requirement.
- Keep repository-authored source ASCII-only. Spell non-ASCII characters by
  code point in rules, documentation, schemas, specifications, and tests;
  literal Unicode belongs only in input and expected-output data fixtures.
- Declare every source path as a relative file inside the example directory.
  An example is validated with its own directory as the only approved root
  and carries no `yamaa-project.yaml`, so a rooted path names no approved
  location here, and it belongs with a URL, a parent traversal, and a symbolic
  link in a negative example. A rooted path is for a study that declares a
  data root, not for a fixture this repository carries.
- Write every fixture under the source profile its extension selects. CSV
  fixtures follow R023: UTF-8 without a byte-order mark, comma-separated
  fields, a named non-empty field per column, and the same field count in
  every record. Parquet fixtures follow R027's closed schema and value profile.
  A file that departs from its profile belongs only in a negative example that
  declares the condition it provokes.
- Do not duplicate normative behavior across schema definitions, rules, or
  examples. Keep operation-local behavior beside its schema entry and shared
  behavior in the applicable rule.
- Do not infer unspecified behavior. Record it as an unresolved design question
  or propose a new rule.
- Update or add examples whenever a normative rule changes behavior.
- Preserve deterministic behavior and require equivalent results from R and
  Python implementations.
