# YAML derivation specification

This folder defines the language-agnostic specification for ODM-to-SDTM and
SDTM-to-ADaM derivations. The design is under active development.

## Start here

The [contract index](../rules/README.md#rule-files) organizes the language by semantic owner:

| Block | Subject |
| --- | --- |
| Specification | Structure, composition, and binding |
| Values | Types, numbers, text, and temporal values |
| Execution | Lifecycle, rows, ordering, handlers, and verification |
| Operations | Expressions, languages, lookup, windows, and functions |
| Storage | Resources, ingestion, CSV, Parquet, and publication |
| Submission | Metadata, terminology, and Define-XML |
| Reference | Schema notation for maintainers |

## Sources of authority

Schemas own structure, defaults, and structural constraints. Indexed contracts
own shared and operation-local behavior. Schema descriptions link to the owning
rule. Examples demonstrate those contracts without redefining them.

Closed grammars are defined once in [grammar/](grammar/README.md). Their
contract blocks are generated views checked against those files; R and Python
replay the same parser vectors.

## Contents

| Location | Purpose |
| --- | --- |
| [schema.yaml](schema.yaml) | Specification entry point |
| [schema_project.yaml](schema_project.yaml) | Project file entry point: environments, terminology and submission sections |
| [schema_environment.yaml](schema_environment.yaml) | Environment file entry point: one host's lock and project functions |
| [v1/](v1/) | The 1.0 schema: one entry per file type, its modules and generated JSON Schemas (see [The 1.0 schema](#the-10-schema)) |
| [rules/](../rules/README.md) | Normative contracts |
| [Schema fields](https://elong0527.github.io/yamaa/reference/schema-fields/) | Generated shapes, defaults, and semantic links |
| [Schema fields (1.0)](https://elong0527.github.io/yamaa/reference/schema-fields-v1/) | The same view of `v1/`, with each field's kind |
| [Rule index](https://elong0527.github.io/yamaa/reference/rule-index/) | Generated canonical IDs and owners |
| [benchmarks/](../benchmarks/README.md) | Specifications, inputs, and expected outcomes |
| [conformance/](conformance/) | Shared language fixtures: the portable regex contract, and the evaluation set of the expression language |
| [grammar/](grammar/README.md) | Closed grammars and parser vectors |
| [vocabulary/cdisc.yaml](vocabulary/cdisc.yaml) | Closed CDISC vocabularies: models, dataset classes and subclasses, standard names and statuses, publishing sets, origin types and sources, column roles, core designations, submission data types, where comparators |

## The 1.0 schema

Two schema sets exist until Phase 2 of
[#3030](https://github.com/elong0527/yamaa/issues/3030) removes the 0.5 set
and moves `v1/` up into this folder (P2-13):

- `schema*.yaml` in this folder is the 0.5 schema. The engine embeds and
  reads three of its modules, `schema_project.yaml`, `schema_environment.yaml`
  and `schema_shared.yaml`, for a 0.5 project file and the environment files
  it links only, until [#3685](https://github.com/elong0527/yamaa/issues/3685)
  removes that route; any other file that declares `schema_version` is
  refused ([SCHEMA-2](../rules/reference/schema-language.md#schema-2)).
- [v1/](v1/) is the 1.0 schema, written from the approved rename table,
  [tools/migrate/renames.yaml](../tools/migrate/renames.yaml). The engine
  embeds each file type's closure, and a file that declares `yamaa` selects
  it by its header
  ([SCHEMA-2](../rules/reference/schema-language.md#schema-2)). Public calls
  read 1.0 files, and every benchmark is written in 1.0.

`v1/` has one entry module per 1.0 file type, holding the root class a file
of that type is checked against:

| File type | Entry | Root class |
| --- | --- | --- |
| `spec` | [v1/schema_spec.yaml](v1/schema_spec.yaml) | `root_class` |
| `project` | [v1/schema_project.yaml](v1/schema_project.yaml) | `project_class` |
| `codelists` | [v1/schema_codelists.yaml](v1/schema_codelists.yaml) | `codelist_file_class` |
| `function` | [v1/schema_function.yaml](v1/schema_function.yaml) | `function_definition_class` |

The 1.0 files are written in the
[schema notation](../rules/reference/schema-language.md) with two additions.
`kind` carries each field's kind (#3030 D31):

- A named value type may declare `kind`: `expression`, `name`, `identifier`,
  `value`, `path` or `structure`. A class field and a registry type never
  declare it.
- A field's kind is the kind of its type. `str`, `int`, `float`, `bool` and
  `"null"` are values; a class and a registry type are structures;
  `list[T]` and `dict[K, T]` take the kind of `T`; a named value type takes
  the kind it declares, or else the kind of its type; and a union takes the
  kind its members share. A union whose members differ is written as a named
  type that declares its kind.

`since` names the language version that introduced a field (#3030 P-VER-1):

- A class field may declare `since`, written `"1.N"`. A named value type and
  a registry type never declare it. A field without `since` exists from 1.0,
  and no 1.0 field declares it.
- A file declaring an older version than a field's `since` does not declare
  the field: writing it there fails as `unknown_field`, with context naming
  the version that introduced it.

The engine's bundle admission accepts both keywords, each only where it
applies.

Each file type has a JSON Schema in [v1/json/](v1/json/), generated by
`tools/packaging/generate_json_schema.py` beside its sources so that a schema
change shows both diffs; `make quick` refuses a stale one. Every field carries
its kind as `x-yamaa-kind` and its notation type as `x-yamaa-type`, and a
field that declares `since` carries it as `x-yamaa-since`. A
specification file may be one layer of an inheritance chain, so the spec JSON
Schema checks what one layer may write
([LAYER-4](../rules/inheritance.md#layer-4)), and its `$defs/root_class`
checks a complete specification. The engine embeds them byte for byte, and
`schema(file_type)` gives each as canonical JSON text, one line with its
members in key order. After changing a file in `v1/`, regenerate them, then
the copy of `v1/` and its JSON Schemas that the engine embeds and the field
orders the lint reads from each file type's classes, which `make quick`
also compares:

```sh
uv run --project python --no-sync python -m tools.packaging.generate_json_schema --write
uv run --project python --no-sync python -m tools.packaging.generate_shipped_schema --write
uv run --project python --no-sync python -m tools.packaging.generate_style_orders --write
```

## Execution overview

The [lifecycle](../rules/columns.md) owns execution order. Read it
first when implementing a runner, then follow the contract for each stage.
[Define-XML generation](../rules/profiles/define-xml.md) is a separate workflow
that composes specifications and metadata without running their derivations.

## Review workflow

1. Read the [schema notation](../rules/reference/schema-language.md), the relevant
   schema entry point, and its transitive includes.
2. Read the owning contracts and their dependencies.
3. Inspect representative positive and negative specifications, input data,
   and expected outcomes.
4. Update fixtures when behavior changes and require equivalent R/Python
   outcomes within their implemented coverage.
5. Run repository, migration, grammar, documentation, and conformance checks.

Unspecified behavior remains an explicit design question. Implementations
must not infer a new contract from a host default.

## Maintenance rules

Terminology and requirement allocation live in the
[contract index](../rules/README.md#terminology). When changing a schema,
rule, grammar, or fixture:

- Keep `schema.yaml` and every `schema_*.yaml` module compact and strictly valid
  YAML.
- Spell a 1.0 field in `v1/` as the approved rename table does:
  `tools/tests/test_schema_v1.py` compares every entry's spelling, type,
  requirement, default and kind with `v1/`, and refuses a field the table does
  not list or a spelling it reserves.
- Register every expression in an `expressions` registry and every verification
  in its applicable verification registry before an example uses it.
- Register a condition in `conditions.yaml` with its phases and its message:
  one plain clause that starts in lower case or with a placeholder and names
  only context keys every cause of the condition records (`{spec_path}` is the
  first spec path). Then regenerate the table the engine renders messages from
  with `uv run --project python --no-sync python -m
  tools.packaging.generate_message_templates --write`. `make quick` refuses a
  stale table, and the Rust test that reaches every registry cause fails when
  a message names a key some cause does not record. Name the condition on the
  `Fails` line of the card whose rule raises it: `tools.rules.check` refuses a
  registered condition that no card's `Fails` line names. Give it a parity
  case too, a negative benchmark or a case in `rust/tests/host-fixtures/`, or
  list it in `rust/tests/host-fixtures/unreached-conditions.yaml` with the
  reason: `tools.validation.check_diagnostics` refuses a registered condition
  with neither.
- Keep handler fields local to the expression or result stage that can use them;
  do not recreate a generic exception list.
- Store each cohesive semantic area in one rule file under `rules/`.
- Declare governed submission metadata in `submission`, never in the free-form
  `metadata` map. The [submission contract](../rules/profiles/submission.md)
  reserves its governed key names, so the map cannot
  become a second place a governed fact lives.
- Declare nothing a rule derives. For `sdtm` and `send`, `Mandatory` comes
  from `core`; for `adam` the two are independently declared. A submission
  length comes from `max_length`, an ADaM origin source from its type, and,
  under the `Other` context, a collected value's page-less annotated-CRF
  reference from the document that declares it.
- Declare a codelist once in the project file and bind columns to its
  identifier, as required by the
  [terminology contract](../rules/profiles/terminology.md).
- Change a closed grammar in its `grammar/` file first. The rule's grammar
  block is rendered from that file, the shared parser's closed vocabulary is
  compared with it, and both hosts replay its vectors, so a change made
  anywhere else fails validation. Add a vector for every behavior the change
  decides, in each context that decides it.
- Change what an expression evaluates to, or the issue it reports, together
  with the cases of `conformance/expressions.yaml` that show it, authored from
  the rule cards. `make check-repository` runs every case through the engine,
  and refuses a function, reducer or operator of the grammar that no case
  evaluates.
- Change a closed CDISC vocabulary in `vocabulary/cdisc.yaml` first, giving
  each added value the language version that admits it, then the schema
  type or class field the vocabulary names, in `yaml/` and `v1/` or only in
  the schema sets its `schemas` lists, the engine's list, and each rule card
  table that lists it: those of
  [SUB-3](../rules/profiles/submission.md#sub-3),
  [SUB-9](../rules/profiles/submission.md#sub-9),
  [SUB-10](../rules/profiles/submission.md#sub-10),
  [SUB-15](../rules/profiles/submission.md#sub-15),
  [SUB-26](../rules/profiles/submission.md#sub-26) and
  [JSON-10](../rules/profiles/dataset-json.md#json-10).
  `python -m tools.validation.check_cdisc_vocabulary` keeps each list that
  names a Define-XML enumeration equal to that enumeration in the Define-XML
  schema bundled with the engine, and each schema type or field equal to its
  list. `python -m tools.validation.check_cdisc_tables` keeps those tables to
  the file: each value they list is one of its values, and each value of a
  vocabulary they divide among their rows sits in one row, unless the check
  names it as belonging in none, as it does `BIMO`. Rust tests compare the
  engine's own lists with the file for every vocabulary but the where
  comparators, for which the engine keeps no list. A vocabulary only grows:
  no value is removed in 1.x.
- Keep repository-authored source ASCII-only. Spell non-ASCII characters by
  code point in rules, documentation, schemas, specifications, and tests;
  literal Unicode belongs only in input and expected-output data fixtures.
- Keep ordinary examples portable: declare source paths relative to the YAML
  file that writes them and store their fixtures beside that file. Explicit
  `relative_to: entry` examples keep study inputs beside each study entry. Dedicated
  path-policy tests may exercise parent-directory segments and temporary
  absolute local paths as positive cases. URI schemes, symbolic links,
  non-regular resources and missing files remain negative cases. A
  project file, like the retired `yamaa-project.yaml`, does not change
  document resource locations.
- Write every fixture under the source profile its extension selects. CSV
  fixtures follow the [CSV profile](../rules/reference/csv.md): UTF-8, with a
  byte-order mark only where the input declares one, comma-separated
  fields, a named non-empty field per column, and the same field count in
  every record. Parquet fixtures follow the closed
  [Parquet profile](../rules/reference/parquet.md).
  A file that departs from its profile belongs only in a negative example that
  declares the condition it provokes.
- Do not duplicate normative behavior across schema definitions, rules, or
  examples. Keep all behavior in its owning contract and link to it from schema
  descriptions. Generate field tables with `generate_rule_reference.py`.

## Reusing a complete window

A specification can declare the same grouping, ordering, and selection once:

```yaml
windows:
  RESPONSE_ORDER:
    group_by: [STUDYID, USUBJID]
    order_by: [ADT, RSSEQ]
```

Use `window: RESPONSE_ORDER` in each expression that shares those settings.
An inline mapping remains valid. References use the whole definition and
accept no extra fields or overrides. Omitted settings keep their usual
meaning, and each use operates on its caller's rows. See the
[window contract](../rules/windows.md#window-1) and the
[confirmed-response benchmark](../benchmarks/adam-adrs-confirmed-response/spec.yaml).

## Carry-forward and donor-record selection

`locf` takes a completed `source` variable and a `window` with `order_by`.
It preserves a current value and fills gaps from earlier non-missing values
within the partition; it never constructs additional rows.

A named intermediate can compare a donor field with the
current driver's field in its `filter`, for example
`OBS.AVISITN < PLAN.AVISITN`. The driver is the row template's `dataset` (an
input dataset or an eligible named intermediate), or root `base` without row
templates; either defaults to the single declared input when omitted (see the
[contract index](../rules/README.md#terminology)). `order_by` and `keep` select one donor
from the surviving candidates. Use an explicit planning input for visits
that do not yet have observed records. See the [window contract](../rules/windows.md)
and [lookup contract](../rules/lookup.md).

## Explicit source record order

Declare `ordinal` on a CSV, Parquet, SAS, or Transport input to expose its original one-based
data record position as an `int` field:

```yaml
input:
  SOURCE:
    path: input/source.csv
    ordinal: SourceOrdinal
```

A sequence column can then state its ordering explicitly:

```yaml
derivation:
  row_number:
    window:
      group_by: [GROUP_ID]
      order_by: [SOURCE.SourceOrdinal]
```

Filters retain the original positions, while `row_number` assigns consecutive
numbers within each group. The generated name must be absent from the stored
fields. It is typed automatically and is emitted only through a selected
output column that derives it. Fixed-schema ODM inputs do not accept `ordinal`.
See the [source ingestion contract](../rules/inputs.md#input-3)
and [source ordinal benchmark](../benchmarks/schema-source-ordinal/spec.yaml).
