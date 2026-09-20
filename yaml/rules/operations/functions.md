---
id: operations/functions
title: Project functions
status: normative
---

# Project functions

## Purpose

Resolve immutable runtimes and validate function inputs, results, and activation conformance.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Local handlers](../execution/handlers.md).
- [Execution lifecycle](../execution/lifecycle.md).
- [CSV profile](../storage/csv.md).
- [Temporal values](../values/temporal.md).
- [Text values](../values/text.md).
- [Types and conversion](../values/types.md).


## Requirements

### Type behavior

<a id="req-0314"></a>

**REQ-0314.** `function` states its exact argument and result types in [Project functions](functions.md).

### Project resolution

<a id="req-0662"></a>

**REQ-0662.** A portable specification may declare a logical `function` call
before project code is implemented. Specification authoring and structural
validation do not select a project root and do not require an
`environment.yaml`. They validate the call's closed schema shape, including its
logical name, exact requested contract version, and permitted argument leaves.
Contract existence, signature, argument types, missing permissions, and return
type checks wait until an implementation environment is supplied. Omission does
not create or maintain an environment implicitly.

<a id="req-0663"></a>

**REQ-0663.** When actual project code is validated, activated, or executed, the
runner receives one explicitly selected project root. It resolves exactly
`environment.yaml` at that root before reading specification data. A
specification cannot name, replace, extend, or override that environment.

<a id="req-0664"></a>

**REQ-0664.** An environment is validated independently against
`schema_environment.yaml`. Its `schema_version` selects that schema bundle and
the separate `version` identifies the complete environment content. Once an
implementation stage is requested for a specification containing a `function`
expression, missing, unreadable, structurally invalid, or ambiguous environment
resolution fails before code activation or execution.

### One immutable runtime

<a id="req-0665"></a>

**REQ-0665.** `runtime.language` is exactly `r` or `python`. It applies to every
function in the project. An environment cannot contain language-specific sub-
environments, parallel R and Python bindings, or language choices per function.

<a id="req-0666"></a>

**REQ-0666.** `runtime.artifact.reference` names one organization-resolvable
runtime artifact and `runtime.artifact.digest` supplies its verified SHA-256
content identity. That identity covers callable project code and all transitive
dependencies. Only code inside that artifact participates in function
resolution. A global library, process search path, working directory, user
profile, or ambient package installation is not a fallback.

<a id="req-0667"></a>

**REQ-0667.** The runner must support the declared language and must verify the
artifact digest before activation. A runner-language or artifact mismatch fails
before specification data is read.

### Logical contracts

<a id="req-0668"></a>

**REQ-0668.** `functions` is a non-empty mapping from logical function names to
contracts. Each name has one contract and one binding in an environment.
A call contains that logical `name` and an exact `contract_version`. No call
contains a runtime-specific callable name.

<a id="req-0669"></a>

**REQ-0669.** A contract declares:

- a `contract_version` identifying its language-neutral behavior;
- a separate `implementation_version` identifying this project's binding,
  defaulting to the environment `version` when omitted;
- one closed ordered `params` list;
- one [Types and conversion](../values/types.md) `returns` type;
- whether an invoked binding `may_return_missing`;
- `comparison_decimals`, defaulting to four; and
- one conformance-vector path.

<a id="req-0670"></a>

**REQ-0670.** Changing meaning, parameter order or names, parameter types,
requiredness, defaults, missing behavior, return semantics, or effective
comparison precision requires a new `contract_version`. Changing only project
code without changing the logical contract changes `implementation_version` and
the runtime artifact identity instead.

<a id="req-0671"></a>

**REQ-0671.** For comparison across projects, implementations calculate a
contract fingerprint. Its payload is the following logical object, called
`yamaa-r018-contract-v1`:

```text
format, name, contract_version, params, returns,
may_return_missing, comparison_decimals
```

<a id="req-0672"></a>

