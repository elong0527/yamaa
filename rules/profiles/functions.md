# Project functions

This file is a profile: it is optional, and it applies only to a build whose
caller supplies a project file linking an environment file that declares
`functions` for the host language, and to the specification calls that name
them. Project code runs in that host language through statically named
installed callables, its contract is closed, and its inline tests run before
any study data is read. A build without project functions is unaffected,
except that a linked environment file is admitted under [FUNC-2](#func-2).

## The environment file

<a id="func-1"></a>

### FUNC-1. Project functions come only from the environment file the caller's project links for the host language

A portable specification may declare a logical `function` call before project
code exists: structural authoring validates its versionless `name` and closed
argument-leaf shape, and contract-dependent checks require an explicitly
supplied project file that links an environment for the host language. The
project's `environments` entry for that language links the one environment
file the host reads ([PROJECT-2](project.md#project-2)). A specification cannot
select, replace or override the project or its environments.

Fails: `project_environment_missing`.
Evidence: `rust/crates/yamaa-adapters/tests/file_project.rs::unavailable_environment_precedes_bad_entry_and_valid_environment_is_retained_before_missing_entry`, `python/tests/installed/installed_public_environment.py::test_missing_environment_file_precedes_bad_or_absent_entry_without_activation`, `python/tests/installed/installed_public_environment.py::test_each_host_reads_only_its_own_environment_entry`.

Omission never creates an environment implicitly, and no adjacent environment
or language subdirectory is discovered. Every path an environment file declares
resolves relative to that file under the admitted resource policy of
[File resolution](../reference/files.md). A supplied project file that cannot
be read fails with `project_environment_missing`. A call to a project function
when the supplied project links no environment for the host language cannot
execute: it fails with `project_environment_missing`, reported at each call
with the host language. A call when no project is supplied names no declared
function under [FUNC-4](#func-4).

<a id="func-2"></a>

### FUNC-2. An environment file is admitted on its own against the environment schema before any activation or study read

An environment file is independently admitted against
`schema_environment.yaml`. `schema_version` selects the schema bundle, and
there is no environment content version. A missing, unreadable or invalid
project file, linked environment file or declared metadata resource fails
before activation or study reads. An invalid project file, environment file,
definition, test or duplicate logical declaration fails, and static checking
returns every independently inferable finding before activation.

Fails: `project_environment_invalid`.
Evidence: `schema-functions`, `rust/crates/yamaa-adapters/tests/file_project.rs::metadata_failures_name_each_written_dependency_and_cause_before_study_reads`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::inline_scalars_report_alone_without_study_or_language_findings_and_unlinked_hosts_read_nothing`.

A missing or invalid declared metadata resource, including the environment
file a project links, reports its written path, declaring field and concrete
cause, and invalid YAML retains its line, column and parser reason.

An environment file declares `schema_version`, and `lock` and `functions`
declare installed code. The language comes from the project `environments` key
that links the file ([FUNC-3](#func-3)). Study terminology and submission
metadata belong to the project file ([PROJECT-3](project.md#project-3)), not to
an environment file.

The `lock` and external function declarations are local resource paths under
[File resolution](../reference/files.md). Relative spellings start at the
environment file that declares them, not at the project file that links it
([PROJECT-4](project.md#project-4)). Absolute local paths and parent segments
have the same meaning as domain resource paths, and an absent file fails at
that location without configuration discovery or fallback search.

<a id="func-3"></a>

### FUNC-3. One host language applies to every function, and a declared lock pins the called packages

The `environments` key that links an environment file, exactly `python` or
`r`, states the language of that file and applies to every function definition
it declares; an environment file has no language field of its own. `lock`
optionally names the packaging lock for that language: uv TOML for Python or
renv JSON for R. On each build that calls a function with a declared lock,
before any study read, the host verifies the installed version of yamaa and of
every called function package against the captured lock.

Fails: `runtime_artifact_mismatch`, `project_environment_invalid`.
Evidence: `schema-functions`, `rust/crates/yamaa-core/tests/preparation_project_preparation.rs::lock_kind_and_function_findings_are_core_owned_after_capture_without_code_authority`, `rust/crates/yamaa-adapters/tests/project_environment.rs::lock_and_actual_conformance_mismatches_keep_complete_zero_read_reports`.

Parallel bindings and per-function language choices are invalid. `lock` is
optional, including when `functions` is present. Omission uses the installed
callable without a package-version audit; yamaa does not search for a lock
beside the environment file. An explicitly declared lock must exist and be
valid. The packaging tool manages installation, code and transitive
dependencies; yamaa neither vendors, resolves, pins nor caches project code.
Captured bytes establish the lock format independently of the filename.

Static checking rejects a lock format incompatible with the language the
linking `environments` key states, without importing project code or reading
study data, and fails with `project_environment_invalid`. Standard-library and
base-package functions are exempt from the version check. This is a
called-package check; a full environment audit belongs to uv or renv.

A missing, invalid, ambiguous or mismatched installed called-package version
fails with `runtime_artifact_mismatch`; its context identifies the package,
the expected lock versions and the observed installed version.

## Definitions and calls

<a id="func-4"></a>

### FUNC-4. Each logical function has one closed definition, and a call names only the function and its arguments

`functions` maps unique logical names to one definition each, either inline or
through a path. A definition contains a qualified `function`, a non-empty
`description`, closed ordered `params`, a scalar `returns`,
`may_return_missing` (default false), `comparison_decimals` (default four) and
inline `tests`. A specification call contains only the logical `name` and
`args`, and its result is one scalar for one logical row. A call naming no
declared logical function fails.

Fails: `unknown_project_function`.
Evidence: `schema-functions`, `rust/crates/yamaa-core/tests/project_execution.rs::complete_static_faults_are_projected_in_authored_call_order_without_source_binding`, `rust/crates/yamaa-core/tests/project_execution.rs::retired_fields_and_malformed_literals_are_complete_static_findings`.

A call contains no implementation language, callable name or contract version.
A `function` call's exact argument and result types are those its definition
states. `expressions.function.name` selects a logical function from the
environment the project links for the host language, and `args` supplies its
named argument leaves; the result
is one scalar for one logical row under the shared invocation lifecycle.

A definition file contains that one definition and does not repeat its logical
name or schema version. Definition and test paths use the declaring-file
resource policy, and a definition is not split into contract, binding and
conformance documents.

<a id="func-5"></a>

### FUNC-5. A signature is closed and named, and every argument and default matches its declared type exactly

Signatures are closed and named: each parameter name is unique, and a binding
has no positional, variadic or arbitrary keyword parameter bag. `required`
defaults to true; every optional parameter declares an environment `default`,
and a required parameter declares no `default`. Parameter types are the
[Values](../values.md) column vocabulary plus function-only
`bool`, and return types do not include `bool`. Every argument and default
matches its declared exact type, with no implicit conversion.

Fails: `invalid_function_argument`.
Evidence: `negative-function-contract`, `rust/crates/yamaa-core/tests/project_function_invocation.rs::shared_function_invocation_vectors`, `rust/crates/yamaa-core/tests/project_execution.rs::complete_static_faults_are_projected_in_authored_call_order_without_source_binding`.

The function-only `bool` introduces neither Boolean columns nor Boolean
derivation results. There is no `int`-to-`float` widening, and
[Values](../values.md) conversion can run only after the
function has returned under the
[Column derivation](../columns.md). Argument names, requiredness
and exact types are contract-dependent checks at the implementation stage;
structural validation before then checks only the closed argument-leaf forms of
[FUNC-6](#func-6). An unknown, missing required, or incorrectly typed argument
or default fails with `invalid_function_argument`.

<a id="func-6"></a>

### FUNC-6. A call argument is a named variable or a scalar literal leaf, never an expression

A call argument is one of: a named variable, written as a plain string; an
`int`, `float`, `bool` or missing YAML scalar; a string literal written as
`{literal: text}`; a date literal written as `{date: YYYY-MM-DD}`; or a
datetime literal written as `{datetime: YYYY-MM-DDThh:mm[:ss]}`. The temporal
text must be a [Values](../values.md) value. No argument may
contain another expression.

Evidence: `schema-functions`, `schema-non-finite`, `rust/crates/yamaa-core/tests/project_execution.rs::retired_fields_and_malformed_literals_are_complete_static_findings`.

A calculation needed by a function is first declared as an internal column and
then passed by name, preserving its visible dependency.

<a id="func-7"></a>

### FUNC-7. An omitted optional argument takes its default, and a missing argument short-circuits unless its parameter accepts missing

Omitting an optional argument selects the environment default; passing missing
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

`function` is a statically written fully qualified callable, such as
`projectbmi.bmi` or `projectbmi::bmi`, whose concrete closed named signature
matches the parameter names exactly. Inline code, anonymous functions,
evaluation, shell commands, script paths, executable argument transforms and
variadic signatures are invalid. After applying environment defaults and
missing short-circuiting, the runner maps the logical arguments and invokes the
callable once for one logical row, supplying no undeclared data or execution
context.

Fails: `function_call_failed`.
Evidence: `schema-functions`, `rust/crates/yamaa-core/tests/project_execution.rs::observer_counts_actual_calls_and_preserves_signature_short_circuit_and_original_failure`, `rust/crates/yamaa-adapters/tests/project_environment.rs::actual_engine_failures_project_without_a_second_callback_or_payload_consumption`.

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
Evidence: `schema-non-finite`, `rust/crates/yamaa-core/tests/project_function_invocation.rs::shared_function_invocation_vectors`, `R/yamaa/tests/function_values.R`.

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

Fails: `project_environment_invalid`.
Evidence: `schema-functions`, `schema-non-finite`, `rust/crates/yamaa-adapters/tests/project_environment.rs::complete_static_report_keeps_semantics_alongside_original_opaque_capture_and_authored_holes`.

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
missing obligations with `project_environment_invalid`. The contract author
identifies which input is the contract semantic boundary. Activation checks the
declared result.

Equivalent projects in another language carry the same logical parameter
declarations, comparison policy and test content. Qualified callable names and
packaging locks differ by host.

<a id="func-12"></a>

### FUNC-12. Every build runs every case of every called function before any study data is read

Every build with called functions verifies the lock when declared, binds the
called functions and runs every case of every called function before any study
data is read. Ordinary independent failures collect; an original host interrupt
aborts immediately. There is no activation cache, and an empty selection
performs no lock verification or code execution. A failed activation vector or
numeric comparison fails.

Fails: `function_conformance_failed`.
Evidence: `rust/crates/yamaa-core/tests/project_execution.rs::owned_environment_repeats_selected_activation_before_cached_study_capture`, `rust/crates/yamaa-core/tests/project_execution.rs::lock_binding_case_and_original_interrupt_failures_never_enter_study_ports`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::real_activation_mismatches_keep_every_case_in_selected_order_without_retry`.

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
