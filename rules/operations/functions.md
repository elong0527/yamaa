---
id: operations/functions
title: Project functions
status: normative
---

# Project functions

## Requirements

### Type behavior

<a id="req-0314"></a>

**REQ-0314.** `function` states its exact argument and result types in [Project functions](functions.md).

### Project resolution

<a id="req-0662"></a>

**REQ-0662.** A portable specification may declare a logical `function` call before project code exists. Structural authoring validates its versionless `name` and closed argument-leaf shape. Contract-dependent checks require an explicitly supplied environment. Omission never creates an environment implicitly.

<a id="req-0663"></a>

**REQ-0663.** The caller supplies one environment file through `environment=` to `domain` or `check`. Every path declared there resolves relative to that file under the admitted resource policy. A specification cannot select, replace or override the environment. No adjacent environment or language subdirectory is discovered implicitly.

<a id="req-0664"></a>

**REQ-0664.** The environment is independently admitted against `schema_environment.yaml`. `schema_version` selects the schema bundle. There is no environment content version. Missing, unreadable or invalid supplied metadata fails before activation or study reads.

### Installed runtime

<a id="req-0665"></a>

**REQ-0665.** `language` is exactly `r` or `python` and applies to every function definition. It is required when `functions` is present. Parallel bindings and per-function language choices are invalid.

<a id="req-0666"></a>

**REQ-0666.** `lock` names the packaging lock for the running environment: uv TOML for Python or renv JSON for R. It is required when `functions` is present. The packaging tool manages installation, code and transitive dependencies; yamaa neither vendors, resolves, pins nor caches project code. Captured bytes establish lock format independently of the filename.

<a id="req-0667"></a>

**REQ-0667.** Static checking rejects a host-language mismatch or a lock format incompatible with `language`, without importing project code or reading study data. On each build that calls a function, before any study read, the host verifies the installed version of yamaa and every called function package against the captured lock. Standard-library/base-package functions are exempt. This is a called-package check; a full environment audit belongs to uv or renv.

### Function definitions

<a id="req-0668"></a>

**REQ-0668.** `functions` maps unique logical names to one definition each, either inline or through a path. A specification call contains only logical `name` and `args`. It contains no implementation language, callable name or contract version.

<a id="req-0669"></a>

**REQ-0669.** A definition contains a qualified `function`, non-empty `description`, closed ordered `params`, scalar `returns`, `may_return_missing` (default false), `comparison_decimals` (default four), and inline `tests`. A definition file contains that one definition and does not repeat its logical name or schema version. Definition and test paths use the declaring-file resource policy; a definition is not split into contract, binding and conformance documents.







### Parameters and arguments

<a id="req-0676"></a>

**REQ-0676.** Signatures are closed and named. Each parameter name is unique and
a binding has no positional, variadic, or arbitrary keyword parameter bag.
`required` defaults to `true`. Every optional parameter declares an environment
`default`. A required parameter declares no `default`.

<a id="req-0677"></a>

**REQ-0677.** Parameter types are the [Types and conversion](../values/types.md) column vocabulary plus function-only
`bool`. Return types are the [Types and conversion](../values/types.md) column vocabulary and do not include `bool`,
so this extension introduces neither Boolean columns nor Boolean derivation
results.

<a id="req-0678"></a>

**REQ-0678.** Every argument and default matches its declared exact type. There
is no implicit conversion, including no `int`-to-`float` widening. [Types and conversion](../values/types.md)
conversion can run only after the function has returned under the [Execution lifecycle](../execution/lifecycle.md)
lifecycle. Argument names, requiredness, and exact types are contract-dependent
checks at the implementation stage. Structural validation before then checks
only the closed argument-leaf forms below.

<a id="req-0679"></a>

**REQ-0679.** A call argument is one of:

- a named variable, written as a plain string;
- an `int`, `float`, `bool`, or missing YAML scalar;
- a string literal written as `{literal: text}`;
- a date literal written as `{date: YYYY-MM-DD}`; or
- a datetime literal written as `{datetime: YYYY-MM-DDThh:mm[:ss]}`.