**REQ-0672.** `params` is an array in declared order. Each entry has `name`,
`type`, the effective `required` and `accepts_missing` Booleans, and `default`.
An absent default is `{present: false}`. A present default is `{present: true,
value: typed-value}`. A typed value is encoded as follows:

| Logical value | Canonical object |
|---|---|
| missing | `{type: "missing"}` |
| `str` | `{type: "str", value: text}` |
| `int` | `{type: "int", value: base-10-string}` |
| finite `float` | `{type: "float", value: binary64}` |
| `bool` | `{type: "bool", value: JSON-Boolean}` |
| `date` or `datetime` | `{type: type-name, value: canonical-temporal-text}` |

<a id="req-0673"></a>

**REQ-0673.** [Types and conversion](../values/types.md) normalizes non-finite values before encoding a default or
other typed value. This table has no non-finite representation. A binary64
value is 16 lowercase hexadecimal big-endian bits.

<a id="req-0674"></a>

**REQ-0674.** `comparison_decimals` is its non-negative base-10 string. The
object is UTF-8 JSON under the JSON Canonicalization Scheme in RFC
8785, then hashed with SHA-256 and prefixed with `sha256:`. Strings use their
[Text values](../values/text.md) value. Parameters retain their declared order. Object member order
comes only from canonical JSON.

<a id="req-0675"></a>

**REQ-0675.** The runtime language, artifact, binding, description, and
implementation version are excluded. Repository validation requires every
discovered logical-name and contract-version pair to have the same calculated
fingerprint. Projects do not claim the same logical function contract unless
these fingerprints are identical.

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

**REQ-0683.** `binding.call` is a statically written fully qualified callable in
the selected runtime: an R package-qualified name such as `projectbmi::bmi`, or
a Python module-qualified name such as `orgstats.normal_cdf`. The environment
also maps every logical parameter name to one unique host argument name. An
omitted `binding.args` maps each logical parameter to the same-named host
argument. The mapping must exactly cover the logical signature. A Python host name is an
ASCII identifier and not a Python keyword. An R host name is an unquoted
syntactic R name and not a reserved word, `...`, or a `..n` positional name.

<a id="req-0684"></a>

**REQ-0684.** Inline code, anonymous functions, evaluation, shell commands,
script paths, computed callable names, executable argument transforms, and
lookup outside the verified artifact are invalid.

<a id="req-0685"></a>

**REQ-0685.** After applying environment defaults and missing short-circuiting,
the runner maps the logical arguments and invokes the callable once for one
logical row. It supplies no undeclared data or execution context. The binding
returns one scalar of the declared exact type. Batch or vector execution is
allowed only when every observable value and failure matches
independent calls in logical row order.

<a id="req-0686"></a>

**REQ-0686.** An invoked binding may return missing only when
`may_return_missing` is true. It may not return a vector, collection, table,
object wrapper, or value of a different type. [Execution lifecycle](../execution/lifecycle.md) conversion is not a repair
mechanism for an invalid function result. [Types and conversion](../values/types.md)'s non-finite normalization runs
immediately after the host scalar is returned and before these result checks.
The normalized missing result therefore still requires `may_return_missing:
true`.

### Activation conformance

<a id="req-0687"></a>

**REQ-0687.** Every logical contract names a language-neutral YAML conformance
document. It identifies the same logical name and contract version and contains
uniquely named cases. A case supplies `covers`, logical arguments, and one
expected scalar result. Its arguments obey the same signature, exact-type,
default, and missing rules as a specification call.

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

**REQ-0690.** A project claiming the same contract in another language runs the
same vector content.

<a id="req-0691"></a>

**REQ-0691.** Activation loads the verified artifact and runs all vectors before
any specification may execute. Success may be cached only for the exact
combination of environment version, artifact digest, contract fingerprints,
every implementation version, and the complete vector-content identity. Any
change invalidates the cache and requires activation again.

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

