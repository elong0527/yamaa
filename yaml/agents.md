# YAML configuration layer

This folder defines a language-agnostic YAML derivation specification for ODM,
SDTM, and ADaM datasets.

## Required reading

Before reviewing, implementing, or modifying this design:

1. Read `README.md` for scope and navigation.
2. Read `rules/reference/schema-language.md` for the notation used by the schema.
3. Read `schema.yaml` and follow every transitive `includes` entry needed for
   the derivation or verification vocabulary in scope.
4. Read `rules/README.md` and every rule applicable to the fields in scope.
5. Read the relevant example specification, README, input data, and expected
   output under `../benchmarks/`.

Schemas own shape, defaults, and structural constraints. Indexed contracts
own shared and operation-local semantics. Schema descriptions link to their
owning requirement and do not define additional behavior. Example READMEs do
not override either. Proposals remain outside the normative index until their
contracts and example coverage are complete.

## Terminology

One term per concept across all rules, schema comments, and messages:

- **specification** - the YAML file under review. `study` and `document` stay
  project-level (resources and submission contracts), never names for the YAML.
- **input dataset** / **output dataset** - state the role whenever it matters.
  Bare `dataset` appears only as a key name. `table` means a documentation
  table, never data.
- **row template** - one `rows` entry, building a group of output rows. Bare
  `template` means a string template (text operations).
- **record** - input side (a file or input dataset row). **row** - output side
  (an output record under construction or built).
- **column** - dataset level. **field** - file and schema level (ingestion and format profiles).
  **variable** - an expression-level name (schema language).
- **path notation** - `row.dataset`, `root.input`, `expression.source`
  (binding notation). Never `row_class.` or `root_class.`.
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
  `metadata` map. The [submission contract](../rules/submission/metadata.md)
  reserves its governed key names, so the map cannot
  become a second place a governed fact lives.
- Declare nothing a rule derives. `Mandatory` comes from `core`, a submission
  length from `max_length`, an ADaM origin source from its type, and a
  collected value's annotated-CRF reference from the document that declares
  it.
- Declare a codelist once in the study document and bind columns to its
  identifier, as required by the
  [terminology contract](../rules/submission/terminology.md).
- Change a closed grammar in its `grammar/` file first. The rule's grammar
  block is rendered from that file, each parser's closed vocabulary is
  compared with it, and both implementations replay its vectors, so a change
  made anywhere else fails validation. Add a vector for every behavior the
  change decides.
- Give every requirement a permanent global `REQ-NNNN` ID and list its
  owning contract in `rules/README.md`. Preserve IDs across moves; update
  `rules/migration.yaml` and regenerate the reference tables. Keep legacy
  aliases in that map rather than in duplicate normative paragraphs.
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
  fixtures follow the [CSV profile](../rules/storage/csv.md): UTF-8 without a
  byte-order mark, comma-separated
  fields, a named non-empty field per column, and the same field count in
  every record. Parquet fixtures follow the closed
  [Parquet profile](../rules/storage/parquet.md).
  A file that departs from its profile belongs only in a negative example that
  declares the condition it provokes.
- Do not duplicate normative behavior across schema definitions, rules, or
  examples. Keep all behavior in its owning contract and link to it from schema
  descriptions. Generate field tables with `generate_rule_reference.py`.
- Do not infer unspecified behavior. Record it as an unresolved design question
  or propose a new rule.
- Update or add examples whenever a normative rule changes behavior.
- Preserve deterministic behavior and require equivalent results from R and
  Python implementations.
