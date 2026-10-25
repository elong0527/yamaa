# Expressions

A derivation computes its value with one expression from a closed registry.
These rules define the registry and dispatch, each expression's result type,
and the expressions that copy, select and classify values.
[Formulas](formulas.md), [Lookup and joins](lookup.md), [Windows](windows.md)
and the text, temporal and function operations define the rest.

## Registry and evaluation

<a id="expr-1"></a>

### EXPR-1. A derivation holds exactly one registered expression keyword, which owns its payload

An expression contains exactly one keyword of the `expressions` registry. Its
variable dependencies are resolved, then the keyword is evaluated; fields whose
declared type contains `expression` are evaluated recursively, and YAML
mapping order has no execution meaning. Each keyword owns all its inputs,
options, grouping, local error handlers and operation-local semantics, and
adding a keyword takes one complete registry entry. An unknown keyword or an
unknown payload field fails validation, and an unsatisfied semantic constraint
of an operation fails.

Evidence: `schema-expressions`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/schema.json::a_registry_value_naming_no_keyword_declares_no_operation`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/schema.json::a_registry_value_naming_a_keyword_the_registry_lacks_is_an_unknown_operation`.

`schema_expression_*.yaml` contribute the entries of the `expressions`
registry under [Schema language](reference/schema-language.md), and
`schema_derivation.yaml` exposes that registry as the `expression` type.
Most keywords name their input variables directly; text in an expression
field, a YAML number, Boolean or null, and a structured `source` are leaves
([EXPR-3](#expr-3)).

The schema registry owns payload shape, defaults and structural constraints.
The operation's rule defines evaluation, input and result semantics, missing
behavior and failures. Schema descriptions point to that rule; they do not
define a competing operation contract or perform validation. An unregistered
keyword or an invalid payload fails under
[Schema language](reference/schema-language.md), and a semantic constraint of
an operation definition or another applicable rule that is not satisfied
fails.

<a id="expr-2"></a>

### EXPR-2. An expression yields one value per row, and only windows and aggregates read other rows

A scalar expression returns one value per current row. A window expression
evaluates over the partitions its own `group_by` declares
([WINDOW-3](windows.md#window-3)), and an aggregate expression evaluates in
the contexts [LOOKUP-18](lookup.md#lookup-18) permits. A scalar or window
expression that changes the row count fails under
[Column derivation](columns.md), which owns the phase
invariant.

<a id="expr-3"></a>

### EXPR-3. Expressions nest only in case results, key match values and a nearest selection's `to`

Text in an expression field, a YAML number, Boolean or null, and a
structured `source` are expression leaves. Every other expression names its
input variables directly, except in fields whose declared type contains
`expression`, which are evaluated recursively: each `case` `then` and
trailing `otherwise`, a `key` match value under
[LOOKUP-6](lookup.md#lookup-6), and a `nearest` selection's `to` under
[LOOKUP-26](lookup.md#lookup-26). A derivation's own top-level expression,
in `derivation` or `handled_expression_class.value`, is not restricted by
this rule. That text and fields typed `string_template` and
`aggregate_expression` are leaves whose identifiers their own rules resolve.

Evidence: `schema-expressions`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_model.json::a_literal_nearest_reference_checks_clean`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_model.json::a_literal_nearest_reference_is_the_same_for_every_ecg`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_model.json::an_expression_nearest_reference_checks_clean`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_model.json::an_expression_nearest_reference_reads_the_current_row`.

`case` selects among expressions, a key match value computes the operand it
matches, and a nearest selection's `to`, read as a key match value is,
computes the value each candidate's distance is measured to.
[FORMULA-9](formulas.md#formula-9) resolves the identifiers of
expression-field text and of `aggregate_expression`, and
[Text](text.md) those of `string_template`.

A plain string's meaning follows its field's kind ([EXPR-5](#expr-5)):
grammar text in an expression field, one variable in a name field
([NAME-5](names.md#name-5)) and text in a value field. A function argument is
one leaf ([FUNC-6](profiles/functions.md#func-6)).

<a id="expr-14"></a>

### EXPR-14. Every expression has one declared result type function, and every context reads its type

Each registered expression has one result type function, decided before any
data is read: fixed by the expression, set by a declared parameter, or taken
from the types of what it reads, as the table below gives. A selecting
expression's type is the set of the types it can select; a computing
expression's type is one type. Key matching, intermediate fields and drivers,
aggregate derivations, range bounds, operand checks, extremes and column
conversion all read that type.

Evidence: `rust/crates/yamaa-core/src/ast/types.rs::verbs_fix_or_set_their_result_type`, `rust/crates/yamaa-core/src/specification/types/tests.rs::numeric_and_aggregate_result_types_follow_their_operands`, `rust/crates/yamaa-core/src/specification/types/tests.rs::declared_types_follow_each_inputs_type_authority`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/intermediates.json::a_computed_float_key_of_a_text_donor_column_is_refused_before_matching`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/intermediates.json::a_greatest_key_of_int_sources_on_a_text_donor_column_is_refused_before_matching`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/intermediates.json::a_key_whose_type_is_decided_per_value_binds_and_is_compared_when_matched`, `rust/crates/yamaa-core/tests/relations_reference_scope.rs::a_driving_intermediate_exposes_each_derived_field_as_its_one_result_type`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/intermediates.json::a_driving_intermediate_field_whose_type_is_decided_per_value_has_no_layout`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/project_functions.json::a_callback_derived_field_drives_rows_with_its_declared_result_type`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/completed_intermediates.json::nullif_over_an_int_and_a_float_is_refused_as_integer_days_before_data`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/completed_intermediates.json::nullif_over_an_int_and_a_float_is_refused_as_integer_days_when_bound`.

| Expression | Class | Result type |
| --- | --- | --- |
| `str_pad`, `str_substr`, `str_decimal`, `str_extract`, `str_replace`, `str_template`, `str_case` | fixed | `str` |
| `cut`, `precision`, `imputation_flag`, `baseline_flag`, `odm`, `dtc`, `date_text` | fixed | `str` |
| `row_number`, `rank`, `study_day`, `to_epoch_day`, `time_of_day`, `str_length` | fixed | `int` |
| `round_half_away_from_zero`, `duration_value` | fixed | `float` |
| `duration_text` | fixed | `str` |
| `to_date` | fixed | `date` |
| `diff` | by parameter | `int` with `type: date`; with `type: datetime`, `int` in `second` and `float` in `minute`, `hour` or `day` |
| `impute`, `shift` | by parameter | the declared `type` |
| `function` | by parameter | the result type its definition declares ([FUNC-9](profiles/functions.md#func-9)) |
| `source`, `row_value`, `locf` | from inputs | the variable's type |
| text in an expression field | from inputs | a reference's variable type; a literal's type, under [FORMULA-3](formulas.md#formula-3) for a grammar literal and `int`, `float` or `bool` for a YAML scalar, with YAML null always missing; or [VALUE-10](values.md#value-10) over arithmetic: `int` while every operator and operand keeps `int`, and `float` otherwise |
| `first_available` | from inputs | the types its `sources` and `missing` can take |
| `greatest`, `least` | from inputs | the types its `sources` can take |
| `case` | from inputs | the types its `then` and `otherwise` values can take |
| `mapping` | from inputs | the types its dictionary values can take |
| `flag` | from inputs | the types its `true_value`, `false_value` and `missing` can take |
| `aggregate` | from inputs | [FORMULA-23](formulas.md#formula-23) over its relation's fields and aggregate derivations |

A variable's type is its column's declared type, a stored field's type under
[INPUT-5](inputs.md#input-5), or an intermediate field's: a typed
derivation's declared type and an untyped one's result type
([LOOKUP-15](lookup.md#lookup-15)). Before capture, as `yamaa check` reads a
specification and as every operand check does, a stored field has the type
its input declares: its producer's type for a linked input
([INPUT-10](inputs.md#input-10)), its `types` entry, and `str` for every
other field of a delimited input, as INPUT-5 types each such field. Any other
undeclared field, of a SAS, Transport or self-describing input, has only the
type its captured schema gives, so before capture its type and every type
taken from it are not yet known. So is a name a lookup reads from the current
record that another intermediate over the same input derives: the record of
the intermediate that reads the lookup holds that field.

The selecting expressions are a reference, a literal, `source`, `row_value`,
`locf`, `first_available`, `greatest`, `least`, `case`, `mapping` and `flag`. Each
returns the value it selects unchanged, with that value's own type
([EXPR-6](#expr-6)), so its type is the set of the types of its alternatives:
one type when every alternative has it, and otherwise a type decided per value,
such as `int` or `float` for a `case` between an `int` and a `float` column.
YAML null adds no type, and local `missing`, `invalid`, `absent`,
`unmapped` and `no_match` handlers take no part.

Every other expression computes its result, and its type is one type. `NULLIF`
is a `float` when either argument is a `float`, also when that argument is
missing ([VALUE-10](values.md#value-10)), and `duration_value` is a `float` in
every unit ([TIME-17](temporal.md#time-17)). Over an operand whose type is
decided per value, a computing expression gives one result type for each type
the operand can take.

Key matching ([LOOKUP-6](lookup.md#lookup-6)) and column conversion
([VALUE-24](values.md#value-24)) read the whole of a type decided per value
and fail before data only when none of the types it can take compares or
converts. Every other context reads one type and checks a type decided per
value when each value is used; an intermediate drives rows only when every
field it exposes has one column type ([ROW-2](rows.md#row-2)).

## Values and shorthand

<a id="expr-4"></a>

### EXPR-4. A reference or a structured source copies a named variable, and a literal returns its value unchanged

A reference or a structured `source` copies a named source or derived
variable, and a literal returns its value unchanged ([EXPR-5](#expr-5)). Each
is an expression leaf.

Evidence: `schema-expressions`, `schema-source-binding`.

A dataset-qualified reference reads through the implicit join, and a
structured `source:` binding adds its range, filter, selection and absent
value ([LOOKUP-1](lookup.md#lookup-1)). Each returns its variable's value
under [EXPR-6](#expr-6), with the variable's type ([EXPR-14](#expr-14)). A
literal is quoted text, a number, `NULL` or a `DATE` or `DATETIME` literal
written as expression-field text, or a YAML number, Boolean or null.

<a id="expr-5"></a>

### EXPR-5. An expression field reads a bare name as a reference and quoted text as a literal

A `derivation`, a `derivations` entry, `then`, `otherwise`, a handled
wrapper's `value`, a key match value, a nearest selection's `to` and a
function argument are expression fields: each holds one expression of the
closed grammar ([FORMULA-1](formulas.md#formula-1)), in which a bare name is a
reference, quoted text such as `'DM'` is a literal, and a YAML number,
Boolean or null is a literal. A forgotten quote therefore fails validation as
an unknown name. A value field holds a constant, and its bare text is text.

Fails: `unknown_field`, `redundant_long_form`.
Evidence: `rust/crates/yamaa-adapters/tests/declaration_v1_lowering.rs::text_holding_one_literal_lowers_to_it_and_other_text_to_a_formula`, `schema-shorthand`, `schema-expressions`, `rust/crates/yamaa-core/tests/schema_validation.rs::bare_derivation_strings_are_source_references_never_calls`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/schema.json::a_derivation_s_source_long_form_restates_the_bare_string_source`.

Text in an expression field is one expression in the derivation context of
[FORMULA-27](formulas.md#formula-27): text that holds only a name is a
reference, text that holds only a literal is that literal
([EXPR-4](#expr-4)), and other text returns one value per current row. So
`derivation: DM.AGE` copies `DM.AGE`, `derivation: "'DM'"` is the text `DM`,
`derivation: 5` and `derivation: "5"` are the number 5, and
`derivation: "AGE * 2"` computes. Of two such spellings the YAML scalar is
canonical, and the lint reports the text as `scalar_as_text`. An operation is
written as its mapping, such as `{to_date: {source: DM.BRTHDTC}}`; written as
text, `to_date(DM.BRTHDTC)` calls a function the field does not admit and fails
as `prohibited_function` under FORMULA-27. A name that no input or column
declares fails when it binds, as `unknown_field` under
[NAME-3](names.md#name-3), so a forgotten quote around `DM` never becomes the
text `DM`.

A reference keeps the canonical `source` binding and its path, so a finding
about `derivation: DM.AGE` names `derivation.source`; a literal or other text
is located at its field, such as `derivation` or `case[0].then`. The
structured `source:` binding ([LOOKUP-1](lookup.md#lookup-1)) stays where it
adds a range, filter, selection or absence answer.

The schema marks each field's kind. A value field, such as a handler, a
`dict` value, a `cut` label, a `flag` value or an `allowed_values` entry,
holds a constant, in which bare text is text and YAML null is missing
([COLUMN-10](columns.md#column-10)). The specification reader uses YAML 1.2
core, so only `true`/`false` spellings, numbers and null parse as non-strings;
quote text that YAML would otherwise parse as one.

<a id="expr-6"></a>

### EXPR-6. An expression that selects a value returns it unchanged, with its collected precision

A reference or a structured `source` returns the value of its variable, and a
literal its value after non-finite normalization. Mapping, conditional,
coalescing and extreme expressions return the value they select, never a
converted one, and a selected temporal value keeps its collected precision. The result type each
expression declares is [EXPR-14](#expr-14)'s.

Evidence: `rust/crates/yamaa-application/tests/fixtures/first_available.json`, `schema-expressions`.

## Conditional selection

<a id="expr-7"></a>

### EXPR-7. case returns the value of its first TRUE branch, then otherwise, else missing

A `case` is a non-empty list of items. Every item declares `when` and `then`,
except that one item may declare `otherwise` instead, and that item, when
present, is the last. The `case` returns the `then` value of the first branch
whose `when` predicate is `TRUE`, then the `otherwise` value, or missing. A
`case` with no `when`/`then` item, more than one `otherwise` item, or an
`otherwise` item in any other position fails.

Fails: `invalid_field_type`.
Evidence: `schema-expressions`, `negative-case-otherwise-only`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/schema.json::case_otherwise_reports_inside_the_otherwise_member`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/schema.json::a_case_list_without_a_branch_or_with_an_otherwise_before_its_end_is_refused_under_expr_7`.

A malformed `case` fails as `invalid_field_type`, at the `case` when it has no
`when`/`then` item and at each `otherwise` item that is not the last item.
`FALSE` and `UNKNOWN` select no branch under the three-valued logic of
[FORMULA-4](formulas.md#formula-4).

<a id="expr-8"></a>

### EXPR-8. flag returns a declared value for each of TRUE, FALSE and UNKNOWN

`flag` evaluates its `condition` predicate: only `TRUE` selects `true_value`,
which defaults to `"Y"`; `FALSE` returns `false_value`, or missing when it is
absent; `UNKNOWN` returns `missing`, or missing when it is absent, counted
under `missing` either way. An unknown condition never falls through to
`false_value`. A `flag` that declares `false_value` also declares `missing`,
which may repeat `false_value`, name another value, or be YAML null for
missing.

Fails: `missing_value_required`.
Evidence: `negative-flag-missing-value`, `negative-flag-condition`, `adam-adae-occurrence-flags`, `rust/crates/yamaa-application/tests/fixtures/flag_reference.json`.

`flag` is the shorthand for the common one-branch `case` that returns a flag
value. A bare predicate string is the condition with the default values:
`true_value: "Y"`, and `false_value` and `missing` missing; the mapping form
names `condition` beside the values it declares. The condition follows the
three-valued logic [FORMULA-4](formulas.md#formula-4) gives `case`; unlike
`case` with `otherwise`, `UNKNOWN` does not reach `false_value`. Each `UNKNOWN`
condition is counted at the flag's `missing` path
([COLUMN-14](columns.md#column-14)), so a missing value a flag gives for an
unknown condition is reported even when `missing` is omitted; `FALSE` is a
known answer and is not counted.

A `flag` that declares `false_value` without `missing` fails as
`missing_value_required` at `missing`. A `flag` whose payload is neither a
predicate string nor a mapping, whose mapping form lacks `condition` or names a
non-predicate `condition`, or whose condition is not a valid predicate fails.
A mapping that holds only `condition` restates the bare predicate and fails as
`redundant_long_form` under
[SCHEMA-15](reference/schema-language.md#schema-15).

<a id="expr-9"></a>

### EXPR-9. first_available returns the first source that is not missing

`first_available` tests its `sources` in order and returns the value of the
first source that is not missing. When every source is missing, it returns the
declared `missing`, or missing when none is declared, counted under `missing`
either way.

Evidence: `schema-expressions`, `adam-adsl-demographics`, `rust/crates/yamaa-application/tests/fixtures/first_available.json`.

A source that is a binding reading through an implicit join answers its own
absent record first ([LOOKUP-24](lookup.md#lookup-24)); the answer, missing by
default, is what `first_available` then tests.

<a id="expr-10"></a>

### EXPR-10. greatest and least return the largest or smallest non-missing source

`greatest` returns the largest non-missing value among its `sources`, and
`least` the smallest; each returns missing when no source has a value. The
sources must be mutually comparable.

Evidence: `schema-expressions`, `negative-greatest-mixed`, `negative-least-mixed`.

[VALUE-19](values.md#value-19) checks that comparability and states
which source an equal extreme keeps; `int` and `float` compare exactly under
[VALUE-19](values.md#value-19).

## Classification

<a id="expr-11"></a>

### EXPR-11. mapping replaces a string source with its dictionary value

`mapping` looks up a string `source` in `dict`, a source-value to result-value
dictionary written inline or as the path of a YAML file holding it, and
returns the matching entry's value. A missing source returns `missing`, or
missing when it is omitted, counted under `missing`. A present source with no
dictionary entry returns `unmapped`, and fails without it; `missing` never
answers an unmapped source ([COLUMN-10](columns.md#column-10)).

Fails: `incompatible_input_type`.
Evidence: `schema-text-mapping`, `schema-text-mapping-unmapped`, `negative-mapping-unmapped-value`, `schema-missing-input-default`.

A `mapping` requires a string source. The source may be a filtered source
carrying the variable with its range, filter, selection or absent value
([LOOKUP-1](lookup.md#lookup-1)). A dictionary path is read once during
workflow planning through the specification's
[project resources](reference/files.md), and its content must satisfy the
inline dictionary contract. `case_sensitive` selects exact comparison when
true and the ASCII fold of [EXPR-12](#expr-12) when false.

<a id="expr-12"></a>

### EXPR-12. A case-insensitive mapping folds ASCII letters, and its folded keys must be unique

With `case_sensitive: true`, `mapping` compares its source with the dictionary
keys by [Values](values.md) equality. With `false`, it folds the
source and every dictionary key by replacing `U+0061` through `U+007A` with
`U+0041` through `U+005A`, leaving every other scalar unchanged, and compares
the folded sequences by exact equality. Dictionary keys must be unique after
this fold, or validation fails.

Fails: `ambiguous_dictionary`.
Evidence: `negative-mapping-case-collision`, `schema-text-mapping`, `sdtm-lb-character-results`.

Keys that collide after the fold fail validation with `ambiguous_dictionary`,
reporting the folded key and the original entries. A non-ASCII value can match
only the same non-ASCII scalar sequence, apart from ASCII letters that also
occur in that sequence.

<a id="expr-13"></a>

### EXPR-13. cut assigns a numeric source to one of its labeled break intervals

`cut` assigns a numeric `source` to one of the intervals its ascending
`breaks` bound and returns that interval's label. `labels` holds one label per
interval, exactly one more than `breaks`. With `closed: right` the intervals
are right-closed and left-open; with `closed: left`, the default, they are
left-closed and right-open. A missing source returns `missing`, or missing
when it is omitted, counted under `missing`, and a non-numeric source fails.

Fails: `incompatible_input_type`, `invalid_field_type`.
Evidence: `schema-expressions`, `negative-cut-coded`, `schema-missing-input-default`.

The source may be a filtered source carrying the variable with its range,
filter, selection or absent value ([LOOKUP-1](lookup.md#lookup-1)). Breaks compare
mathematical values exactly under [VALUE-19](values.md#value-19).
Without `missing`, a missing source gives missing, the default substitution of
[COLUMN-10](columns.md#column-10).
