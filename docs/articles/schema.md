# Schema files and JSON Schemas

The 1.0 schema is a set of YAML files in
[`yaml/`](../../yaml/README.md), written in the
[schema notation](../../rules/reference/schema-language.md). From them the
repository generates one JSON Schema for each file type, committed in
`yaml/json/`, and the engine embeds the schema files and the JSON Schemas.
[Schema introduction](schema-intro.md) explains the file types, the field
kinds and how layers compose; [Schema fields](../reference/schema-fields.md)
lists every field with its type, kind, default and the rule that gives its
meaning.

## The schema files

Each file type has one entry module. It holds the class a file of that type
is checked against and includes the modules that class needs:

| File type | Entry module | Root class | JSON Schema |
|---|---|---|---|
| `spec` | [`schema_spec.yaml`](../../yaml/schema_spec.yaml) | `root_class` | [`spec.schema.json`](../../yaml/json/spec.schema.json) |
| `project` | [`schema_project.yaml`](../../yaml/schema_project.yaml) | `project_class` | [`project.schema.json`](../../yaml/json/project.schema.json) |
| `codelists` | [`schema_codelists.yaml`](../../yaml/schema_codelists.yaml) | `codelist_file_class` | [`codelists.schema.json`](../../yaml/json/codelists.schema.json) |
| `function` | [`schema_function.yaml`](../../yaml/schema_function.yaml) | `function_definition_class` | [`function.schema.json`](../../yaml/json/function.schema.json) |

The modules they include, directly or through another module:

| Module | What it declares | Included by |
|---|---|---|
| [`schema_shared.yaml`](../../yaml/schema_shared.yaml) | The language version, column types, constants, paths and identifiers | every entry module |
| [`schema_derivation.yaml`](../../yaml/schema_derivation.yaml) | The `expression` and `derivation` types, a derivation written in full with `unconvertible`, and a named intermediate's derivations | `schema_spec.yaml` |
| [`schema_verification.yaml`](../../yaml/schema_verification.yaml) | Column, dataset and named-intermediate verifications | `schema_spec.yaml` |
| [`schema_metadata.yaml`](../../yaml/schema_metadata.yaml) | Dataset and column submission metadata | `schema_spec.yaml` |
| [`schema_terminology.yaml`](../../yaml/schema_terminology.yaml) | Codelist sources, codelists and their items | `schema_project.yaml`, `schema_codelists.yaml` |

`schema_derivation.yaml` includes the expression modules, each registering
expressions in the `expressions` registry. The table below is generated from
them, so it lists every expression each one registers. The core module also
declares order terms and `keep`, and the window module the windows its
expressions read.

<!-- expression-modules:start -->

| Module | Expressions |
|---|---|
| [`schema_expression_core.yaml`](../../yaml/schema_expression_core.yaml) | `source`, `first_available`, `greatest`, `least`, `case` and `flag` |
| [`schema_expression_mapping.yaml`](../../yaml/schema_expression_mapping.yaml) | `mapping` and `cut` |
| [`schema_expression_str.yaml`](../../yaml/schema_expression_str.yaml) | `str_pad`, `str_substr`, `str_length`, `str_extract`, `str_replace`, `str_template` and `str_case` |
| [`schema_expression_numeric.yaml`](../../yaml/schema_expression_numeric.yaml) | `round_half_away_from_zero` and `str_decimal` |
| [`schema_expression_date.yaml`](../../yaml/schema_expression_date.yaml) | `diff`, `impute`, `precision`, `imputation_flag`, `to_date`, `date_text`, `study_day`, `to_epoch_day`, `dtc`, `time_of_day`, `shift`, `duration_value` and `duration_text` |
| [`schema_expression_window.yaml`](../../yaml/schema_expression_window.yaml) | `row_number`, `rank`, `row_value`, `locf` and `baseline_flag` |
| [`schema_expression_aggregate.yaml`](../../yaml/schema_expression_aggregate.yaml) | `aggregate` |
| [`schema_expression_odm.yaml`](../../yaml/schema_expression_odm.yaml) | `odm`, which reads an item of the collected ODM data |
| [`schema_expression_function.yaml`](../../yaml/schema_expression_function.yaml) | `function`, which calls a project function |