<a id="req-0680"></a>

**REQ-0680.** The temporal text must be an [Temporal values](../values/temporal.md) value. No argument may contain
another expression. A calculation needed by a function is first declared as an
internal column and then passed by name, preserving its visible dependency.

<a id="req-0681"></a>

**REQ-0681.** Omitting an optional argument selects the environment default.
Passing missing explicitly never selects the default. `accepts_missing` is
`false` by default for each parameter:

- if any supplied value is missing for a non-accepting parameter, the call is
  not invoked and its result is missing; and
- a missing value for an accepting parameter is passed to the binding as the
  host runtime's canonical missing scalar.

<a id="req-0682"></a>

**REQ-0682.** A short-circuit result is not a result returned by the binding and
therefore does not require `may_return_missing: true`.

### Binding and invocation

<a id="req-0683"></a>

**REQ-0683.** `function` is a statically written fully qualified callable, such as `projectbmi.bmi` or `projectbmi::bmi`. Parameter names match the installed callable's concrete closed named signature exactly. No argument-name mapping or computed name is permitted. Normal installed module or namespace resolution runs only after the lock gate passes.

<a id="req-0684"></a>

**REQ-0684.** Inline code, anonymous functions, evaluation, shell commands, script paths, executable argument transforms and variadic signatures are invalid. Project code is an ordinary package installed by the packaging tool.

<a id="req-0685"></a>

**REQ-0685.** After applying environment defaults and missing short-circuiting,
the runner maps the logical arguments and invokes the callable once for one
logical row. The runner supplies no undeclared data or execution context.
The binding returns one scalar of the declared exact type. Batch or vector
execution is allowed only when every observable value and failure matches
independent calls in logical row order.

<a id="req-0686"></a>

**REQ-0686.** An invoked binding may return missing only when
`may_return_missing` is true. It may not return a vector, collection, table,
object wrapper, or value of a different type. [Execution lifecycle](../execution/lifecycle.md) conversion is not a repair
mechanism for an invalid function result. [Types and conversion](../values/types.md)'s non-finite normalization runs
immediately after the host scalar is returned and before these result checks.
The normalized missing result therefore still requires `may_return_missing:
true`.