**REQ-1080.** The `environment_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `environment_class.schema_version` | Schema bundle this environment is written against. |
| `environment_class.version` | Version of the complete project-environment content. |
| `environment_class.runtime` | One language and immutable artifact used by every binding. |
| `environment_class.functions` | Logical contracts and singular bindings available to specifications. |

<a id="req-1081"></a>

**REQ-1081.** The `project_runtime_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `project_runtime_class.language` | Sole implementation language selected by the project. |
| `project_runtime_class.artifact` | Immutable runtime containing project code and dependencies. |

<a id="req-1082"></a>

**REQ-1082.** The `runtime_artifact_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `runtime_artifact_class.reference` | Organization-resolvable name of the immutable runtime artifact. |
| `runtime_artifact_class.digest` | Content identity verified before environment activation. |

<a id="req-1083"></a>

**REQ-1083.** The `function_contract_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `function_contract_class.contract_version` | Exact version of the language-neutral logical contract. |
| `function_contract_class.implementation_version` | Version of the selected language implementation; defaults to the environment `version` when omitted. |
| `function_contract_class.description` | Human-readable statement of what the function computes. |
| `function_contract_class.comparison_decimals` | Decimal places used only for cross-project numeric comparison. |
| `function_contract_class.may_return_missing` | Whether an invoked binding may deliberately return missing. |
| `function_contract_class.params` | Closed ordered logical signature. |
| `function_contract_class.returns` | Scalar [Types and conversion](../values/types.md) type returned before column conversion. |
| `function_contract_class.binding` | Sole callable binding in the project runtime language. |
| `function_contract_class.conformance` | Language-neutral vectors required for environment activation. |

<a id="req-1084"></a>

**REQ-1084.** The `function_binding_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `function_binding_class.call` | Statically written callable inside the immutable runtime. |
| `function_binding_class.args` | Complete mapping from logical to host argument names; omitted means each logical parameter maps to the same-named host argument. |

<a id="req-1085"></a>

**REQ-1085.** The `expressions.function` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.function.name` | Function name resolved in the project environment. |
| `expressions.function.contract_version` | Exact logical contract version required from the environment. |
| `expressions.function.args` | Named arguments passed to the function. |
| `Result` | Calls a logical function once per row when an implementation is supplied. Vectorization is valid only when equivalent to logical row-wise calls. [Project functions](functions.md) defines environment resolution, validation, and invocation. |

<a id="req-1086"></a>

**REQ-1086.** The `function_arg` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `function_arg` | Named variable or scalar literal leaf; arbitrary nesting is not allowed. |

<a id="req-1087"></a>

**REQ-1087.** The `function_contract_version` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `function_contract_version` | Exact logical contract version required by a function call. |

## Error conditions

<a id="req-0338"></a>

**REQ-0338.** A project function that violates its environment, contract,
binding, or result requirements: fail under [Project functions](functions.md).

<a id="req-0694"></a>

**REQ-0694.** No usable `environment.yaml` when implementation validation,
activation, or execution is requested at the selected root:
`project_environment_missing`.

<a id="req-0695"></a>

**REQ-0695.** An invalid environment, contract,
binding, vector document, or duplicate logical declaration:
`project_environment_invalid`.

<a id="req-0696"></a>

**REQ-0696.** Unsupported or mismatched runner
language: `runner_language_mismatch`.

<a id="req-0697"></a>

**REQ-0697.** Missing or mismatched
immutable artifact: `runtime_artifact_mismatch`.

<a id="req-0698"></a>

**REQ-0698.** A call naming no
declared logical function: `unknown_project_function`.

<a id="req-0699"></a>

**REQ-0699.** A call with
exact contract version is unavailable: `function_contract_mismatch`.

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

**REQ-0704.** Each failure identifies the logical function, contract version,
implementation version when available, and original host context when a binding
was invoked.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [negative-function-contract-mismatch](../../../benchmark/negative-function-contract-mismatch/README.md).

The [execution manifest](../../../benchmark/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Resolve immutable runtimes and validate function inputs, results, and activation conformance. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
