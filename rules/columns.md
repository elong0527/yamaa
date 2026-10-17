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
Replaces: REQ-0031, REQ-0032.

Derivation has two phases: row construction under
[Row construction](rows.md), then column derivation under
[COLUMN-2](#column-2).

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
Replaces: REQ-0033, REQ-0044, REQ-0075.

A source `filter` decides which of those records the derivation reads before
that count, as [LOOKUP-13](lookup.md#lookup-13) defines. A source
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
Replaces: REQ-0048, REQ-0049, REQ-0050, REQ-0051, REQ-0052, REQ-0053, REQ-0054,
REQ-0055, REQ-0056, REQ-0057, REQ-0058.

| Site | Collected references |
|---|---|
| `source` | every unqualified output variable it references |
| a named lookup read by a qualified reference | its match values and `between.value` variable, or the applicable output keys when matching fields are omitted, under [Lookup and joins](lookup.md) |
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

### COLUMN-4. Row derivations follow their template's dependency graph and never read a column-phase value

For each row template, row derivations evaluate using a dependency graph. A
row derivation cannot depend on a value produced only during the column phase.
Every unqualified identifier in a grouped `row.filter` must resolve to a column
derived by that row template. The grouped `filter` is not a derivation: it adds
no graph edge between columns and runs only after all columns have completed.

Fails: `phase_boundary`.
Evidence: `rust/crates/yamaa-core/tests/project_specification.rs::row_reads_of_nonlocal_columns_return_complete_phase_findings_before_lowering`.
Replaces: REQ-0059, REQ-0069.

A row dependency on a later-phase value fails as `phase_boundary`.

<a id="column-5"></a>

### COLUMN-5. Columns evaluate in declaration order, each depending only on columns declared earlier

After row construction, the column dependency graph is built. Every dependency
must refer to a column declared earlier, and columns evaluate in declaration
order. Composition orders the resolved columns only when a specification
declares a `parent`; otherwise declaration order is the resolved order.

Fails: `unknown_reference`, `forward_reference`.
Evidence: `negative-forward-reference`, `rust/crates/yamaa-core/tests/schema_references.rs::unknown_dependencies_are_lexical_and_precede_cycle_reporting`.
Replaces: REQ-0060, REQ-0070, REQ-0071.

[Inheritance](inheritance.md) composes and prunes every specification before
these rules apply: a specification that declares no `parent` is a composition
of one layer and is pruned on the same terms as one that declares a `parent`.
An unresolved variable or predicate reference fails. A reference to a later
declared column fails as `forward_reference` and reports both columns.

<a id="column-6"></a>

### COLUMN-6. A column that reaches its own value, directly or through another row, is a dependency cycle

The column dependency graph is over columns, not over rows: a column reading
another row in its own window partition depends on the whole named column. A
column that reaches its own value through another row is a cycle rather than an
iteration, and a dependency cycle fails.

Fails: `dependency_cycle`.
Evidence: `negative-adlb-computed-param`, `negative-first-available-self`, `negative-row-value-self`.
Replaces: REQ-0061, REQ-0072.

`locf` with `include_current: false` crosses any number of missing rows by
searching a separate completed source column; with the default
`include_current: true` it returns the current source when present and
otherwise performs that search. Searching the column being derived remains a
cycle rather than an instruction to iterate. A cycle fails as
`dependency_cycle` and reports the cycle path.

<a id="column-7"></a>

### COLUMN-7. Without row templates, a key column derivation must not depend on a non-key output column

In a specification without `rows`, keys are derived before any row logic runs,
and a key column derivation that depends on a non-key output column fails.

Fails: `key_dependency`.
Evidence: `negative-keys-missing`.
Replaces: REQ-0074.

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
Replaces: REQ-0211, REQ-0212, REQ-0213, REQ-0214, REQ-0215, REQ-0216, REQ-0217,
REQ-0241.

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

Evidence: `schema-lifecycle`, `rust/crates/yamaa-adapters/tests/fixtures/row_filters.json`.
Replaces: REQ-0218.

For a grouped template, the values the filter may read are those of every row
derivation. For an ungrouped template, they are those of every derivation that
does not read a window result, evaluated before the window pass.

## Local handlers

<a id="column-10"></a>

### COLUMN-10. A local handler answers its condition with a literal or `{literal: null}`, and an omitted handler leaves it fatal

Each local handler applies at the site the table below lists and is
substituted only when its condition occurs. Every handler value is a scalar
literal, or `{literal: null}`, which answers with missing; handlers are never
expressions and add no dependencies. Omitting an applicable handler field, or
writing it as YAML null, leaves its condition fatal at every site. A condition
that registers no handler is answered by its owning rule.

Fails: `missing_input`, `unmapped_value`, `unmatched_pattern`.
Evidence: `schema-handlers`, `negative-cut-missing-input`, `negative-mapping-unmapped-value`.
Replaces: REQ-0334, REQ-0342, REQ-0343, REQ-0344, REQ-0360, REQ-0362, REQ-0364.

These sites identify conditions within expression evaluation or result
handling; the ordered derivation lifecycle is [COLUMN-8](#column-8) and
[COLUMN-9](#column-9). Each handler uses a literal unless its behavior says
otherwise.

A handler's literal is a string, integer, float or Boolean, written bare. The
missing value has one spelling, `{literal: null}`, the same as the literal
expression: a YAML null leaves the field absent under
[LAYER-9](inheritance.md#layer-9), so `unmapped: null` declares no handler.
The same two forms are the values of a `flag`'s `true_value`, `false_value`
and `missing`, of `first_available`'s `missing` and of an inline `mapping`
dictionary entry, and a `function` argument passes the missing value as
`{literal: null}`.

| Stage | Local declaration | Behavior |
|---|---|---|
| bind | `source.absent` | Absent source variable or ODM item |
| join | `source.order_by` and `source.keep` | Choose one `source.filter` result |
| join | `intermediates[].no_match` | The intermediate selects no record ([LOOKUP-13](lookup.md#lookup-13)) |
| mapping | `missing` | Missing input |
| mapping | `unmapped` | Non-missing value with no mapping |
| cut | `missing` | Missing numeric input |
| extract | `missing` | Missing string input |
| extract | `no_match` | Non-missing string does not match |
| template | `missing` | Any placeholder value is missing |
| impute | `impute.invalid`, `precision.invalid` | Text that is not collected date text ([TIME-13](temporal.md#time-13)) |
| impute | `duration_value.invalid` | Invalid fixed-length duration text ([TIME-17](temporal.md#time-17)) |
| convert | `unconvertible` | Failed output conversion |

An implicit join that finds no record, for example, yields missing under
[LOOKUP-1](lookup.md#lookup-1). An unhandled local absent, missing,
mapping, or extraction condition fails. A handler field on an expression that
does not register it is a schema failure. A handler literal incompatible with
its result context fails, reporting both the handler and the original context.

<a id="column-11"></a>

### COLUMN-11. `absent` answers a variable that does not exist, and `missing` answers a missing input value

`absent` on a `source` binding applies when the variable or ODM item does not
exist in context, never when it exists and holds a missing value. `missing`
applies when the named input value is missing, on every expression that
declares it; where an operation has several inputs, such as a `str_template`
with several placeholders, it fires when any input is missing. `invalid`,
`unmapped` and `str_extract`'s `no_match` fire only when every input is
present.

Evidence: `rust/crates/yamaa-adapters/tests/fixtures/original_source_bindings.json`, `rust/crates/yamaa-adapters/tests/fixtures/original_mapping_cut.json`, `schema-handlers`.
Replaces: REQ-0345, REQ-0346, REQ-0347, REQ-0348, REQ-0349, REQ-0350, REQ-0357.

On `mapping`, `missing` never answers a non-missing input with no dictionary
entry; that is `unmapped`'s condition
([EXPR-11](expressions.md#expr-11)). An intermediate's `no_match` also
answers a missing match value ([LOOKUP-13](lookup.md#lookup-13)).

The owning rule states which values an operation cannot use;
[Dates and times](temporal.md) states them for the temporal operations on the
`impute` stage. One stage name can serve several operations with the same
conditions. `impute` and `precision` read the same source domain, answer the
same invalid condition, and both use `impute` in structured errors; a missing
source to either gives missing without a handler
([TIME-13](temporal.md#time-13)).

An aggregate declares no handler at all. A variable it names that does not
exist is [Name binding](names.md)'s unresolved reference. A
right side that reduces to no matching record is
[Lookup and joins](lookup.md)'s absent match. A group whose records
all hold missing values is neither condition;
[Formulas](formulas.md) states what each reducer returns for
such a group.

<a id="column-12"></a>

### COLUMN-12. A structured source carries its own handlers, and order_by with keep selects one of several records

Under the `source` expression, only structured source binding carries
handlers; the concise `source: DATASET.VARIABLE` form has none. A source's
`order_by` and `keep` relax right-side uniqueness wherever one source reaches
several records: its optional `filter` selects the eligible right-side
records, the survivors sort by the `order_by` terms, `keep` retains the
`first` or `last`,
and right-side record order resolves remaining ties. An empty filtered result
is not a handled condition: it is an absent match and yields missing.

Evidence: `schema-handlers`, `rust/crates/yamaa-adapters/tests/fixtures/source_selection.json`, `rust/crates/yamaa-adapters/tests/fixtures/original_source_bindings.json`.
Replaces: REQ-0351, REQ-0352, REQ-0353, REQ-0354, REQ-0355, REQ-0365.

```yaml
source:
  variable: RAW.AGE
  absent: {literal: null}
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
match follows [Lookup and joins](lookup.md). `source.keep` outside
`first` or `last` is a schema failure.

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
Evidence: `schema-handlers`, `rust/crates/yamaa-adapters/tests/fixtures/original_conversion_handlers.json`, `rust/crates/yamaa-adapters/tests/fixtures/original_row_conversion_handlers.json`.
Replaces: REQ-0219, REQ-0358, REQ-0359, REQ-0363, REQ-0366.

A derivation that needs stage 3 wraps its expression in `value`:

```yaml
derivation:
  value:
    source: RAW.AGE
  unconvertible: {literal: null}
```

[Values](values.md) defines which conversions fail and
states that a missing input is not converted at all, so `unconvertible` never
fires for one. A conversion replacement that cannot be converted fails.

<a id="column-14"></a>

### COLUMN-14. Every handler path reports how many records it answered as a report row, zero included

Each handler path's record count is a `handler` row of the run report under
[CHECK-23](verification.md#check-23), for a failed run as well as a successful
one. A handler firing zero times is reportable and is not an error. The count
for a source's `order_by` and `keep` reports only records where more than one
match survived the filter.

Evidence: `rust/crates/yamaa-adapters/tests/fixtures/original_source_bindings.json`, `rust/crates/yamaa-adapters/tests/fixtures/original_mapping_cut.json`, `rust/crates/yamaa-core/tests/execution_numeric_lifecycle.rs::accounting_retains_zero_declarations_and_prior_firings_on_failure`.
Replaces: REQ-0356, REQ-0361.

## Diagnostic citations

<a id="column-15"></a>

### COLUMN-15. A language diagnostic cites the requirement that owns its failure; only listed implementation conditions cite none

A language diagnostic cites the requirement that owns its failure. A missing
normative requirement is permitted only for the implementation conditions
listed below, in their stated phases. These conditions report an admitted
operation that the implementation does not support or an engine refusal;
neither asserts a language rule violation.

Fails: `unsupported_operation`, `engine_rejected`.
Evidence: `tools/tests/test_diagnostics.py::test_null_policy_is_explicit_registered_and_reached`.
Replaces: REQ-1297.

<!-- normative-null-conditions:start -->
- `unsupported_operation` (`planning`)
- `engine_rejected` (`engine`)
<!-- normative-null-conditions:end -->
