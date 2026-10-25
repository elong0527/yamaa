# Project functions

This file is a profile: it is optional, and it applies only to a build whose
caller supplies a project file that lists function files, and to the
specification calls that name them. Project code runs in the host language
through the statically named installed callable each function file names for
it, its contract is closed, and its inline tests run before any study data is
read. A build without project functions is unaffected, except that each listed
function file is admitted under [FUNC-2](#func-2).

## Function files

<a id="func-1"></a>

### FUNC-1. Project functions come only from the function files the caller's project lists, through the running host's implementation

A portable specification may declare a logical `function` call before project
code exists: structural authoring validates its versionless `name` and closed
argument-leaf shape, and contract-dependent checks require an explicitly
supplied project file whose `functions` lists the function file defining that
name. The host running the build calls the callable that file's
`implementations` names for its language, and a called function with no
implementation for that host fails before any input is read. A specification
cannot select, replace or override the project or its function files.

Fails: `function_implementation_missing`, `project_binding_missing`.
Evidence: `rust/crates/yamaa-core/tests/fixtures/diagnostics/projects.json::a_function_without_a_callable_for_the_host_language_fails_each_call_before_data_is_read`, `rust/crates/yamaa-core/tests/project_execution.rs::an_engine_without_a_host_language_compiles_a_call_and_misses_no_implementation`, `rust/crates/yamaa/tests/project_binding.rs::each_binding_of_a_specification_without_its_project_fails_before_any_input_is_read`, `rust/crates/yamaa-core/tests/preparation_project_preparation.rs::a_1_0_host_reads_only_its_own_lock_and_every_listed_function_file`, `rust/crates/yamaa-adapters/tests/file_workflow.rs::unavailable_environment_precedes_bad_entry_and_valid_environment_is_retained_before_missing_entry`, `python/tests/installed/installed_public_environment.py::test_missing_function_file_precedes_bad_or_absent_entry_without_activation`, `python/tests/installed/installed_public_environment.py::test_each_host_reads_only_its_own_lock`.

No function file is discovered, and none is created by omission. A function
file that names no implementation for the host is admitted, since another host
may run the build; a call of it fails with `function_implementation_missing`
at the call's `name`, reported at each call with the function and the host
language, while the specification is checked, before activation. A call when
no project file is supplied fails with `project_binding_missing` at the call's
`name`, with the function, before any input is read
([CT-13](terminology.md#ct-13)). A supplied project file that cannot be read
fails as any path the caller passes ([FILE-1](../reference/files.md#file-1)).

<a id="func-2"></a>

### FUNC-2. A function file is admitted on its own against the function schema before any activation or study read

A function file is independently admitted against the function schema; it
declares `yamaa` and `type: function`
([SCHEMA-2](../reference/schema-language.md#schema-2)), and there is no
function content version. A missing, unreadable or invalid project file,
listed function file or declared metadata resource fails before activation or
study reads. An invalid definition or test, or a logical name that two listed
files declare, fails, and static checking returns every independently
inferable finding before activation.

Fails: `function_file_invalid`.
Evidence: `schema-functions`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_files.json::a_python_spelling_of_the_r_callable_fails_in_the_function_file_in_either_host`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_files.json::a_case_date_its_declared_type_cannot_read_fails_in_the_function_file_as_written`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_files.json::a_listed_function_file_without_the_header_fails_in_that_file_at_its_missing_yamaa`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_files.json::a_listed_function_file_that_is_missing_fails_at_its_listing`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_files.json::a_listed_file_that_is_no_yaml_fails_in_that_file_at_its_listing`, `rust/crates/yamaa-adapters/tests/declaration_project_files.rs::an_unclassified_read_failure_refuses_the_preparation_opaquely`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/environment.json::each_function_file_finding_and_a_lock_of_another_host_fail_where_they_are_written`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/environment.json::a_function_file_value_its_type_cannot_read_fails_as_its_file_keeping_its_text`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/environment.json::a_lock_that_is_no_packaging_lock_fails_at_its_field_naming_the_project_file`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/workflow.json::each_file_a_project_names_that_does_not_exist_fails_at_the_field_naming_it`, `rust/crates/yamaa-adapters/tests/file_workflow.rs::metadata_failures_name_each_written_dependency_and_cause_before_study_reads`.

A function file declares `yamaa`, `type`, its own `name`, and the definition
of [FUNC-4](#func-4). The project's `functions` checks the header of each
file it lists as every referencing field does
([SCHEMA-2](../reference/schema-language.md#schema-2)). Every listed function
file is read and admitted, whichever host languages it implements. Study
terminology and submission metadata belong to the project file
([PROJECT-3](project.md#project-3)), not to a function file.

A finding in a function file fails with `function_file_invalid` at the file's
own path, such as `tests[0].result` or `implementations.r`, and names the
function file as its `source`, the project file as its `entry` and the listing
as its `environment_path`, such as `functions[0]`. A logical name that a
second listed file declares fails at that file's `name`. A value its declared
type cannot read, or an integer outside the `int` range, fails with its
`reason` and the text it is written as.

Each listed function file is a local resource path under
[File resolution](../reference/files.md), relative to the project file
([PROJECT-4](project.md#project-4)), as are the lock
([FUNC-3](#func-3)) and each codelist file. Absolute local paths and parent
segments have the same meaning as domain resource paths, and an absent file
fails at that location without configuration discovery or fallback search. A
listed file or the lock that cannot be read fails as any file a run reads, at
the field that names it, `functions[i]`, `codelists[i]` or
`environments.<host>.lock`, with the path as written and one condition of
[FILE-11](../reference/files.md#file-11): a listed function file that does not
exist fails at `functions[i]` with `resource_path_missing`. A host failure that
FILE-11 does not classify is an engine refusal, `engine_rejected`.

<a id="func-3"></a>

### FUNC-3. One host language applies to every function, and a declared lock pins the called packages

One host language, `python` or `r`, runs a whole build: each called function
runs through the callable its `implementations` entry names for that language.
`environments.<host>.lock` in the project file optionally names that
language's packaging lock: uv TOML for Python or renv JSON for R. On each build
that calls a function with a declared lock, before any study read, the host
verifies the installed version of yamaa and of every called function package
against the captured lock.

Fails: `runtime_artifact_mismatch`, `function_file_invalid`.
Evidence: `schema-functions`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/environment.json::each_function_file_finding_and_a_lock_of_another_host_fail_where_they_are_written`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/environment.json::a_lock_that_is_no_packaging_lock_fails_at_its_field_naming_the_project_file`, `rust/crates/yamaa-core/tests/preparation_project_preparation.rs::lock_kind_and_function_findings_are_core_owned_after_capture_without_code_authority`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_functions.json::lock_findings_report_against_the_project_file_that_holds_the_lock`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_files.json::a_lock_that_is_no_packaging_lock_fails_as_the_function_files_do`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_activation.json::an_r_lock_finding_reports_at_the_lock_field_of_the_project_that_holds_it`.

Parallel bindings and per-function language choices are invalid:
`implementations` names one callable per host language. A lock is optional,
including when `functions` is present. Omission uses the installed callable
without a package-version audit; yamaa does not search for a lock beside the
project file. An explicitly declared lock must exist and be valid. The
packaging tool manages installation, code and transitive dependencies; yamaa
neither vendors, resolves, pins nor caches project code. Captured bytes
establish the lock format independently of the filename.

Static checking rejects a lock format incompatible with the host key that
holds it, and captured bytes that are no packaging lock, without importing
project code or reading study data, and fails with `function_file_invalid` at
`environments.<host>.lock`. Standard-library and base-package functions are
exempt from the version check. This is a called-package check; a full
environment audit belongs to uv or renv.

A missing, invalid, ambiguous or mismatched installed called-package version
fails with `runtime_artifact_mismatch` at `environments.<host>.lock`; its
context names the project file as the lock's source and identifies the
package, the expected lock versions and the observed installed version.

## Definitions and calls

<a id="func-4"></a>

### FUNC-4. Each logical function has one closed definition, and a call names only the function and its arguments

The project's `functions` lists function files, each holding one definition
under a unique logical `name`. A definition contains `name`, a non-empty
`description`, closed ordered `params`, a scalar `returns`, `implementations`
naming a qualified callable for `python`, `r` or both, `may_return_missing`
(default false), `comparison_decimals` (default four) and inline `tests`. A
specification call contains only the logical `name` and `args`, and its result
is one scalar for one logical row. A call naming no function the project lists
fails.

Fails: `unknown_project_function`.
Evidence: `schema-functions`, `rust/crates/yamaa-core/src/front_end/v1/project.rs::a_function_file_names_its_function_and_types_its_values_by_declaration`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/projects.json::complete_static_faults_are_projected_in_authored_call_order_without_source_binding`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/projects.json::retired_fields_and_malformed_literals_are_complete_static_findings`.

A call contains no implementation language, callable name or contract version.
A `function` call's exact argument and result types are those its definition
states. `expressions.function.name` selects a logical function from the
function files the project lists, and `args` supplies its named argument
leaves; the result is one scalar for one logical row under the shared
invocation lifecycle.

A function file holds one definition and declares its own `name`, `yamaa` and
`type`; the project holds no inline definition. A function file that names
no callable at all fails with `function_file_invalid`
([FUNC-2](#func-2)). A definition is not split into contract, binding and
conformance documents.

<a id="func-5"></a>

### FUNC-5. A signature is closed and named, and every argument and default matches its declared type exactly

Signatures are closed and named: each parameter name is unique, and a binding
has no positional, variadic or arbitrary keyword parameter bag. `required`
defaults to true; every optional parameter declares a `default`,
and a required parameter declares no `default`. Parameter types are the
[Values](../values.md) column vocabulary plus function-only
`bool`, and return types do not include `bool`. Every argument and default
matches its declared exact type, with no implicit conversion.

Fails: `invalid_function_argument`.
Evidence: `negative-function-contract`, `rust/crates/yamaa-core/tests/project_function_invocation.rs::shared_function_invocation_vectors`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/projects.json::complete_static_faults_are_projected_in_authored_call_order_without_source_binding`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/function_arguments.json::a_call_argument_that_is_no_scalar_names_its_argument_and_no_written_value`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/function_arguments.json::a_call_argument_that_names_no_day_names_its_argument_and_its_written_date`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/runtime.json::an_argument_of_another_type_than_its_parameter_is_invalid`.

The function-only `bool` introduces neither Boolean columns nor Boolean
derivation results. There is no `int`-to-`float` widening, and
[Values](../values.md) conversion can run only after the
function has returned under the
[Column derivation](../columns.md). Argument names, requiredness
and exact types are contract-dependent checks at the implementation stage;
structural validation before then checks only the closed argument-leaf forms of
[FUNC-6](#func-6). An unknown, missing required, or incorrectly typed argument
or default fails with `invalid_function_argument`.

Its context names the `function` and, for a fault of one argument, the
`argument`, with a `cause`: `unknown_argument`, `duplicate_argument`,
`missing_required_argument`, or `type_mismatch` with the parameter's
`expected_type` and the argument's `actual_type`. A written value the engine
cannot run with is a fault of its argument too: an integer outside the 64-bit
range (`integer_range`), a `DATE` or `DATETIME` literal that names no day or
second (`invalid_date`, `invalid_datetime`), or a value that is no scalar
(`invalid_shape`), each naming the text it is written as in `value`, null when
it is no scalar text. A call with other fields than `name` and `args`, or
whose `args` are no mapping, fails as `invalid_shape` there, naming no
argument. A run that passes a function an argument it does not accept fails
in the derivation phase, its context naming the `function` and its `call`,
and the `unknown` arguments, or the `argument`, with `expected_type` and
`actual_type` for a type mismatch.

A function file writes a `date` or `datetime` default, test argument or result
as text, which the declared parameter or return type reads; text that type
cannot read fails the file under [FUNC-2](#func-2). Each parameter name must be
a name in every host language the file implements.

<a id="func-6"></a>

### FUNC-6. A call argument is one leaf of the expression grammar, never a composed expression

A call argument is one expression leaf: a bare name for a variable, quoted
text, a YAML number or Boolean, YAML null for missing, or a `DATE` or
`DATETIME` literal whose text is a [Values](../values.md) value. No argument
may contain an operator, a function call or another expression, and a name
resolves under [FORMULA-9](../formulas.md#formula-9).

Fails: `invalid_function_argument`.
Evidence: `rust/crates/yamaa-adapters/tests/declaration_v1_lowering.rs::a_function_argument_is_one_leaf_and_any_other_text_fails`, `schema-functions`, `schema-non-finite`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/projects.json::retired_fields_and_malformed_literals_are_complete_static_findings`.

An argument is an expression field ([EXPR-5](../expressions.md#expr-5)):
`weight: WEIGHTKG` passes a variable, `unit: "'kg'"` text,
`on: "DATE '2024-01-01'"` a date and `none: null` the missing value. Text that
is no single leaf, such as `"WEIGHTKG * 2"`, `"ABS(WEIGHTKG)"` or `"TRUE"`,
fails validation as `invalid_function_argument` at the argument, its context
naming the function and the argument, with `cause: expression`. A calculation
needed by a function is first declared as an internal column and then passed
by name, preserving its visible dependency.

<a id="func-7"></a>

### FUNC-7. An omitted optional argument takes its default, and a missing argument short-circuits unless its parameter accepts missing

Omitting an optional argument selects the default; passing missing
explicitly never selects the default. `accepts_missing` is false by default for
each parameter. If any supplied value is missing for a non-accepting parameter,
the call is not invoked and its result is missing. A missing value for an
accepting parameter is passed to the binding as the host runtime's canonical
missing scalar.

Evidence: `schema-functions`, `rust/crates/yamaa-core/tests/project_function_invocation.rs::shared_function_invocation_vectors`, `rust/crates/yamaa-core/tests/project_function_invocation.rs::versionless_invocation_defaults_missing_types_and_repeated_effects_share_one_service`.

A short-circuit result is not a result returned by the binding and therefore
does not require `may_return_missing: true`.

## Binding and invocation

<a id="func-8"></a>

### FUNC-8. A binding is a statically named installed callable, invoked once for each logical row

Each `implementations` entry is a statically written fully qualified callable,
such as `projectbmi.bmi` or `projectbmi::bmi`, whose concrete closed named signature
matches the parameter names exactly. Inline code, anonymous functions,
evaluation, shell commands, script paths, executable argument transforms and
variadic signatures are invalid. After applying defaults and
missing short-circuiting, the runner maps the logical arguments and invokes the
callable once for one logical row, supplying no undeclared data or execution
context.

Fails: `function_call_failed`.
Evidence: `schema-functions`, `rust/crates/yamaa-core/tests/project_execution.rs::observer_counts_actual_calls_and_preserves_signature_short_circuit_and_original_failure`, `rust/crates/yamaa-adapters/tests/project_environment.rs::actual_engine_failures_project_without_a_second_callback_or_payload_consumption`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_functions.json::a_classified_live_call_failure_fails_the_call_after_the_input_is_read`, `negative-function-host-error`.

No argument-name mapping or computed name is permitted. Normal installed module
or namespace resolution runs only after the lock gate of [FUNC-3](#func-3)
passes. Project code is an ordinary package installed by the packaging tool.
Batch or vector execution is allowed only when every observable value and
failure matches independent calls in logical row order.

A host exception or enforced resource failure fails with
`function_call_failed`. It is fatal and has no local fallback under
[Column derivation](../columns.md).

<a id="func-9"></a>

### FUNC-9. A binding returns one scalar of its declared type, and missing only when its definition permits it

The binding returns one scalar of the declared exact type. An invoked binding
may return missing only when `may_return_missing` is true, and it may not
return a vector, collection, table, object wrapper or value of a different
type. [Values](../values.md)'s non-finite normalization
runs immediately after the host scalar is returned and before these checks, so
a normalized missing result still requires `may_return_missing: true`. An
invalid result fails and is not converted.

Fails: `invalid_function_result`.
Evidence: `schema-non-finite`, `rust/crates/yamaa-core/tests/project_function_invocation.rs::shared_function_invocation_vectors`, `R/yamaa/tests/function_values.R`, `negative-function-non-finite-result`.

Conversion under the [Column derivation](../columns.md) is not a
repair mechanism for an invalid function result. A wrong type or shape, or an
undeclared missing value returned by an invoked binding, fails with
`invalid_function_result`; it is fatal.

The R adapter passes plain R values. A missing argument is `NA`, `str` is a
character string, `int` and `float` are doubles and `bool` is a logical.
`date` and `datetime` use the Date and explicitly UTC POSIXct representations
fixed by [Temporal values](../values.md#value-14). An `int` beyond 2^53 in
magnitude or text containing NUL has no exact R value, and the call fails
rather than pass a changed value. The adapter converts a result by the declared
type. `NULL` or a length-one `NA` is missing. Otherwise `str` needs one
character string, `int` one whole number of magnitude at most 2^53, `float`
one number, `date` one Date of whole days and `datetime` one UTC POSIXct of
whole seconds; a number may have integer or double storage. Any other value,
including a logical, a classed number or a vector, is an invalid result. This
host mapping adds no logical type.

<a id="func-10"></a>

### FUNC-10. An invocation failure identifies the logical function, its callable and its authored source

A project function that violates its environment, contract, binding or result
requirements fails under this profile. Each invocation failure identifies its
logical function, qualified callable, authored source and path, and the
original host context when code ran. Original host error and interrupt objects
remain retained independently of human-readable issue formatting.

Evidence: `rust/crates/yamaa-adapters/tests/project_environment.rs::every_runtime_failure_keeps_versionless_identity_and_authored_provenance`, `rust/crates/yamaa-adapters/tests/project_environment.rs::opaque_payload_is_borrowed_unchanged_and_complete_ordinary_facts_remain_exact`, `rust/crates/yamaa-adapters/tests/file_project.rs::project_report_uses_held_external_function_origins_after_metadata_files_change`.

## Activation conformance

<a id="func-11"></a>

### FUNC-11. Each definition's inline tests cover every obligation its contract implies

Every definition contains inline `tests`. Each uniquely named case declares
`covers`, logical `args` and one expected `result`, and obeys the same exact
types, defaults and missing permissions as specification calls. Every contract
covers `normal` and `boundary`; every optional parameter covers its default,
every parameter its applicable missing behavior, and both values of every
Boolean parameter are covered. A nullable contract covers `nullable-output`,
and a float-returning contract `numeric-comparison`.

Fails: `function_file_invalid`.
Evidence: `schema-functions`, `schema-non-finite`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/environment.json::a_function_file_value_its_type_cannot_read_fails_as_its_file_keeping_its_text`, `rust/crates/yamaa-adapters/tests/project_environment.rs::complete_static_report_keeps_semantics_alongside_original_opaque_capture_and_authored_holes`.

Tests do not repeat a logical name or contract version. `covers` is a
non-empty list of unique obligations demonstrated by that case:

| Tag | Required evidence |
|---|---|
| `normal` | A contract-defined ordinary case |
| `boundary` | A contract-defined boundary case |
| `default:name` | The named optional parameter is omitted |
| `accepted-missing:name` | Accepting `name` is explicitly missing |
| `short-circuit-missing:name` | Missing non-accepting `name`; missing result |
| `nullable-output` | An invoked nullable binding returns missing |
| `boolean-true:name`, `boolean-false:name` | `name` has stated Boolean value |
| `numeric-comparison` | Non-missing `float` result tests decimal comparison |

Static validation checks each tagged case for inferable evidence and rejects
missing obligations with `function_file_invalid`. The contract author
identifies which input is the contract semantic boundary. Activation checks
the declared result.

One function file serves every host language, with one set of parameter
declarations, comparison policy and tests; only each host's callable and lock
differ.

<a id="func-12"></a>

### FUNC-12. Every build runs every case of every called function before any study data is read

Every build with called functions verifies the lock when declared, binds the
called functions and runs every case of every called function before any study
data is read. Ordinary independent failures collect; an original host interrupt
aborts immediately. There is no activation cache, and an empty selection
performs no lock verification or code execution. A failed activation vector or
numeric comparison fails.

Fails: `function_conformance_failed`.
Evidence: `rust/crates/yamaa-core/tests/project_execution.rs::owned_environment_repeats_selected_activation_before_cached_study_capture`, `rust/crates/yamaa-core/tests/project_execution.rs::lock_binding_case_and_original_interrupt_failures_never_enter_study_ports`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_activation.json::every_case_of_each_called_function_runs_and_each_mismatch_reports_in_listing_order`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_activation.json::an_r_case_result_that_differs_fails_conformance_at_its_case_in_the_function_file`.

A failed case is reported in the function file that defines it, at its
`tests[i]`, naming that file as `source`, the project file as `entry` and the
listing, such as `functions[0]`, as `environment_path`; a callable that cannot
be bound fails under [FUNC-2](#func-2) at the function file's
`implementations.<host>`.

<a id="func-13"></a>

### FUNC-13. A numeric result is compared at `comparison_decimals` with the one rounding, and is never changed

Nonnumeric expected results and missingness use equality for their type,
including [Values](../values.md) for strings. Numeric results compare
temporary decimal copies at the contract's non-negative `comparison_decimals`,
four by default, rounded with the one rounding of
[FORMULA-16](../formulas.md#formula-16). The comparator never replaces or
mutates the runtime result, and calculations use the unrounded result.

Evidence: `adam-advs-percentiles`, `rust/crates/yamaa-core/tests/project_execution.rs::ties_and_near_ties_go_away_from_zero_with_the_one_rounding_interval`, `rust/crates/yamaa-core/tests/project_execution.rs::comparison_never_mutates_runtime_bits_or_normalizes_text`.

At four places, `1.23445` becomes `1.2345` and `-1.23445` becomes `-1.2345`;
at two places, `2.675` becomes `2.68`.
Rounding for final display occurs once, under `output.decimals` in
[CSV-8](../reference/csv.md#csv-8). Predicate outcomes, keys, row membership
and order, conditions, and final displayed artifacts must still agree exactly
across projects; `comparison_decimals` is not a tolerance for structural
differences.
