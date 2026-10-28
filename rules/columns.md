# Column derivation

A run builds rows, then derives columns in dependency order. Every derived
value completes its lifecycle before anything reads it, and local handlers
answer named conditions at the site where they occur.

## Run order

<a id="column-1"></a>

### COLUMN-1. A run validates, builds rows, derives columns, verifies, orders and publishes, in that order

A run resolves and validates the specification and resources, constructs rows,
derives columns in dependency order, completes column verification, validates
output keys, runs dataset verification, orders the artifact, then serializes
and publishes it. A value completes its lifecycle before a dependent reads it.
Row construction evaluates the `rows` entries and may change the row count.

Evidence: `schema-lifecycle`.

Row construction under [Row construction](rows.md) comes before column
derivation under [COLUMN-2](#column-2).

<a id="column-2"></a>

### COLUMN-2. A column derivation yields exactly one value per row and never changes the row count

Column derivation enriches constructed rows and must not change the row count:
a column derivation must yield exactly one value per row. It counts values
rather than the records that carry them. Repeated readings of one value are
that one value; two input records of one key combination that carry different
present values are two values and fail. A missing result is still the row's one
value but never counts as a second value.

Fails: `multiple_values_per_key`.
Evidence: `negative-keys-conflict`, `negative-sex-duplicate`.

A source `filter` decides which of those records the derivation reads before
that count, as [LOOKUP-8](lookup.md#lookup-8) defines. A source
declaring `order_by` and `keep` keeps one of the records carrying those values
instead of failing, as [COLUMN-12](#column-12) defines. A column derivation
yielding more than one value for one key combination fails as
`multiple_values_per_key` and reports the column, how many values it yielded,
and the keys; missing results are excluded from the count. In a specification
without `rows`, key-column derivation must also satisfy [COLUMN-7](#column-7).

## Dependencies

<a id="column-3"></a>

### COLUMN-3. Dependencies come from every reference an expression makes, closed grammars included

Dependencies are inferred to validate declaration order and detect cycles:
each expression is traversed recursively, collecting the references the table
below lists. Identifier extraction parses each predicate, numeric expression,
string template and reducer expression under its grammar; none of the four is
treated as dependency-free.

Evidence: `rust/crates/yamaa-core/tests/schema_references.rs::closed_languages_retain_field_and_record_count_dependencies`.

| Site | Collected references |
|---|---|
| `source` | every unqualified output variable it references |
| a named lookup read by a qualified reference | its match values and `between.value` variable, under [Lookup and joins](lookup.md) |
| an implicit join, or a qualified aggregate without `key` | the columns of its input's declared `key` ([LOOKUP-1](lookup.md#lookup-1), [LOOKUP-5](lookup.md#lookup-5)) |
| `group_by`, `order_by` and other fields typed `variable` | their variables |
| `function.args` | the variables passed as leaves |
| fields whose type contains a nested `expression` | the variables they reference |
| a `predicate` field | the constructed output identifiers it uses |
| a `numeric_expression` | the constructed output identifiers it uses |
| an `aggregate_expression` | the identifiers it uses |
| a `string_template` | the variables used as placeholders |

Predicates include `case` items' `when`, `row.filter`, aggregate `filter` and
window `filter`. An ungrouped `row.filter` reads the candidate row's derived
columns and lookup state under [ROW-6](rows.md#row-6), after the non-window
derivations complete and before the window pass. A grouped `row.filter`
resolves unqualified columns derived by that row template and runs after that
graph completes. Both sites read values after stages 1 to 3, as
[COLUMN-9](#column-9) defines. The four grammars are those of
[Formulas](formulas.md),
[Formulas](formulas.md),
[Text](text.md) and
[Formulas](formulas.md).

<a id="column-4"></a>

### COLUMN-4. Row derivations follow their template's dependency graph and never read a value only column derivation produces

For each row template, row derivations evaluate using a dependency graph. A
row derivation cannot depend on a value produced only during column derivation.
Every unqualified identifier in a grouped `row.filter` must resolve to a column
derived by that row template. The grouped `filter` is not a derivation: it adds
no graph edge between columns and runs only after all columns have completed.

Fails: `phase_boundary`.
Evidence: `negative-row-phase-flag`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/rows.json::row_reads_of_nonlocal_columns_return_complete_phase_findings_before_lowering`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_row_phase_boundaries.json::an_ungrouped_template_reading_a_case_result_aggregate_default_is_a_phase_boundary`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_row_phase_boundaries.json::a_grouped_template_reading_a_case_result_aggregate_default_is_a_phase_boundary`.

A row dependency on such a value fails as `phase_boundary`.

<a id="column-5"></a>

### COLUMN-5. Columns evaluate in declaration order, each depending only on columns declared earlier

After row construction, the column dependency graph is built. Every dependency
must refer to a column declared earlier, and columns evaluate in declaration
order. Composition orders the resolved columns
([LAYER-13](inheritance.md#layer-13)) only when a specification declares a
`parent` or includes a fragment; otherwise declaration order is the resolved
order.

Fails: `unknown_reference`, `forward_reference`.
Evidence: `negative-forward-reference`, `negative-forward-key-reference`, `rust/crates/yamaa-core/tests/schema_references.rs::unknown_dependencies_are_lexical_and_precede_cycle_reporting`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/column_driver_bindings.json::column_bindings_run_one_driver_after_another`.

[Inheritance](inheritance.md) composes and prunes every specification before
these rules apply: a specification that declares no `parent` and includes no
fragment is a composition of one layer and is pruned on the same terms as a
composition of several.
An unresolved variable or predicate reference fails. A reference to a later
declared column fails as `forward_reference` and reports both columns.

A column derivation completes the rows of the row templates that leave the
column to it, each row from the record that built it
([NAME-4](names.md#name-4)). When those templates have different drivers,
the column evaluates once for each driver, in the order of the first of those
templates each drives, each time over that driver's rows in row order. A
column holding a qualified aggregate ([LOOKUP-18](lookup.md#lookup-18))
evaluates once, over all of their rows in row order.

<a id="column-6"></a>

### COLUMN-6. A column that reaches its own value, directly or through another row, is a dependency cycle

The column dependency graph is over columns, not over rows: a column reading
another row in its own window partition depends on the whole named column. A
column that reaches its own value through another row is a cycle rather than an
iteration, and a dependency cycle fails.

Fails: `dependency_cycle`.
Evidence: `negative-adlb-computed-param`, `negative-first-available-self`, `negative-row-value-self`.

`locf` with `include_current: false` crosses any number of missing rows by
searching a separate completed source column; with the default
`include_current: true` it returns the current source when present and
otherwise performs that search. Searching the column being derived remains a
cycle rather than an instruction to iterate. A cycle fails as
`dependency_cycle` and reports the cycle path.

<a id="column-7"></a>

### COLUMN-7. Without row templates, a key is computed from one base record and never from a non-key output column

In a specification without `rows`, keys are derived before any row logic runs,
each from one base record. A key column derivation that depends on a non-key
output column fails, and so does one that reads records other than its own,
an aggregate or an `odm` read, unless a key derived by a window makes each
base record its own row.

Fails: `key_dependency`.
Evidence: `negative-keys-missing`, `negative-keys-aggregate`, `negative-keys-odm`, `schema-keys-record-reads`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/row_scopes.json`.

A key reads its base record's fields, the keys declared before it, and a
named intermediate's selection for that record, through any operation that
reads one record, a formula, a flag, a `case` condition and a project
function argument among them. An aggregate reduces the records of a key
combination and an `odm` read reads its items, but neither exists before the
keys do: either fails as `key_dependency` at the operation, also inside a
`case` branch, naming the key column and the operation. When a key is a
window, each base record is its own key combination
([ROW-8](rows.md#row-8)), and a key is derived for that record's row as any
column is.

## Value lifecycle

<a id="column-8"></a>

### COLUMN-8. Every derived value is evaluated, converted and handled before anything reads it, then verified

Every derived value passes through four stages in order, and nothing consumes
a value before its lifecycle completes. Stage 1 evaluates the derivation's
expression for one value; stage 2 converts the result to the column's declared
`type`; stage 3 applies conversion-failure handling; stage 4 runs the column's
verifications over the whole column. Dependent columns, verifications and the
artifact see the same converted value.

Evidence: `schema-lifecycle`, `rust/crates/yamaa-core/tests/execution_numeric_lifecycle.rs::dependent_reads_converted_handler_value`.

Stages 1 to 3 run on each value, in whichever phase its derivation belongs to.
Stage 4 runs once, after every row holds that column's final value. A
row-level derivation therefore completes stages 1 to 3 during row
construction, and a column derivation that depends on it reads a converted
value of the declared type. [VALUE-2](values.md#value-2) permits no
implicit conversion between operation inputs, and an operation consuming a
row-derived column must rely on the column's declared type.

1. Evaluate the expression under
   [Expressions](expressions.md), with the local handlers
   of [COLUMN-10](#column-10).
2. Convert the result under [Values](values.md).
3. Apply conversion-failure handling under [COLUMN-13](#column-13).
4. Run the column's verifications under [Verification](verification.md). An
   error stops execution; warnings accumulate without changing the column.

A failed error-level verification fails under [Verification](verification.md).
A warning-level violation leaves the primary artifact intact and is recorded
in the run report of [Verification](verification.md).

<a id="column-9"></a>

### COLUMN-9. A row filter reads candidate values after stages 1 to 3 and never hides a failed derivation

A row template's `filter` evaluates after stages 1 to 3 complete for every
value the filter may read on the candidate row. A discarded candidate never
enters the completed dataset, and stage 4 column verifications do not include
it. An error reached while deriving the candidate still fails the run; the
filter does not retroactively hide a failed derivation.

Evidence: `schema-lifecycle`, `rust/crates/yamaa-application/tests/fixtures/row_filters.json`.

For a grouped template, the values the filter may read are those of every row
derivation. For an ungrouped template, they are those of every derivation that
does not read a window result, evaluated before the window pass.

## Local handlers

<a id="column-10"></a>

### COLUMN-10. A local handler answers its condition with a literal, and an omitted handler gives missing for a missing single input or absent record and leaves every other condition fatal

Each local handler applies at the site the table below lists, only when its
condition occurs, and substitutes the constant it declares, written as a
value field: text, a number or a Boolean, or YAML null for missing, never an
expression. An omitted handler for the missing input of a single-input verb,
a record a lookup does not find, an `UNKNOWN` flag condition or
`first_available` sources all missing gives missing, counted as that
handler. Every other condition fails without its handler, a missing
`str_template` placeholder among them.

Fails: `missing_input`, `unmapped_value`, `unmatched_pattern`.
Evidence: `schema-handlers`, `schema-missing-input-default`, `schema-lookup-absent-default`, `negative-mapping-unmapped-value`, `negative-str-template-missing-placeholder`, `negative-str-extract-no-match`.

These sites identify conditions within expression evaluation or result
handling; the ordered derivation lifecycle is [COLUMN-8](#column-8) and
[COLUMN-9](#column-9). Each handler uses a literal unless its behavior says
otherwise, and no handler adds a dependency.

A handler's value is a value field: a string, integer, float or Boolean,
written bare, or YAML null, the missing value. YAML null never removes a
handler: `unmapped: null` declares a handler that answers missing, where
omitting it leaves the condition fatal, and a child layer clears an
inherited handler only by listing its path in `clear`
([LAYER-9](inheritance.md#layer-9)). The same values are those of a `flag`'s
`true_value`, `false_value` and `missing`, of `first_available`'s `missing`
and of an inline `mapping` dictionary entry, and a `function` argument passes
the missing value as YAML null. A handler that restates its default is the
lint finding `restated_default`, never a refusal.

The table states, for each handler, the phase in which its condition arises
([COLUMN-16](#column-16)), the condition, and what the condition gives when
the handler is omitted.

| Phase | Handler | Condition | Omitted |
|---|---|---|---|
| join | `source.absent` | A read through the implicit join finds no record ([LOOKUP-24](lookup.md#lookup-24)) | Missing |
| join | `source.order_by` and `source.keep` | Several `source.filter` results | Fails |
| join | `intermediates[].absent` | The intermediate selects no record ([LOOKUP-24](lookup.md#lookup-24)) | Missing |
| mapping | `mapping.missing` | Missing input | Missing |
| mapping | `mapping.unmapped` | Non-missing value with no mapping | Fails |
| mapping | `decode.missing` | Missing coded input ([EXPR-15](expressions.md#expr-15)) | Missing |
| mapping | `decode.unmapped` | Non-missing value no item of the codelist lists | Fails |
| mapping | `cut.missing` | Missing numeric input | Missing |
| mapping | `missing` of `str_extract`, `str_replace`, `str_case`, `str_substr` and `str_length` | Missing string input | Missing |
| mapping | `str_decimal.missing` | Missing numeric input | Missing |
| mapping | `str_extract.no_match` | Non-missing string does not match | Fails |
| mapping | `str_template.missing` | Any placeholder value is missing | Fails |
| derivation | `flag.missing` | The condition is `UNKNOWN` ([EXPR-8](expressions.md#expr-8)) | Missing |
| derivation | `first_available.missing` | Every source is missing ([EXPR-9](expressions.md#expr-9)) | Missing |
| impute | `impute.invalid`, `precision.invalid` | Text that is not collected date text, an ISO 8601 interval or duration included ([TIME-13](temporal.md#time-13)) | Fails |
| impute | `dtc.invalid` | A date part that is not its field's digits or lies outside its range ([TIME-19](temporal.md#time-19)) | Fails |
| impute | `date_text.invalid` | Text in none of the declared date forms ([TIME-22](temporal.md#time-22)) | Fails |
| impute | `duration_value.invalid` | Invalid fixed-length duration text ([TIME-17](temporal.md#time-17)) | Fails |
| convert | `unconvertible` | Failed output conversion | Fails |

An omitted handler that gives missing is a default substitution: the value is
the missing value, and it is counted under the handler's path as
a declared substitution is ([COLUMN-14](#column-14)). `impute` and `precision`
declare no `missing`: a missing source gives missing, counted at their
`missing` path, unless `impute` declares `fallback`
([TIME-13](temporal.md#time-13)). A read through an implicit join that is not
a source binding declares no `absent` and takes its default
([LOOKUP-24](lookup.md#lookup-24)). `require_match: true` on a named
intermediate, a source binding or an input turns the absence answer into an
`unmatched_key` failure. A handler field on an expression that does not
register it is a schema failure. A handler literal incompatible with its result
context fails, reporting both the handler and the original context.

<a id="column-11"></a>

### COLUMN-11. `absent` answers a lookup that finds no record, and `missing` answers a missing input value

`absent` on a `source` binding or a named intermediate applies when its lookup
finds no record, never when a record exists and holds a missing value; a
variable that does not exist fails under [NAME-3](names.md#name-3). `missing`
applies when the named input value is missing, on every expression that
declares it; where an operation has several inputs, such as a `str_template`
with several placeholders, it fires when any input is missing. `invalid`,
`unmapped` and `str_extract`'s `no_match` fire only when every input is
present.

Evidence: `rust/crates/yamaa-core/src/specification/source_expressions/mod.rs::absent_does_not_answer_a_variable_that_does_not_exist`, `rust/crates/yamaa/tests/language_header.rs::absent_does_not_answer_a_variable_the_input_does_not_hold`, `rust/crates/yamaa-application/tests/fixtures/original_source_bindings.json`, `rust/crates/yamaa-application/tests/fixtures/original_mapping_cut.json`, `schema-handlers`.

On `mapping`, `missing` never answers a non-missing input with no dictionary
entry; that is `unmapped`'s condition
([EXPR-11](expressions.md#expr-11)). An intermediate's `absent` also
answers a missing match value ([LOOKUP-24](lookup.md#lookup-24)), as a source
binding's `absent` answers one on its implicit join
([LOOKUP-24](lookup.md#lookup-24)). Omitted, each of these handlers gives the
default of [COLUMN-10](#column-10). An `odm` read declares no `absent`, and
one that identifies no record gives missing ([NAME-13](names.md#name-13)).

The owning rule states which values an operation cannot use;
[Dates and times](temporal.md) states them for the temporal operations in the
`impute` phase. One phase can serve several operations with the same
conditions. `impute` and `precision` read the same source domain, answer the
same invalid condition, and both use `impute` in structured errors; a missing
source to either gives missing without a handler
([TIME-13](temporal.md#time-13)).

An aggregate declares no handler at all. A variable it names that does not
exist is [Name binding](names.md)'s unresolved reference. A right side that
reduces to no matching record yields missing, and no handler counts it. A
group whose records all hold missing values is neither condition;
[Formulas](formulas.md) states what each reducer returns for such a group.

<a id="column-12"></a>

### COLUMN-12. A structured source carries its own handlers, and order_by with keep selects one of several records

Under the `source` expression, only structured source binding declares
handlers; a bare reference such as `DATASET.VARIABLE` declares none and takes
their defaults. A source's
`order_by` and `keep` relax right-side uniqueness wherever one source reaches
several records: its optional `filter` narrows the right-side records it
matches ([LOOKUP-8](lookup.md#lookup-8)), the survivors sort by the
`order_by` terms, `keep` retains the `first` or `last`,
and right-side record order resolves remaining ties. An empty filtered result
is an absent match, answered as [LOOKUP-24](lookup.md#lookup-24) states.

Evidence: `schema-handlers`, `rust/crates/yamaa-application/tests/fixtures/source_selection.json`, `rust/crates/yamaa-application/tests/fixtures/original_source_bindings.json`.

```yaml
source:
  variable: RAW.AGE
  absent: null
```

Other expressions type `source` as a variable or a variable with a `filter`,
and declare their handler fields beside `source`. Binding handlers belong to
the source. The source reaches records through lookup and joins, and uses its
own handlers after it holds a value.

The several records are [Lookup and joins](lookup.md)'s matched
records and the records a key combination was derived from. Without `order_by`
and `keep`, several matched records fail as `multiple_matches` under
[LOOKUP-10](lookup.md#lookup-10), and disagreement among the records a
key combination was derived from fails under [COLUMN-2](#column-2). The absent
match follows [LOOKUP-24](lookup.md#lookup-24): `absent` answers it, missing
by default, and `require_match: true` fails it. `source.keep` outside `first`
or `last` is a schema failure.

<a id="column-13"></a>

### COLUMN-13. A result wrapper's unconvertible replaces only a failed conversion to the column type

A derivation with conversion handling uses `value` for its normal expression;
a bare expression is the [Schema language](reference/schema-language.md)
shorthand for that wrapper, and after expansion every derivation carries its
expression in `value`. `unconvertible` on the wrapper supplies a literal
replacement only when conversion to the declared column type fails, and the
replacement converts to that same type. Without `unconvertible`, a failed
output conversion is fatal.

Fails: `conversion_failed`.
Evidence: `schema-handlers`, `rust/crates/yamaa-application/tests/fixtures/original_conversion_handlers.json`, `rust/crates/yamaa-application/tests/fixtures/original_row_conversion_handlers.json`.

A derivation that needs stage 3 wraps its expression in `value`:

```yaml
derivation:
  value: RAW.AGE
  unconvertible: null
```

The wrapper is written only beside `unconvertible`: a wrapper holding `value`
alone restates the bare expression and fails as `redundant_long_form` under
[SCHEMA-15](reference/schema-language.md#schema-15).

[Values](values.md) defines which conversions fail and
states that a missing input is not converted at all, so `unconvertible` never
fires for one. A conversion replacement that cannot be converted fails. A
derivation declaring `unconvertible` is not refused under
[VALUE-24](values.md#value-24) when its result type can never convert: the
literal answers every value.

<a id="column-14"></a>

### COLUMN-14. Every handler path reports how many records it answered as a report row, default substitutions and zero included

Each handler path's record count is a `handler` row of the run report under
[CHECK-23](verification.md#check-23), for a failed run as well as a successful
one, counting the declared handler's substitutions and the default
substitutions an omitted handler gives under the same path. A handler firing
zero times is reportable and is not an error. The count for a source's
`order_by` and `keep` reports only records where more than one match survived
the filter.

Evidence: `rust/crates/yamaa-application/tests/fixtures/original_source_bindings.json`, `rust/crates/yamaa-application/tests/fixtures/original_mapping_cut.json`, `rust/crates/yamaa-application/tests/fixtures/flag_reference.json`, `rust/crates/yamaa-core/tests/execution_numeric_lifecycle.rs::accounting_retains_zero_declarations_and_prior_firings_on_failure`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/run_report.json::handler_rows_register_every_path_at_zero_in_one_order`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/run_report.json::intermediate_derivation_handlers_count_each_build_of_the_donor_records`, `rust/crates/yamaa-core/src/dataset/resolved/tests.rs::column_bindings_register_their_sites_one_binding_after_another`.

A default substitution is counted where the handler it stands for would be: at
`<operation>.missing` for `mapping`, `decode`, `cut`, `str_extract`,
`str_replace`, `str_case`, `str_substr`, `str_length`, `str_decimal`, `flag`,
`first_available`, `impute` and `precision`; and at `<path>.absent` for a read
through an implicit join or of a named intermediate, at the source binding or
operand that makes the read ([LOOKUP-24](lookup.md#lookup-24),
[LOOKUP-14](lookup.md#lookup-14)). A lookup read made while a predicate is
evaluated is not a handler site: its absence gives missing, uncounted. Nor is a
read of a named intermediate that supplies another lookup's `between` value,
filter field or derivation's current-record operand: it answers as in a
predicate, uncounted, and only the lookup it feeds counts its own answer. So
one record is never counted twice at one path, a read made while a predicate is
evaluated counts nothing at any path, including the lookups it reads through,
and a count does not depend on which read of an intermediate first selected a
record for the row.

A run registers every handler path before it reads any row, so a run reports
a path even when it answers nothing. That covers every declared handler and
every default substitution counted like one, a `partial` date and
`not_after` included; the `multiple_matches` of each `keep`, on a source
binding, a collected value or a named intermediate read, and of each named
intermediate read that a `nearest` selection chooses for, which counts only
records where more than one candidate had a distance
([LOOKUP-26](lookup.md#lookup-26)); and each named intermediate read's
`absent` at its reading path. A read that requires a match registers no
`absent`.

Handler rows come in registration order. The dataset's own paths come first:
the `unconvertible` paths of the output derivations, in declaration order with
the row templates' before the columns'; then each derivation in the order the
run evaluates it, the row templates' derivations template by template, then
the column derivations, each as often as it evaluates and in that order
([COLUMN-5](#column-5)); then the intermediates' key expressions. Each
derivation lists its other paths in expression order: an operation's own paths
before those of the expressions it reads, which follow in the order the
operation reads them. An operation lists its own paths as `missing` or
`fallback`, `unmapped`, `invalid`, `not_after` and `partial`, then a lookup's
`multiple_matches` before its `absent`. Then each intermediate
with derivations follows in turn, its paths in the same order: the
`unconvertible` paths of its typed derivations, each derivation in declaration
order, then the key expressions of the intermediates those derivations read,
as bound for the donor records. A path is registered once, where this order
first reaches it.

A path inside an intermediate's `derivations:`
([LOOKUP-15](lookup.md#lookup-15)) counts the donor records it answers each
time the run builds those records, so its count follows how often they are
built, not the output rows. The run builds them at most once, when the
intermediate is first read, and again: for each current record when a
derivation reads the current input record; after each row template for a
`SELF` intermediate, whose completed rows grow
([LOOKUP-7](lookup.md#lookup-7)); for column derivation when the
intermediate's key, or the key of an intermediate its current-record operands
read, depends on a window over the completed rows; and inside each build of
another intermediate whose derivation reads it
([LOOKUP-16](lookup.md#lookup-16)). The key expression of an intermediate that
another intermediate's derivation reads also counts the evaluations made for
those donor records.

## Diagnostics

<a id="column-15"></a>

### COLUMN-15. A language diagnostic cites the rule that owns its failure; only listed implementation conditions cite none

A language diagnostic cites the rule that owns its failure. A missing rule is
permitted only for the implementation conditions listed below, in their stated
phases. These conditions report an admitted operation that the implementation
does not support or an engine refusal; neither asserts a language rule
violation.

Fails: `unsupported_operation`, `engine_rejected`.
Evidence: `tools/tests/test_diagnostics.py::test_null_policy_is_explicit_registered_and_reached`.

<!-- normative-null-conditions:start -->
- `unsupported_operation` (`planning`)
- `engine_rejected` (`engine`)
<!-- normative-null-conditions:end -->

<a id="column-16"></a>

### COLUMN-16. Every condition reports one phase from a closed list, which names the part of the run that decides it

A condition reports one phase: `validation`, `planning`, `ingest`,
`row_construction`, `join`, `mapping`, `impute`, `derivation`, `convert`,
`verification`, `output` or `engine`, as the table below defines.
`yaml/conditions.yaml` registers the phases each condition may report, and
reporting any other is a defect. The numbered stages of a value
([COLUMN-8](#column-8)) are not phases: stage 1 reports the phase of what it
evaluates, stages 2 and 3 report `convert`, and stage 4 reports
`verification`.

Evidence: `negative-adlb-duplicate-wbc`, `negative-multiple-baselines`, `tools/tests/test_diagnostics.py::test_missing_condition_and_wrong_phase_fail`, `tools/tests/test_validate_repository.py::test_wrong_phase_condition_is_rejected`.

| Phase | Decided while |
| --- | --- |
| `validation` | checking the files and each input's stored fields, before any input record is read (P9) |
| `planning` | lowering a valid specification to a plan (`unsupported_operation`) |
| `ingest` | reading an input snapshot into typed records |
| `row_construction` | building output rows ([Row construction](rows.md)) |
| `join` | a lookup's match, narrow and select steps ([LOOKUP-25](lookup.md#lookup-25)) |
| `mapping` | the value-mapping verbs `mapping`, `decode`, `cut`, `str_extract` and `str_template` |
| `impute` | completing collected dates and times ([Dates and times](temporal.md)) |
| `derivation` | evaluating any other expression of a value |
| `convert` | converting a completed result to its column type, and handling a failed conversion ([COLUMN-8](#column-8), stages 2 and 3) |
| `verification` | column and dataset verifications ([Verification](verification.md)), an intermediate's `unique` ([LOOKUP-17](lookup.md#lookup-17)), and the checks that submission metadata and codelists imply |
| `output` | validating keys, rendering and publishing ([Output](output.md)) |
| `engine` | an application refusal outside the language (`engine_rejected`) |

A condition registered for several phases reports the one in which the run
decides it: `aggregate_multiple_records` reports `row_construction` for a row
template's derivation and `derivation` for a column derivation.
