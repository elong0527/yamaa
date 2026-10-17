# Expressions

A derivation computes its value with one expression from a closed registry.
These rules define the registry and dispatch, and the expressions that copy,
select and classify values. [Formulas](formulas.md),
[Lookup and joins](lookup.md), [Windows](windows.md) and the text, temporal
and function operations define the rest.

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

Evidence: `schema-expressions`, `rust/crates/yamaa-core/tests/schema_references.rs::registry_and_bare_derivation_diagnostics_preserve_paths_and_context`, `negative-tied-baseline`.
Replaces: REQ-0045, REQ-0288, REQ-0289, REQ-0318, REQ-0321, REQ-0322.

`schema_expression_*.yaml` and `schema_function.yaml` contribute the entries of
the `expressions` registry under [Schema language](reference/schema-language.md),
and `schema_derivation.yaml` exposes that registry as the `expression` type.
Most keywords name their input variables directly; `source` and `literal` are
leaves ([EXPR-3](#expr-3)).

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

Replaces: REQ-0046, REQ-0325.

<a id="expr-3"></a>

### EXPR-3. Expressions nest only in case results and key match values

`source` and `literal` are expression leaves. Every other expression names its
input variables directly, except in fields whose declared type contains
`expression`, which are evaluated recursively: each `case` `then` and trailing
`otherwise`, and a `key` match value under [LOOKUP-6](lookup.md#lookup-6). A
derivation's own top-level expression, in `derivation` or
`handled_expression_class.value`, is not restricted by this rule. Fields typed `numeric_expression`,
`string_template` and `aggregate_expression` are leaves whose identifiers
their own rules resolve.

Evidence: `schema-expressions`.
Replaces: REQ-0290, REQ-0291, REQ-0292.

`case` selects among expressions, and a key match value computes the operand
it matches. [FORMULA-10](formulas.md#formula-10),
[Text](text.md) and
[FORMULA-17](formulas.md#formula-17) resolve the identifiers of the three leaf
types.

A plain string is a value unless its schema field is typed `variable`,
`function_arg`, `predicate` or `string_template`.
[Project functions](profiles/functions.md) closes `function_arg`: a string
is a variable, while string, date and datetime literals use their explicit
tagged leaf forms.

## Values and shorthand

<a id="expr-4"></a>

### EXPR-4. source copies a named variable, and literal returns its declared scalar unchanged

`source` copies a named source or derived variable, and `literal` returns the
declared scalar value unchanged. Both are expression leaves.

Evidence: `schema-expressions`, `schema-source-binding`.
Replaces: REQ-1093, REQ-1094.

A dataset-qualified `source` reads through the implicit join, and a structured
`source:` binding adds its filter, selection and absent value
([LOOKUP-1](lookup.md#lookup-1)). Each keeps its type under
[EXPR-6](#expr-6).

<a id="expr-5"></a>

### EXPR-5. A bare string derivation or case result is a source reference, never a literal

A `derivation`, `case_branch_class.then` or `case_otherwise_class.otherwise`
written as a bare string is the source shorthand: it desugars to
`{source: <string>}` before registry dispatch, and a validated document
contains only that canonical form. The string is always a source reference,
never a literal, so a bare `DM` with no variable `DM` fails validation rather
than producing the literal `"DM"`. A `derivation` that is a non-string scalar
is invalid, and the error names the dict form.

Evidence: `schema-shorthand`, `schema-expressions`, `rust/crates/yamaa-core/tests/schema_references.rs::registry_and_bare_derivation_diagnostics_preserve_paths_and_context`.
Replaces: REQ-0319, REQ-0320.

For a `derivation`, the handled-expression expansion of
[SCHEMA-10](reference/schema-language.md#schema-10) then applies unchanged.
`derivation: 5` must be written `{literal: 5}`. The specification reader uses
YAML 1.2 core, so only `true`/`false` spellings, numbers and null parse as
non-strings; quote a column reference that YAML would otherwise parse as a
non-string.

<a id="expr-6"></a>

### EXPR-6. An expression that selects a value returns it with its type and collected precision

`source` keeps its source type under [Inputs](inputs.md),
and `literal` keeps its YAML scalar type after non-finite normalization.
Mapping, conditional, coalescing and extreme expressions keep the selected
value's type, including its collected precision when the selected value is
temporal. Numeric computation follows [Formulas](formulas.md), and each text
operation's result follows its own rule.

Evidence: `rust/crates/yamaa-core/tests/dataset_callbacks.rs::first_available_skips_unselected_reads_and_does_not_fallback_after_conversion`, `schema-expressions`.
Replaces: REQ-0315.

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
Evidence: `schema-expressions`, `negative-case-otherwise-only`, `rust/crates/yamaa-core/tests/schema_references.rs::case_otherwise_reports_inside_the_otherwise_member`.
Replaces: REQ-0339, REQ-1098, REQ-1099, REQ-1100, REQ-1101.

A malformed `case` fails as `invalid_field_type`, at the `case` when it has no
`when`/`then` item and at each `otherwise` item that is not the last item.
`FALSE` and `UNKNOWN` select no branch under the three-valued logic of
[FORMULA-4](formulas.md#formula-4).

<a id="expr-8"></a>

### EXPR-8. flag returns a declared value for each of TRUE, FALSE and UNKNOWN

`flag` evaluates its `condition` predicate: only `TRUE` selects `true_value`,
which defaults to `"Y"`; `FALSE` returns `false_value`, or missing when it is
absent; `UNKNOWN` returns `missing`, or missing when it is absent. An unknown
condition never falls through to `false_value`. A `flag` that declares
`false_value` also declares `missing`, which may repeat `false_value`, name
another value, or be `{literal: null}` for missing.

Fails: `missing_value_required`.
Evidence: `negative-flag-missing-value`, `negative-flag-condition`, `adam-adae-occurrence-flags`.
Replaces: REQ-1256, REQ-1257, REQ-1258.

`flag` is the shorthand for the common one-branch `case` that returns a flag
value. A bare predicate string is the condition with the default values:
`true_value: "Y"`, and `false_value` and `missing` missing. The condition
follows the three-valued logic [FORMULA-4](formulas.md#formula-4) gives
`case`; unlike `case` with `otherwise`, `UNKNOWN` does not reach
`false_value`.

A `flag` that declares `false_value` without `missing` fails as
`missing_value_required` at `missing`. A `flag` whose payload is neither a
predicate string nor a mapping, whose mapping form lacks `condition` or names a
non-predicate `condition`, or whose condition is not a valid predicate fails.

<a id="expr-9"></a>

### EXPR-9. first_available returns the first source that is not missing

`first_available` tests its `sources` in order and returns the value of the
first source that is not missing. When every source is missing, it returns the
declared `missing`, or missing when none is declared.

Evidence: `schema-expressions`, `adam-adsl-demographics`, `rust/crates/yamaa-adapters/tests/fixtures/first_available.json`.
Replaces: REQ-1095.

<a id="expr-10"></a>

### EXPR-10. greatest and least return the largest or smallest non-missing source

`greatest` returns the largest non-missing value among its `sources`, and
`least` the smallest; each returns missing when no source has a value. The
sources must be mutually comparable.

Evidence: `schema-expressions`, `negative-greatest-mixed`, `negative-least-mixed`.
Replaces: REQ-0310, REQ-1096, REQ-1097.

[VALUE-19](values.md#value-19) checks that comparability and states
which source an equal extreme keeps; `int` and `float` compare exactly under
[VALUE-19](values.md#value-19).

## Classification

<a id="expr-11"></a>

### EXPR-11. mapping replaces a string source with its dictionary value

`mapping` looks up a string `source` in `dict`, a source-value to result-value
dictionary written inline or as the path of a YAML file holding it, and
returns the matching entry's value. A missing source returns `missing`, and a
present source with no dictionary entry returns `unmapped`; `missing` never
answers an unmapped source. Without the applicable handler, the source fails
([COLUMN-10](columns.md#column-10)).

Evidence: `schema-text-mapping`, `schema-text-mapping-unmapped`, `negative-mapping-unmapped-value`.
Replaces: REQ-0304, REQ-1110.

A `mapping` requires a string source. The source may be a filtered source
carrying the variable with its filter, selection or absent value
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
Replaces: REQ-0709, REQ-0710, REQ-0714.

Keys that collide after the fold fail validation with `ambiguous_dictionary`,
reporting the folded key and the original entries. A non-ASCII value can match
only the same non-ASCII scalar sequence, apart from ASCII letters that also
occur in that sequence.

<a id="expr-13"></a>

### EXPR-13. cut assigns a numeric source to one of its labeled break intervals

`cut` assigns a numeric `source` to one of the intervals its ascending
`breaks` bound and returns that interval's label. `labels` holds one label per
interval, exactly one more than `breaks`. With `right: true` the intervals are
right-closed and left-open; with `right: false`, the default, they are
left-closed and right-open. A missing source returns `missing`, and a
non-numeric source fails.

Evidence: `schema-expressions`, `negative-cut-coded`, `negative-cut-missing-input`.
Replaces: REQ-0306, REQ-1118.

The source may be a filtered source carrying the variable with its filter,
selection or absent value ([LOOKUP-1](lookup.md#lookup-1)). Breaks compare
mathematical values exactly under [VALUE-19](values.md#value-19).
Without `missing`, a missing source fails under
[COLUMN-10](columns.md#column-10).
