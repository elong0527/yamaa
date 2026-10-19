# Field kinds of the 1.0 surface

This page is the table of every field's kind that #3030 section 2.4
asks the RFC to publish (D31). It is generated from
[`tools/migrate/renames.yaml`](../../../tools/migrate/renames.yaml), the
proposed rename table, and lists every field of today's schemas with
its 1.0 spelling, plus the fields 1.0 adds. Back to the
[design document](../interface-1.0.md#field-kinds-d31).

D31 fixes two kinds. In an **expression field** a bare name is a
reference, text is quoted (`'DM'`), and a YAML number, Boolean or null
is a literal (D18). In a **value field** bare text is text, and YAML
null is the missing value. The table refines them:

| Kind | D31 kind | What a bare scalar is |
| --- | --- | --- |
| expression | expression field | grammar text: a reference, an operator expression or quoted text; predicates, the reducer `expr`, string templates, key match values, function arguments and every derivation position |
| name | expression field, restricted | one variable name, bare (an output column) or qualified (NAME-3); never literal text |
| value | value field | a constant: text is text, YAML null is missing; handlers, flag values, `dict` values, `cut` labels, `allowed_values`, labels, enumerations, numbers |
| identifier | value field | a declared name or a reference to one: `domain`, `keys`, `base`, `dataset`, `output.columns` |
| path | value field | a local path, relative to the declaring file (FILE-2) |
| structure | none | a class, a list of classes or a registry list |

Three positions change kind with D18 and need saying: the handled
wrapper's `value` (written `{value: X, unconvertible: L}`), key match
values (LOOKUP-6 already calls them expressions) and function
arguments. Name fields stay names: operations compose through named
derived columns (NAME-5).

## Specification files (`type: spec`)

| Path | 1.0 spelling | Kind | Type today | Decision |
| --- | --- | --- | --- | --- |
| `schema_version` | `yamaa: "1.0"` and `type: spec` | value | `str` | decided (D4) |
| `parent` | unchanged | path | `path` | proposed |
| `domain` | unchanged | identifier | `identifier` | proposed, P-WORD-1 |
| `keys` | unchanged | identifier | `list[identifier]` | proposed |
| `input` | unchanged | structure | `dict[identifier, dataset_source]` | proposed |
| `base` | unchanged | identifier | `identifier` | decided (D23) |
| `filter` | unchanged | expression | `predicate` | decided (D23) |
| `output` | unchanged | structure | `output_class` | proposed |
| `windows` | unchanged | structure | `dict[identifier, window_spec]` | proposed |
| `intermediates` | unchanged | structure | `list[intermediate_class]` | proposed, P-WORD-1 |
| `columns` | unchanged | structure | `list[column_class]` | proposed |
| `rows` | unchanged | structure | `list[row_class]` | decided (D23) |
| `verifications` | unchanged | structure | `list[dataset_verification]` | decided (D9) |
| `submission` | unchanged | structure | `submission_dataset_class` | decided (D7) |
| `metadata` | unchanged | value | `dict[str, str]` | proposed |
| `output.path` | unchanged | path | `path` | decided (D5) |
| `output.relative_to` | unchanged | value | `path_base` | decided (D5) |
| `output.decimals` | unchanged | value | `int` | proposed |
| `output.columns` | unchanged | identifier | `list[identifier]` | decided (D6) |
| `output.report` | unchanged | path | `path` | decided (D5) |
| `output.order_by` | unchanged | name | `list[order_by_term]` | proposed |
| `submission.has_no_data` | unchanged | value | `bool` | proposed |
| `submission.label` | unchanged | value | `str` | proposed |
| `submission.class` | unchanged | value | `dataset_class_name` | proposed |
| `submission.subclass` | unchanged | value | `dataset_subclass_name` | proposed |
| `submission.structure` | unchanged | value | `str` | proposed |
| `submission.repeating` | unchanged | value | `bool` | proposed |
| `submission.reference_data` | unchanged | value | `bool` | proposed |
| `submission.domain` | unchanged | identifier | `identifier` | proposed |
| `submission.comment` | unchanged | value | `submission_comment` | proposed |
| `windows.*.group_by` | unchanged | name | `list[variable]` | proposed |
| `windows.*.order_by` | unchanged | name | `list[order_by_term]` | proposed |
| `windows.*.filter` | unchanged | expression | `predicate` | proposed |
| `intermediates[].id` | unchanged | identifier | `intermediate_id` | proposed |
| `intermediates[].dataset` | unchanged | identifier | `identifier` | proposed, P-WORD-1 |
| `intermediates[].key` | key (required) | expression | `match_key` | decided (D23) |
| `intermediates[].between` | unchanged | structure | `record_between_class` | proposed |
| `intermediates[].filter` | unchanged | expression | `predicate` | proposed |
| `intermediates[].order_by` | unchanged | name | `list[order_by_term]` | proposed |
| `intermediates[].keep` | unchanged | value | `keep_policy` | proposed |
| `intermediates[].columns` | unchanged | identifier | `list[identifier]` | proposed |
| `intermediates[].no_match` | absent | value | `substitute_value` | proposed, P-HND-1 |
| `intermediates[].require_match` | unchanged | value | `bool` | proposed, P-HND-2 |
| `intermediates[].derivations` | unchanged | expression | `dict[identifier, intermediate_derivation]` | proposed |
| `intermediates[].verifications` | unchanged | structure | `list[intermediate_verification]` | decided (D9) |
| `columns[].name` | unchanged | identifier | `identifier` | proposed |
| `columns[].type` | unchanged | value | `column_type` | proposed, P-WORD-1 |
| `columns[].label` | unchanged | value | `str` | proposed |
| `columns[].derivation` | unchanged | expression | `derivation` | proposed |
| `columns[].verifications` | unchanged | structure | `list[column_verification]` | decided (D9) |
| `columns[].submission` | unchanged | structure | `submission_column_class` | decided (D7) |
| `columns[].metadata` | unchanged | value | `dict[str, str]` | proposed |
| `rows[].id` | unchanged | identifier | `row_id` | proposed |
| `rows[].dataset` | unchanged | identifier | `identifier` | proposed, P-WORD-1 |
| `rows[].group_by` | unchanged | name | `list[variable]` | proposed |
| `rows[].filter` | unchanged | expression | `predicate` | proposed |
| `rows[].derivations` | derivations (optional) | expression | `dict[identifier, derivation]` | proposed, P-HND-4 |
| `verifications[].all_or_none.id` | unchanged | identifier | `verification_id` | proposed |
| `verifications[].all_or_none.columns` | unchanged | name | `list[variable]` | proposed |
| `verifications[].all_or_none.severity` | unchanged | value | `verification_severity` | proposed |
| `verifications[].assert.id` | unchanged | identifier | `verification_id` | proposed |
| `verifications[].assert.when` | unchanged | expression | `predicate` | proposed |
| `verifications[].assert.require` | unchanged | expression | `predicate` | proposed |
| `verifications[].assert.severity` | unchanged | value | `verification_severity` | proposed |
| `verifications[].row_count.id` | unchanged | identifier | `verification_id` | proposed |
| `verifications[].row_count.group_by` | unchanged | name | `list[variable]` | proposed |
| `verifications[].row_count.filter` | unchanged | expression | `predicate` | proposed |
| `verifications[].row_count.when` | unchanged | expression | `predicate` | proposed |
| `verifications[].row_count.min` | unchanged | value | `int` | proposed |
| `verifications[].row_count.max` | unchanged | value | `int` | proposed |
| `verifications[].row_count.min_fraction` | unchanged | value | `float` | proposed |
| `verifications[].row_count.max_fraction` | unchanged | value | `float` | proposed |
| `verifications[].row_count.severity` | unchanged | value | `verification_severity` | proposed |
| `intermediates[].between.value` | unchanged | name | `variable` | proposed |
| `intermediates[].between.lower` | unchanged | identifier | `identifier` | proposed |
| `intermediates[].between.upper` | unchanged | identifier | `identifier` | proposed |
| `columns[].submission.core` | unchanged | value | `core_designation` | proposed |
| `columns[].submission.mandatory` | unchanged | value | `bool` | proposed |
| `columns[].submission.role` | unchanged | value | `str` | proposed |
| `columns[].submission.data_type` | unchanged | value | `define_data_type` | proposed |
| `columns[].submission.length` | unchanged | value | `int` | proposed |
| `columns[].submission.significant_digits` | unchanged | value | `int` | proposed |
| `columns[].submission.display_format` | unchanged | value | `str` | proposed |
| `columns[].submission.codelist` | unchanged | identifier | `identifier` | proposed |
| `columns[].submission.inventory_vocabulary` | unchanged | value | `bool` | proposed |
| `columns[].submission.origin` | origins | structure | `submission_origin_class` | proposed, P-ORIGIN-1 |
| `columns[].submission.method` | unchanged | value | `submission_method` | proposed |
| `columns[].submission.comment` | unchanged | value | `submission_comment` | proposed |
| `columns[].submission.value_level` | unchanged | structure | `list[submission_value_class]` | proposed |
| `input.*.path` | unchanged | path | `project_path` | decided (D5) |
| `input.*.relative_to` | unchanged | value | `path_base` | decided (D5) |
| `input.*.types` | unchanged | value | `dict[identifier, column_type]` | proposed |
| `input.*.encoding` | unchanged | value | `str` | proposed, P-UNU-1 |
| `input.*.schema` | unchanged | path | `project_path` | proposed |
| `input.*.empty_string` | unchanged | value | `str` | proposed |
| `input.*.ordinal` | unchanged | identifier | `identifier` | proposed |
| `input.*.require_match` | unchanged | value | `bool` | proposed, P-HND-2 |
| `columns[].verifications[].not_missing.id` | unchanged | identifier | `verification_id` | proposed |
| `columns[].verifications[].not_missing.severity` | unchanged | value | `verification_severity` | proposed |
| `columns[].verifications[].allowed_values.id` | unchanged | identifier | `verification_id` | proposed |
| `columns[].verifications[].allowed_values.values` | unchanged | value | `list[literal_value]` | proposed |
| `columns[].verifications[].allowed_values.severity` | unchanged | value | `verification_severity` | proposed |
| `columns[].verifications[].range.id` | unchanged | identifier | `verification_id` | proposed |
| `columns[].verifications[].range.min` | unchanged | value | `float` | proposed |
| `columns[].verifications[].range.max` | unchanged | value | `float` | proposed |
| `columns[].verifications[].range.severity` | unchanged | value | `verification_severity` | proposed |
| `columns[].verifications[].max_length.id` | unchanged | identifier | `verification_id` | proposed |
| `columns[].verifications[].max_length.max` | unchanged | value | `int` | proposed |
| `columns[].verifications[].max_length.severity` | unchanged | value | `verification_severity` | proposed |
| `columns[].verifications[].matches.id` | unchanged | identifier | `verification_id` | proposed |
| `columns[].verifications[].matches.pattern` | unchanged | value | `regex` | proposed |
| `columns[].verifications[].matches.severity` | unchanged | value | `verification_severity` | proposed |
| `columns[].submission.origin.type` | unchanged | value | `origin_type` | proposed |
| `columns[].submission.origin.source` | unchanged | value | `origin_source` | proposed |
| `columns[].submission.origin.description` | unchanged | value | `str` | proposed |
| `columns[].submission.origin.documents` | unchanged | identifier | `list[document_reference]` | proposed |
| `verifications[].unique.id` | unchanged | identifier | `verification_id` | proposed |
| `verifications[].unique.columns` | unchanged | name | `list[variable]` | proposed |
| `verifications[].unique.severity` | unchanged | value | `verification_severity` | proposed |
| `submission.comment.text` | unchanged | value | `str` | proposed |
| `submission.comment.documents` | unchanged | identifier | `list[document_reference]` | proposed |
| `output.order_by[].variable` | unchanged | name | `variable` | proposed |
| `output.order_by[].direction` | unchanged | value | `str` | proposed |
| `output.order_by[].nulls` | unchanged | value | `str` | proposed |
| `columns[].derivation.first_available.sources` | unchanged | name | `list[filtered_source]` | proposed |
| `columns[].derivation.first_available.missing` | unchanged | value | `substitute_value` | proposed |
| `columns[].derivation.greatest.sources` | unchanged | name | `list[variable]` | proposed |
| `columns[].derivation.least.sources` | unchanged | name | `list[variable]` | proposed |
| `columns[].derivation.mapping.source` | unchanged | name | `filtered_source` | proposed |
| `columns[].derivation.mapping.dict` | unchanged | value | `[dict[str, substitute_value], project_path]` | proposed, P-DICT-1 |
| `columns[].derivation.mapping.case_sensitive` | unchanged | value | `bool` | proposed |
| `columns[].derivation.mapping.missing` | unchanged | value | `substitute_value` | proposed |
| `columns[].derivation.mapping.unmapped` | unchanged | value | `substitute_value` | proposed |
| `columns[].derivation.cut.source` | unchanged | name | `filtered_source` | proposed |
| `columns[].derivation.cut.breaks` | unchanged | value | `list[float]` | proposed |
| `columns[].derivation.cut.labels` | unchanged | value | `list[str]` | proposed |
| `columns[].derivation.cut.right` | closed | value | `bool` | proposed, P-ORD-2 |
| `columns[].derivation.cut.missing` | unchanged | value | `substitute_value` | proposed |
| `columns[].derivation.str_pad.source` | unchanged | name | `variable` | proposed |
| `columns[].derivation.str_pad.width` | unchanged | value | `int` | proposed |
| `columns[].derivation.str_extract.source` | unchanged | name | `variable` | proposed |
| `columns[].derivation.str_extract.pattern` | unchanged | value | `regex` | proposed |
| `columns[].derivation.str_extract.group` | unchanged | value | `int` | proposed |
| `columns[].derivation.str_extract.missing` | unchanged | value | `substitute_value` | proposed |
| `columns[].derivation.str_extract.no_match` | unchanged | value | `substitute_value` | proposed |
| `columns[].derivation.str_replace.source` | unchanged | name | `variable` | proposed |
| `columns[].derivation.str_replace.pattern` | unchanged | value | `regex` | proposed |
| `columns[].derivation.str_replace.replacement` | unchanged | value | `str` | proposed |
| `columns[].derivation.str_replace.missing` | unchanged | value | `substitute_value` | proposed |
| `columns[].derivation.str_case.source` | unchanged | name | `variable` | proposed |
| `columns[].derivation.str_case.to` | unchanged | value | `str` | proposed |
| `columns[].derivation.str_case.missing` | unchanged | value | `substitute_value` | proposed |
| `columns[].derivation.compute.expr` | removed | expression | `numeric_expression` | decided (D18) |
| `columns[].derivation.round_half_away_from_zero.source` | unchanged | name | `variable` | proposed |
| `columns[].derivation.round_half_away_from_zero.digits` | unchanged | value | `int` | proposed |
| `columns[].derivation.diff.type` | unchanged | value | `temporal_type` | proposed |
| `columns[].derivation.diff.start` | unchanged | name | `variable` | proposed |
| `columns[].derivation.diff.end` | unchanged | name | `variable` | proposed |
| `columns[].derivation.diff.unit` | unchanged | value | `str` | proposed |
| `columns[].derivation.diff.bounds` | closed | value | `str` | proposed, P-ORD-2 |
| `columns[].derivation.impute.type` | unchanged | value | `temporal_type` | proposed |
| `columns[].derivation.impute.source` | unchanged | name | `variable` | proposed |
| `columns[].derivation.impute.month` | unchanged | value | `int` | proposed |
| `columns[].derivation.impute.day` | unchanged | value | `[int, day_rule]` | proposed |
| `columns[].derivation.impute.minimum_source_precision` | removed | value | `str` | proposed, P-IMP-1 |
| `columns[].derivation.impute.time` | unchanged | value | `time_rule` | proposed |
| `columns[].derivation.impute.not_before` | unchanged | name | `variable` | proposed |
| `columns[].derivation.impute.not_after` | unchanged | name | `variable` | proposed |
| `columns[].derivation.impute.fallback` | unchanged | name | `variable` | proposed |
| `columns[].derivation.impute.invalid` | unchanged | value | `substitute_value` | proposed |
| `columns[].derivation.precision.type` | unchanged | value | `temporal_type` | proposed |
| `columns[].derivation.precision.source` | unchanged | name | `variable` | proposed |
| `columns[].derivation.precision.invalid` | unchanged | value | `substitute_value` | proposed |
| `columns[].derivation.imputation_flag.source` | unchanged | name | `variable` | proposed |
| `columns[].derivation.imputation_flag.part` | unchanged | value | `str` | proposed |
| `columns[].derivation.imputation_flag.seconds_collected` | unchanged | value | `bool` | proposed |
| `columns[].derivation.to_date.source` | unchanged | name | `variable` | proposed |
| `columns[].derivation.study_day.date` | source | name | `variable` | proposed, P-OP-1 |
| `columns[].derivation.study_day.reference` | unchanged | name | `variable` | proposed |
| `columns[].derivation.to_epoch_day.source` | unchanged | name | `variable` | proposed |
| `columns[].derivation.dtc.year` | unchanged | name | `variable` | proposed |
| `columns[].derivation.dtc.month` | unchanged | name | `variable` | proposed |
| `columns[].derivation.dtc.day` | unchanged | name | `variable` | proposed |
| `columns[].derivation.dtc.hour` | unchanged | name | `variable` | proposed |
| `columns[].derivation.dtc.minute` | unchanged | name | `variable` | proposed |
| `columns[].derivation.dtc.second` | unchanged | name | `variable` | proposed |
| `columns[].derivation.dtc.invalid` | unchanged | value | `substitute_value` | proposed |
| `columns[].derivation.time_of_day.source` | unchanged | name | `variable` | proposed |
| `columns[].derivation.shift.type` | unchanged | value | `temporal_type` | proposed |
| `columns[].derivation.shift.source` | unchanged | name | `variable` | proposed |
| `columns[].derivation.shift.days` | unchanged | name | `[variable, int]` | proposed |
| `columns[].derivation.shift.amount` | unchanged | name | `[variable, int, float]` | proposed |
| `columns[].derivation.shift.unit` | unchanged | value | `str` | proposed |
| `columns[].derivation.duration_value.source` | unchanged | name | `variable` | proposed |
| `columns[].derivation.duration_value.unit` | unchanged | value | `str` | proposed |
| `columns[].derivation.duration_value.invalid` | unchanged | value | `substitute_value` | proposed |
| `columns[].derivation.row_number.window` | unchanged | structure | `window_selection` | proposed |
| `columns[].derivation.rank.method` | unchanged | value | `str` | proposed |
| `columns[].derivation.rank.window` | unchanged | structure | `window_selection` | proposed |
| `columns[].derivation.row_value.source` | unchanged | name | `variable` | proposed |
| `columns[].derivation.row_value.offset` | unchanged | value | `int` | proposed |
| `columns[].derivation.row_value.window` | unchanged | structure | `window_selection` | proposed |
| `columns[].derivation.locf.source` | unchanged | name | `variable` | proposed |
| `columns[].derivation.locf.include_current` | unchanged | value | `bool` | proposed |
| `columns[].derivation.locf.window` | unchanged | structure | `window_selection` | proposed |
| `columns[].derivation.baseline_flag.date` | unchanged | name | `variable` | proposed |
| `columns[].derivation.baseline_flag.reference_date` | reference | name | `variable` | proposed, P-OP-3 |
| `columns[].derivation.baseline_flag.value` | source | name | `variable` | proposed, P-OP-2 |
| `columns[].derivation.baseline_flag.inclusive` | unchanged | value | `bool` | proposed, P-ORD-2 |
| `columns[].derivation.baseline_flag.tie_break` | window.order_by: [<date>, ...tie_break] | name | `list[order_by_term]` | proposed, P-ORD-1 |
| `columns[].derivation.baseline_flag.window` | unchanged | structure | `window_selection` | proposed |
| `columns[].derivation.function.name` | unchanged | identifier | `identifier` | proposed |
| `columns[].derivation.function.args` | unchanged | expression | `dict[identifier, function_arg]` | proposed |
| `columns[].derivation.value` | unchanged | expression | `expression` | decided (D18) |
| `columns[].derivation.unconvertible` | unchanged | value | `substitute_value` | proposed |
| `columns[].submission.value_level[].id` | unchanged | identifier | `identifier` | decided (D17) |
| `columns[].submission.value_level[].where` | unchanged | value | `dict[identifier, list[where_value]]` | decided (D17) |
| `columns[].submission.value_level[].label` | unchanged | value | `str` | decided (D17) |
| `columns[].submission.value_level[].core` | unchanged | value | `core_designation` | decided (D17) |
| `columns[].submission.value_level[].mandatory` | unchanged | value | `bool` | decided (D17) |
| `columns[].submission.value_level[].role` | unchanged | value | `str` | decided (D17) |
| `columns[].submission.value_level[].data_type` | unchanged | value | `define_data_type` | decided (D17) |
| `columns[].submission.value_level[].length` | unchanged | value | `int` | decided (D17) |
| `columns[].submission.value_level[].significant_digits` | unchanged | value | `int` | decided (D17) |
| `columns[].submission.value_level[].display_format` | unchanged | value | `str` | decided (D17) |
| `columns[].submission.value_level[].codelist` | unchanged | identifier | `identifier` | decided (D17) |
| `columns[].submission.value_level[].inventory_vocabulary` | unchanged | value | `bool` | decided (D17) |
| `columns[].submission.value_level[].origin` | origins | structure | `submission_origin_class` | proposed, P-ORIGIN-1 |
| `columns[].submission.value_level[].method` | unchanged | value | `submission_method` | decided (D17) |
| `columns[].submission.value_level[].comment` | unchanged | value | `submission_comment` | decided (D17) |
| `intermediates[].derivations.*.type` | unchanged | value | `column_type` | proposed |
| `intermediates[].derivations.*.derivation` | unchanged | expression | `untyped_intermediate_derivation` | proposed |
| `intermediates[].derivations.*.unconvertible` | unchanged | value | `substitute_value` | proposed |
| `intermediates[].verifications[].unique.id` | unchanged | identifier | `verification_id` | proposed |
| `intermediates[].verifications[].unique.columns` | unchanged | name | `list[variable]` | proposed |
| `columns[].submission.method.name` | unchanged | value | `str` | proposed |
| `columns[].submission.method.type` | unchanged | value | `method_type` | proposed |
| `columns[].submission.method.description` | unchanged | value | `str` | proposed |
| `columns[].submission.method.expression` | unchanged | structure | `formal_expression_class` | proposed |
| `columns[].submission.method.documents` | unchanged | identifier | `list[document_reference]` | proposed |
| `columns[].derivation.source.variable` | unchanged | name | `variable` | proposed |
| `columns[].derivation.source.between` | unchanged | structure | `record_between_class` | proposed |
| `columns[].derivation.source.filter` | unchanged | expression | `predicate` | proposed |
| `columns[].derivation.source.order_by` | unchanged | name | `list[order_by_term]` | proposed |
| `columns[].derivation.source.keep` | unchanged | value | `keep_policy` | proposed |
| `columns[].derivation.source.absent` | unchanged | value | `substitute_value` | proposed, P-HND-5 |
| `columns[].derivation.source.require_match` | unchanged | value | `bool` | proposed, P-HND-2 |
| `columns[].derivation.flag.condition` | unchanged | expression | `predicate` | proposed |
| `columns[].derivation.flag.true_value` | unchanged | value | `substitute_value` | proposed |
| `columns[].derivation.flag.false_value` | unchanged | value | `substitute_value` | proposed |
| `columns[].derivation.flag.missing` | unchanged | value | `substitute_value` | proposed |
| `columns[].derivation.str_template.template` | unchanged | expression | `string_template` | proposed |
| `columns[].derivation.str_template.missing` | unchanged | value | `substitute_value` | proposed |
| `columns[].derivation.aggregate.key` | unchanged | expression | `match_key` | proposed, P-KEY-1 |
| `columns[].derivation.aggregate.between` | unchanged | structure | `record_between_class` | proposed |
| `columns[].derivation.aggregate.filter` | unchanged | expression | `predicate` | proposed |
| `columns[].derivation.aggregate.group_by` | unchanged | name | `list[variable]` | proposed |
| `columns[].derivation.aggregate.derive` | derivations: {NAME: {type: T, derivation: E}} | structure | `list[derive_binding_class]` | proposed, P-BIND-1 |
| `columns[].derivation.aggregate.expr` | unchanged | expression | `aggregate_expression` | proposed |
| `columns[].derivation.odm.item` | unchanged | identifier | `odm_item` | proposed, P-ODM-1 |
| `columns[].derivation.odm.event` | unchanged | identifier | `list[odm_oid]` | proposed |
| `columns[].derivation.odm.form` | unchanged | identifier | `list[odm_oid]` | proposed |
| `columns[].derivation.odm.item_group` | unchanged | identifier | `list[odm_oid]` | proposed |
| `columns[].derivation.odm.filter` | unchanged | expression | `predicate` | proposed |
| `columns[].submission.method.expression.context` | unchanged | value | `str` | proposed |
| `columns[].submission.method.expression.code` | unchanged | value | `str` | proposed |
| `intermediates[].derivations.*.value` | removed | expression | `expression` | proposed, P-BIND-2 |
| `columns[].submission.origin.documents[].document` | unchanged | identifier | `identifier` | proposed |
| `columns[].submission.origin.documents[].pages` | unchanged | structure | `page_reference_class` | proposed |
| `columns[].derivation.case[].when` | unchanged | expression | `predicate` | proposed |
| `columns[].derivation.case[].then` | unchanged | expression | `case_result` | proposed |
| `columns[].derivation.case[].otherwise` | unchanged | expression | `case_result` | proposed |
| `columns[].derivation.aggregate.derive[].name` | map key of `derivations` | identifier | `identifier` | proposed, P-BIND-1 |
| `columns[].derivation.aggregate.derive[].type` | unchanged | value | `column_type` | proposed |
| `columns[].derivation.aggregate.derive[].derivation` | unchanged | expression | `derivation` | proposed |
| `columns[].derivation.function.args.*.literal` | removed | value | `str` | decided (D18) |
| `columns[].derivation.function.args.*.date` | removed | value | `str` | decided (D18) |
| `columns[].derivation.function.args.*.datetime` | removed | value | `str` | decided (D18) |
| `columns[].submission.origin.documents[].pages.type` | unchanged | value | `str` | proposed |
| `columns[].submission.origin.documents[].pages.refs` | unchanged | value | `str` | proposed |
| `columns[].submission.origin.documents[].pages.title` | unchanged | value | `str` | proposed |
| `yamaa` | yamaa: "1.N" | value | | decided (D4) |
| `type` | type: spec | value | | decided (D4) |
| `clear` | clear: [SPEC_PATH, ...] | value | | decided (D18), P-CLR-1 |
| `submission.standard` | submission: {standard: NAME} | identifier | | proposed, P-STD-1 |
| `columns[].submission.value_level[].where.*` | {comparator: NE \| NOTIN, values: [V, ...]} beside today's list of values | value | | proposed, P-VLM-1 |
| `model` | model: SDTM \| SEND \| ADaM | value | | proposed, P-MODEL-1 |
| `columns[].copy` | copy: {dataset: INPUT, columns: [NAME, ...], submission: {core, mandatory, role}} | structure | | proposed, P-COPY-1 |
| `input.*.key` | key: [COLUMN, ...] | identifier | | proposed, P-JOIN-1 |
| `columns[].derivation.cut.closed` | closed: left \| right (default left) | value | | proposed, P-ORD-2 |

## Project files (`type: project`)

| Path | 1.0 spelling | Kind | Type today | Decision |
| --- | --- | --- | --- | --- |
| `schema_version` | `yamaa: "1.0"` and `type: project` | value | `str` | decided (D4) |
| `environments` | environments: {python: {lock: PATH}, r: {lock: PATH}} | structure | `environments_class` | decided (D8) |
| `codelists` | unchanged | structure | `list[codelist_source]` | decided (D3) |
| `study` | unchanged | structure | `study_class` | decided (D3) |
| `sdtm` | unchanged | structure | `submission_section_class` | decided (D3) |
| `adam` | unchanged | structure | `submission_section_class` | decided (D3) |
| `send` | unchanged | structure | `submission_section_class` | decided (D3) |
| `environments.python` | {lock: PATH} | path | `project_path` | decided (D8) |
| `environments.r` | {lock: PATH} | path | `project_path` | decided (D8) |
| `study.id` | unchanged | identifier | `define_id` | decided (D3) |
| `study.name` | unchanged | value | `str` | decided (D3) |
| `study.description` | unchanged | value | `str` | decided (D3) |
| `study.protocol_name` | unchanged | value | `str` | decided (D3) |
| `sdtm.specs` | unchanged | path | `list[path]` | decided (D3) |
| `sdtm.standard` | standards | structure | `implementation_guide_class` | decided (D16) |
| `sdtm.define` | unchanged | path | `path` | decided (D3) |
| `sdtm.documents` | unchanged | structure | `list[document_class]` | decided (D3) |
| `sdtm.datasets` | transport | value | `str` | proposed, P-PRJ-1 |
| `sdtm.context` | unchanged | value | `str` | decided (D3) |
| `sdtm.xml_lang` | unchanged | value | `language_tag` | decided (D3) |
| `sdtm.file_oid` | unchanged | value | `str` | decided (D3) |
| `sdtm.metadata_version` | unchanged | structure | `metadata_version_class` | decided (D3) |
| `sdtm.originator` | unchanged | value | `str` | decided (D3) |
| `sdtm.stylesheet` | unchanged | path | `relative_href` | decided (D3) |
| `sdtm.source_system_version` | unchanged | value | `str` | decided (D3) |
| `sdtm.standard.name` | unchanged | value | `standard_name` | decided (D3) |
| `sdtm.standard.version` | unchanged | value | `str` | decided (D3) |
| `sdtm.standard.status` | unchanged | value | `str` | decided (D3) |
| `sdtm.metadata_version.id` | unchanged | identifier | `define_id` | decided (D3) |
| `sdtm.metadata_version.name` | unchanged | value | `str` | decided (D3) |
| `sdtm.metadata_version.description` | unchanged | value | `str` | decided (D3) |
| `sdtm.documents[].id` | unchanged | identifier | `identifier` | decided (D3) |
| `sdtm.documents[].kind` | unchanged | value | `str` | decided (D3) |
| `sdtm.documents[].link` | unchanged | path | `relative_href` | decided (D3) |
| `sdtm.documents[].title` | unchanged | value | `str` | decided (D3) |
| `yamaa` | yamaa: "1.N" | value | | decided (D4) |
| `type` | type: project | value | | decided (D4) |
| `functions` | functions: [PATH, ...] | path | | decided (D8) |
| `environments.<host>.lock` | environments: {python: {lock: uv.lock}, r: {lock: renv.lock}} | path | | decided (D8) |

## Codelist files (`type: codelists`) and inline codelist sources

Paths are written as in a project file's inline `codelists` list; in a
codelist file the same fields sit at the root, after the header.

| Path | 1.0 spelling | Kind | Type today | Decision |
| --- | --- | --- | --- | --- |
| `codelists[].standard` | unchanged | structure | `terminology_standard_class` | decided (D3) |
| `codelists[].codelists` | unchanged | structure | `list[codelist_class]` | decided (D3) |
| `codelists[].standard.name` | unchanged | value | `str` | decided (D3) |
| `codelists[].standard.publishing_set` | unchanged | value | `str` | decided (D3) |
| `codelists[].standard.version` | unchanged | value | `str` | decided (D3) |
| `codelists[].codelists[].id` | unchanged | identifier | `identifier` | decided (D3) |
| `codelists[].codelists[].name` | unchanged | value | `str` | decided (D3) |
| `codelists[].codelists[].data_type` | unchanged | value | `str` | decided (D3) |
| `codelists[].codelists[].extensible` | unchanged | value | `bool` | decided (D3) |
| `codelists[].codelists[].alias` | unchanged | value | `str` | proposed, P-PRJ-3 |
| `codelists[].codelists[].format_name` | unchanged | value | `str` | decided (D3) |
| `codelists[].codelists[].items` | unchanged | structure | `list[codelist_item_class]` | decided (D3) |
| `codelists[].codelists[].external` | unchanged | structure | `external_codelist_class` | decided (D3) |
| `codelists[].codelists[].external.dictionary` | unchanged | value | `str` | decided (D3) |
| `codelists[].codelists[].external.version` | unchanged | value | `str` | decided (D3) |
| `codelists[].codelists[].external.href` | unchanged | value | `str` | decided (D3) |
| `codelists[].codelists[].items[].value` | unchanged | value | `literal_value` | decided (D3) |
| `codelists[].codelists[].items[].decode` | unchanged | value | `str` | decided (D3) |
| `codelists[].codelists[].items[].rank` | unchanged | value | `int` | decided (D3) |
| `codelists[].codelists[].items[].alias` | unchanged | value | `str` | proposed, P-PRJ-3 |
| `codelists[].codelists[].items[].extended` | unchanged | value | `bool` | decided (D3) |
| `yamaa` | yamaa: "1.N" | value | | decided (D4) |
| `type` | type: codelists | value | | proposed, P-TYPE-1 |

## Function files (`type: function`)

Paths are relative to the function definition, which is the root of a
function file in 1.0 (today it sits under an environment file's
`functions` map).

| Path | 1.0 spelling | Kind | Type today | Decision |
| --- | --- | --- | --- | --- |
| `function` | implementations: {python: CALLABLE, r: CALLABLE} | identifier | `qualified_callable` | decided (D8) |
| `description` | unchanged | value | `str` | decided (D8) |
| `params` | unchanged | structure | `list[function_parameter_class]` | decided (D8) |
| `returns` | unchanged | value | `column_type` | decided (D8) |
| `may_return_missing` | unchanged | value | `bool` | decided (D8) |
| `comparison_decimals` | unchanged | value | `int` | decided (D8) |
| `tests` | unchanged | structure | `list[function_test_class]` | decided (D8) |
| `params[].name` | unchanged | identifier | `identifier` | decided (D8) |
| `params[].type` | unchanged | value | `function_param_type` | decided (D8) |
| `params[].required` | unchanged | value | `bool` | decided (D8) |
| `params[].default` | unchanged | value | `function_value` | decided (D8) |
| `params[].accepts_missing` | unchanged | value | `bool` | decided (D8) |
| `tests[].id` | unchanged | identifier | `function_case_id` | decided (D8) |
| `tests[].covers` | unchanged | identifier | `list[function_coverage]` | decided (D8) |
| `tests[].args` | unchanged | value | `dict[identifier, function_value]` | decided (D8) |
| `tests[].result` | unchanged | value | `function_value` | decided (D8) |
| `params[].default.date` | bare text typed by the declared parameter or return type | value | `str` | proposed, P-FN-1 |
| `params[].default.datetime` | bare text typed by the declared parameter or return type | value | `str` | proposed, P-FN-1 |
| `yamaa` | yamaa: "1.N" | value | | decided (D4) |
| `type` | type: function | value | | decided (D8) |
| `name` | name: IDENTIFIER | identifier | | decided (D8) |
| `implementations` | implementations: {python: CALLABLE, r: CALLABLE} | value | | decided (D8) |
