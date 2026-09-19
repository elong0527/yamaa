---
title: Schema reference
---

# Schema reference

The schema bundle lives in [`yaml/`](https://github.com/elong0527/yamaa/tree/main/yaml).
A specification declares which schema version it targets (`schema_version`), and
structural validation rejects anything the schema does not declare. The four
words you need to read these files -- class, type, expression, registry -- are
explained in [Schema concepts](../articles/schema-concepts.md).

## Schema files

| File | What it declares |
|---|---|
| [`schema.yaml`](https://github.com/elong0527/yamaa/blob/main/yaml/schema.yaml) | The specification entry point: `root_class`, `column_class`, inputs, outputs, rows, verifications, metadata |
| [`schema_shared.yaml`](https://github.com/elong0527/yamaa/blob/main/yaml/schema_shared.yaml) | Headers shared across schema files |
| [`schema_derivation.yaml`](https://github.com/elong0527/yamaa/blob/main/yaml/schema_derivation.yaml) | The `derivation` field: the expression plus its `conversion_failure` layer |
| [`schema_expression_core.yaml`](https://github.com/elong0527/yamaa/blob/main/yaml/schema_expression_core.yaml) | Core expressions: `source`, `literal`, `first_available`, `case` |
| [`schema_expression_mapping.yaml`](https://github.com/elong0527/yamaa/blob/main/yaml/schema_expression_mapping.yaml) | Vocabulary mapping: `mapping`, `mapping_from`, `cut` |
| [`schema_expression_str.yaml`](https://github.com/elong0527/yamaa/blob/main/yaml/schema_expression_str.yaml) | String expressions: `str_extract`, `str_concat`, `str_template`, `str_upper`, `str_lower` |
| [`schema_expression_numeric.yaml`](https://github.com/elong0527/yamaa/blob/main/yaml/schema_expression_numeric.yaml) | The closed `compute` grammar (R010) |
| [`schema_expression_aggregate.yaml`](https://github.com/elong0527/yamaa/blob/main/yaml/schema_expression_aggregate.yaml) | The `aggregate` reducers (R013) |
| [`schema_expression_date.yaml`](https://github.com/elong0527/yamaa/blob/main/yaml/schema_expression_date.yaml) | Temporal expressions: `date_diff`, `study_day`, `date_impute`, `date_precision` (R016) |
| [`schema_expression_window.yaml`](https://github.com/elong0527/yamaa/blob/main/yaml/schema_expression_window.yaml) | Window expressions over constructed output rows: `row_number`, `rank`, `row_value`, `previous_non_missing`, `baseline_flag`, `baseline_value` |
| [`schema_function.yaml`](https://github.com/elong0527/yamaa/blob/main/yaml/schema_function.yaml) | The `function` extension point: logical name, contract version, arguments (R018) |
| [`schema_environment.yaml`](https://github.com/elong0527/yamaa/blob/main/yaml/schema_environment.yaml) | The project environment: runtime, function contracts, bindings, conformance vectors (R018) |
| [`schema_verification.yaml`](https://github.com/elong0527/yamaa/blob/main/yaml/schema_verification.yaml) | Dataset and column verifications (R009) |
| [`schema_metadata.yaml`](https://github.com/elong0527/yamaa/blob/main/yaml/schema_metadata.yaml) | Governed dataset and column metadata (R024) |
| [`schema_define.yaml`](https://github.com/elong0527/yamaa/blob/main/yaml/schema_define.yaml) | Define-XML composition inputs (R026) |

The derivation verbs in full, with what each one is for, are tabulated in
[Schema concepts](../articles/schema-concepts.md#2-the-verb-table). The
execution semantics behind them are owned by the
[derivation rules](rules.md).