A designated lossless host scalar is a representation of an existing logical
type, not an arbitrary object wrapper. The R adapter uses `yamaa_int64` (one
canonical signed-i64 decimal string) for `int` arguments and `yamaa_utf8` (one
validated UTF-8 byte sequence, including NUL) for `str` arguments. It validates
the exact class, storage, shape and payload; subclasses and extra attributes
are not admitted. Results may use those same representations or an unclassed
R integer/character scalar of the corresponding exact logical type. Double and
logical scalars remain `float` and `bool`; there is no numeric or text coercion.
R `date` and `datetime` use the Date and explicitly UTC POSIXct representations
fixed by [Temporal values](../values/temporal.md#req-0563). This host mapping adds no logical type.

### Activation conformance

<a id="req-0687"></a>

**REQ-0687.** Every definition contains inline `tests`. Each uniquely named case declares `covers`, logical `args`, and one expected `result`. Tests obey the same exact types, defaults and missing permissions as specification calls. They do not repeat a logical name or contract version.

<a id="req-0688"></a>

**REQ-0688.** `covers` is a non-empty list of unique obligations demonstrated by
that case:

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

<a id="req-0689"></a>

**REQ-0689.** Every contract covers `normal` and `boundary`. Every optional
parameter covers its default, every parameter covers its applicable missing
behavior, and both values of every Boolean parameter are covered. A nullable
contract covers `nullable-output`, and a float-returning contract covers
`numeric-comparison`. Static validation checks each tagged case for inferable
evidence and rejects missing obligations. The contract author identifies which
input is the contract semantic boundary. Activation checks the declared result.

<a id="req-0690"></a>

**REQ-0690.** Equivalent projects in another language carry the same logical parameter declarations, comparison policy and test content. Qualified callable names and packaging locks differ by host.

<a id="req-0691"></a>

**REQ-0691.** Every build with called functions verifies the lock, binds the called functions and runs every case of every called function before any study data is read. Ordinary independent failures collect; an original host interrupt aborts immediately. There is no activation cache. Empty selection performs no lock verification or code execution.

### Numeric conformance and rounding

<a id="req-0692"></a>

**REQ-0692.** Nonnumeric expected results and missingness use equality for their
type, including [Text values](../values/text.md) for strings. Numeric results compare temporary decimal
copies at the contract's non-negative `comparison_decimals`; the default is
four. At an exact decimal tie, the copy rounds to the nearest value away from
zero. For example, at four places, `1.23445` becomes `1.2345` and `-1.23445`
becomes `-1.2345`.

<a id="req-0693"></a>

**REQ-0693.** The comparator never replaces or mutates the runtime result.
Calculations use the unrounded result, and rounding for final display occurs
once under [CSV profile](../storage/csv.md)'s `output.decimals`. Predicate outcomes, keys, row membership
and order, conditions, and final displayed artifacts must still agree exactly
across projects; `comparison_decimals` is not a tolerance for structural
differences.

### Interface behavior

<a id="req-1080"></a>

**REQ-1080.** The `environment_class` fields have these meanings: `schema_version` selects the schema; `language`, `lock`, and `functions` declare installed code; `codelists` supplies study terminology; `study` and `sdtm`, `adam`, `send` carry submission metadata for [Define-XML](../submission/define-xml.md). A submission section requires `study`. Domain builds do not open submission document links or generate a Define-XML artifact.



<a id="req-1083"></a>

**REQ-1083.** The `function_definition_class` fields hold the qualified `function`, `description`, ordered `params`, scalar `returns`, nullable result permission, comparison precision and inline `tests` described by REQ-0669. Each `function_test_class` holds `id`, `covers`, `args` and `result`. The parameter class holds `name`, `type`, `required`, `default` and `accepts_missing`.


<a id="req-1085"></a>

**REQ-1085.** `expressions.function.name` selects a logical function from the supplied environment; `args` supplies its named argument leaves. The result is one scalar for one logical row under the shared invocation lifecycle.

<a id="req-1086"></a>

**REQ-1086.** The `function_arg` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `function_arg` | Named variable or scalar literal leaf; arbitrary nesting is not allowed. |


## Error conditions

<a id="req-0338"></a>

**REQ-0338.** A project function that violates its environment, contract,
binding, or result requirements: fail under [Project functions](functions.md).

<a id="req-0694"></a>

**REQ-0694.** A supplied environment that cannot be read: `project_environment_missing`. A call without an implementation environment cannot execute.

<a id="req-0695"></a>

**REQ-0695.** An invalid environment, definition, test or duplicate logical declaration: `project_environment_invalid`. Static checking returns every independently inferable finding before activation.

<a id="req-0696"></a>

**REQ-0696.** Unsupported or mismatched runner
language: `runner_language_mismatch`.

<a id="req-0697"></a>

**REQ-0697.** A missing, invalid, ambiguous or mismatched installed called-package version: `runtime_artifact_mismatch`. The historical condition name remains stable; its context identifies the package, expected lock versions and observed installed version.

<a id="req-0698"></a>

**REQ-0698.** A call naming no
declared logical function: `unknown_project_function`.


<a id="req-0700"></a>

**REQ-0700.** An unknown, missing required, or incorrectly typed argument or
default: `invalid_function_argument`.

<a id="req-0701"></a>

**REQ-0701.** A host exception or enforced
resource failure: `function_call_failed`. It is fatal and has no [Local handlers](../execution/handlers.md) local
fallback.

<a id="req-0702"></a>

**REQ-0702.** A wrong type or shape, or an undeclared missing value
returned by an invoked binding: `invalid_function_result`. It is fatal and is
not converted.

<a id="req-0703"></a>

**REQ-0703.** A failed activation vector or numeric comparison:
`function_conformance_failed`.

<a id="req-0704"></a>

**REQ-0704.** Each invocation failure identifies its logical function, qualified callable, authored source and path, and original host context when code ran. Original host error and interrupt objects remain retained independently of human-readable issue formatting.