<!-- expression-modules:end -->

`yaml/` also holds [`conditions.yaml`](../../yaml/conditions.yaml), which
records every registered condition's phases, message template and former
name, and every condition a release retired: the catalogue that
`conditions()` returns. It also holds the closed grammars in
[`grammar/`](../../yaml/grammar/README.md) and the closed CDISC vocabularies
in [`vocabulary/cdisc.yaml`](../../yaml/vocabulary/cdisc.yaml).

## Reading a schema file

A schema file is a YAML mapping: its `version`, the modules it `includes`,
and the named types and registries it declares
([SCHEMA-1](../../rules/reference/schema-language.md#schema-1),
[SCHEMA-5](../../rules/reference/schema-language.md#schema-5)). You need
these words to read one; you never write them in a yamaa file.

- **A class** lists the fields a mapping may hold. Each field has a `type`,
  whether it is `required`, any `default`, and a `description` that usually
  cites the rule owning it. A field the class does not declare fails as
  `unknown_field`.
- **A named value type** constrains a scalar with listed `values`, a
  `pattern` or a `min_length`, and may declare the `kind` of the fields it
  types.
- **A registry** lists the keywords a field admits, each with the payload it
  takes, and a value names exactly one of them: `expressions` holds the verbs,
  and `column_verifications`, `dataset_verifications` and
  `intermediate_verifications` the verifications.
- **A union**, such as `[variable, source_binding_class]`, admits a value of
  either member. When one member is a class whose one required field has the
  other member's type, the bare value is a short form of the class, expanded
  with that field set before anything else reads it
  ([SCHEMA-10](../../rules/reference/schema-language.md#schema-10)).

`fields_from` copies another class's fields into a class. A class field may
declare `since`, the language version that introduced it; a field without one
exists from 1.0.

## A file type's JSON Schema

Each JSON Schema follows JSON Schema Draft 2020-12, so an editor or any
validator can check a file of its type. Every field carries its kind as
`x-yamaa-kind` and its notation type as `x-yamaa-type`, and a field added
after 1.0 carries the version that introduced it as `x-yamaa-since`. A
specification file may be one layer of a composition, so the `spec` schema
checks what one layer may write, and its `$defs/root_class` checks a
complete specification ([LAYER-4](../../rules/inheritance.md#layer-4)).

The installed engine gives each file type's JSON Schema without a network:
the committed file's JSON as canonical text on one line, the same in every
installation of one release
([JSON Schema of a file type](api.md#json-schema-of-a-file-type)).

=== "R"

    ```r
    writeLines(yamaa_schema("spec"), "spec.schema.json")
    ```

=== "Python"

    ```python
    from pathlib import Path

    import yamaa

    Path("spec.schema.json").write_text(yamaa.schema("spec") + "\n")
    ```

=== "Command line"

    ```sh
    yamaa schema spec > spec.schema.json
    ```

## Each release's address

Each release's JSON Schema for a file type is the committed file at the
release's tag, read from GitHub:

```text
https://raw.githubusercontent.com/elong0527/yamaa/yamaa-v<version>/yaml/json/<type>.schema.json
```

For `spec` in 0.6.0, that is
`https://raw.githubusercontent.com/elong0527/yamaa/yamaa-v0.6.0/yaml/json/spec.schema.json`.
From 1.0.0 the files keep their path and carry no `$id`, so only the tag
changes from one release's address to the next, and every later 1.x release
keeps each field they give
([Stability contract](../release.md#stability-contract)). Point an editor at
the release your project runs.

An editor that reads YAML through the YAML language server associates a
schema with files by pattern. In Visual Studio Code, with the YAML extension,
this setting in the workspace's `.vscode/settings.json` checks and completes
every specification under `sdtm/` and `adam/` as you type:

```json
{
  "yaml.schemas": {
    "https://raw.githubusercontent.com/elong0527/yamaa/yamaa-v0.6.0/yaml/json/spec.schema.json": [
      "sdtm/*.yaml",
      "adam/*.yaml"
    ]
  }
}
```

The project file, function files and codelists files take their own schemas
the same way. Offline, give the path of a schema saved with `yamaa schema` in
place of the address.
