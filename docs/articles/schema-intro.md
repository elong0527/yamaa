# Schema introduction

The schema says what a specification may contain. Anything it does not
declare is rejected before execution. Rules own what each written item
means; the schema owns shape, defaults, and structural constraints.

A specification declares which schema version it targets, and the version
must match exactly:

```yaml
schema_version: "1.0"
domain: ADSL
keys: [STUDYID, USUBJID]
```

## Three entry points

| Entry point | What it validates |
|---|---|
| `schema.yaml` | One specification: one output dataset |
| `schema_environment.yaml` | One project environment: runtime, function contracts, bindings |
| `schema_define.yaml` | One study document: Define-XML composition inputs |

Each entry point pulls in shared modules with `includes`. For a
specification, `schema.yaml` includes the shared headers, the derivation
vocabulary, verifications, and metadata. Follow the `includes` list from
the entry point to find the module that owns a field.

The full file index is at [Schema reference](schema.md).

## How to read a schema file

Schema files use four words. You need them to read the files; you never
write them in a specification.

- **A class is a header definition.** It says which fields a mapping may
  contain and which are required. `column_class` declares `name` and
  `type` required and the rest optional; each entry under `columns:` is
  one record written under that header. A field the class does not
  declare fails validation instead of being silently ignored.
- **A type is a shape constraint.** Column values use a closed set:
  `str`, `int`, `float`, `date`, `datetime`. Schema files also use
  descriptor types such as `identifier`, `variable`, `path`, and
  `predicate` to constrain what each field may hold.
- **A derivation says how a value is produced. An expression is the one
  registered keyword that produces it.** A derivation is an expression
  plus what happens when it goes wrong (`missing`, `strict`,
  ...). An expression is a mapping with exactly
  one entry: the key is the registered verb, the value is its
  parameters.
- **A registry is the list of permitted verbs.** Adding a verb costs one
  registry entry plus one rule, so the vocabulary cannot grow without
  end.

## The derivation vocabulary

Derivations use only registered verbs. They live in one module per
family:

| Family | Module | Verbs |
|---|---|---|
| Core selection | `schema_expression_core.yaml` | `source`, `literal`, `first_available`, `case` |
| Vocabulary | `schema_expression_mapping.yaml` | `mapping`, `lookup`, `cut` |
| Strings | `schema_expression_str.yaml` | `str_extract`, `str_concat`, `str_template`, `str_upper`, `str_lower` |
| Arithmetic | `schema_expression_numeric.yaml` | `compute`, `round_half_away_from_zero` |
| Aggregation | `schema_expression_aggregate.yaml` | `aggregate` |
| Dates | `schema_expression_date.yaml` | `date_diff`, `study_day`, `date_impute`, `date_precision` |
| Windows | `schema_expression_window.yaml` | `row_number`, `rank`, `row_value`, `previous_non_missing`, `baseline_flag`, `baseline_value` |
| Extension | `schema_function.yaml` | `function` |

Two composition rules keep dependencies visible: operands are named
variables, not nested expressions (bind a value to a column, then
reference the name), and nesting is permitted in exactly two places
(`case` branches and `str_concat.sources`).

## What the schema does not do

The schema never defines behavior. It says that `cut` takes `breaks`,
`labels`, and `missing`; the
[derivation contracts](rules.md) say what `cut` computes and when it
fails. Schema descriptions link to their owning requirement and add no
new meaning.

That split is the whole reading order: the schema tells you what you
can write, the rules tell you what it means, and `benchmark/` shows a
runnable specification that proves both.
