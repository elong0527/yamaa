---
id: R018
title: Project Function Environment
status: normative
applies_to: [environment, function, function_contract, function_binding]
depends_on: [R001, R005, R006, R011, R016, R019, R020]
---

# Project function environment

## Intent

Make a project-supplied scalar function reproducible and reviewable without
placing host-language code or runtime selection in a derivation specification.
One logical contract may have an R implementation in one project and a Python
implementation in another, while each project remains a single-language
execution environment.

## Boundaries

This rule owns project-root resolution, logical function contracts, singular
runtime bindings, invocation behavior, and activation conformance. R006 owns
the schema notation and structural validation. R001 owns evaluation order and
the scalar row-count invariant. R005 owns what happens to the completed result,
and R011 owns column types and conversion. R016 owns temporal values.

Across-row reduction remains an `aggregate` operation. A project function is
never a reducer and cannot inspect a relation or other rows. Final artifact
formatting is also outside this rule. In particular, comparison precision used
for conformance does not round a derivation value.

Bindings are trusted organization code run by an authorized user in an
organization-controlled secure environment. This rule provides correctness,
reproducibility, and traceability requirements; it does not claim to create a
portable security sandbox.

## Project resolution

A portable specification may declare a logical `function` call before project
code is implemented. Specification authoring and structural validation do not
select a project root and do not require an `environment.yaml`. They validate
the call's closed schema shape, including its logical name, exact requested
contract version, and permitted argument leaves. Contract existence, signature,
argument types, missing permissions, and return type are deferred until an
implementation environment is supplied. Omission does not create or maintain
an environment implicitly.

When actual project code is validated, activated, or executed, the runner
receives one explicitly selected project root. It resolves exactly
`environment.yaml` at that root before reading specification data. A
specification cannot name, replace, extend, or override that environment.

An environment is validated independently against
`schema_environment.yaml`. Its `schema_version` selects that schema bundle and
its separate `version` identifies the complete environment content. Once an
implementation stage is requested for a specification containing a `function`
expression, missing, unreadable, structurally invalid, or ambiguous environment
resolution fails before code activation or execution.

## One immutable runtime

`runtime.language` is exactly `r` or `python`. It applies to every function in
the project. An environment cannot contain language-specific sub-environments,
parallel R and Python bindings, or a per-function language choice.

`runtime.artifact.reference` names one organization-resolvable runtime artifact
and `runtime.artifact.digest` supplies its verified SHA-256 content identity.
That identity covers callable project code and all transitive dependencies.
Only code inside that artifact participates in function resolution. A global
library, process search path, working directory, user profile, or ambient
package installation is not a fallback.

The runner must support the declared language and must verify the artifact
digest before activation. A runner-language or artifact mismatch fails before
specification data is read.

## Logical contracts

`functions` is a non-empty mapping from logical function names to contracts.
Each name has exactly one contract and one binding in an environment. A call
contains that logical `name` and an exact `contract_version`; it never contains
a runtime-specific callable name.

A contract declares:

- a `contract_version` identifying its language-neutral behavior;
- a separate `implementation_version` identifying this project's binding;
- one closed ordered `params` list;
- one R011 `returns` type;
- whether an invoked binding `may_return_missing`;
- `comparison_decimals`, defaulting to four; and
- one conformance-vector path.

Changing meaning, parameter order or names, parameter types, requiredness,
defaults, missing behavior, return semantics, or effective comparison precision
requires a new `contract_version`. Changing only project code without changing
the logical contract changes `implementation_version` and the runtime artifact
identity instead.

For comparison across projects, implementations calculate a contract
fingerprint. The payload is the following logical object, identified by
`yamaa-r018-contract-v1`:

```text
format, name, contract_version, params, returns,
may_return_missing, comparison_decimals
```

`params` is an array in declared order. Each entry contains `name`, `type`, the
effective `required` and `accepts_missing` Booleans, and `default`. An absent
default is `{present: false}`. A present default is
`{present: true, value: typed-value}`. A typed value is encoded as follows:

| Logical value | Canonical object |
|---|---|
| missing | `{type: "missing"}` |
| `str` | `{type: "str", value: R019-text}` |
| `int` | `{type: "int", value: base-10-string}` |
| finite `float` | `{type: "float", value: 16-lowercase-hex-big-endian-binary64-bits}` |
| `bool` | `{type: "bool", value: JSON-Boolean}` |
| `date` or `datetime` | `{type: type-name, value: R016-canonical-text}` |

R011's non-finite normalization runs before a default or other typed value is
encoded, so this table has no non-finite representation.

`comparison_decimals` is its non-negative base-10 string. The object is
serialized as UTF-8 JSON under the JSON Canonicalization Scheme in RFC 8785,
then hashed with SHA-256 and prefixed with `sha256:`. Strings retain their R019
value. Parameters remain in their declared array order; object member order
comes only from canonical JSON.

The runtime language, artifact, binding, description, and implementation
version are excluded. Repository validation requires every discovered pair of
logical name and contract version to have the same calculated fingerprint.
Two projects do not claim the same logical function contract unless these
fingerprints are identical.

## Parameters and arguments

Signatures are closed and named. Every parameter name is unique and a binding
has no positional, variadic, or arbitrary keyword parameter bag. `required`
defaults to `true`. Every optional parameter declares an environment `default`;
a required parameter cannot declare one.

Parameter types are the R011 column vocabulary plus function-only `bool`.
Return types are the R011 column vocabulary and do not include `bool`, so this
extension introduces neither Boolean columns nor Boolean derivation results.

Every argument and default exactly matches its declared type. There is no
implicit conversion, including no `int`-to-`float` widening. R011 conversion
can run only after the function has returned under the R005 lifecycle.
Argument names, requiredness, and exact types are contract-dependent checks at
the implementation stage; structural validation before then checks only the
closed argument-leaf forms below.

A call argument is one of:

- a named variable, written as a plain string;
- an `int`, `float`, `bool`, or missing YAML scalar;
- a string literal written as `{literal: text}`;
- a date literal written as `{date: YYYY-MM-DD}`; or
- a datetime literal written as `{datetime: YYYY-MM-DDThh:mm[:ss]}`.

The temporal text must be an R016 value. No argument may contain another
expression. A calculation needed by a function is first declared as an
internal column and then passed by name, preserving its visible dependency.

Omitting an optional argument selects its environment default. Explicitly
passing missing never selects the default. `accepts_missing` defaults to
`false` for each parameter:

- if any supplied value is missing for a non-accepting parameter, the call is
  not invoked and its result is missing; and
- a missing value for an accepting parameter is passed to the binding as the
  host runtime's canonical missing scalar.

A short-circuit result is not a result returned by the binding and therefore
does not require `may_return_missing: true`.

## Binding and invocation

`binding.call` is a statically written fully qualified callable in the selected
runtime: an R package-qualified name such as `projectbmi::bmi`, or a Python
module-qualified name such as `orgstats.normal_cdf`. The environment also maps
every logical parameter name to one unique host argument name. The mapping must
cover the logical signature exactly. A Python host name is an ASCII Python
identifier and not a Python keyword. An R host name is an unquoted syntactic R
name and not a reserved word, `...`, or a `..n` positional name.

Inline code, anonymous functions, evaluation, shell commands, script paths,
computed callable names, executable argument transforms, and lookup outside
the verified artifact are invalid.

After applying environment defaults and missing short-circuiting, the runner
maps the logical arguments and invokes the callable once for one logical row.
It supplies no undeclared data or execution context. The binding returns one
scalar of the declared exact type. Batch or vector execution is an
implementation optimization only when every observable value and failure is
equivalent to independent calls in logical row order.

An invoked binding may return missing only when `may_return_missing` is true.
It may not return a vector, collection, table, object wrapper, or value of a
different type. R005 conversion is not a repair mechanism for an invalid
function result. R011's non-finite normalization runs immediately after the
host scalar is returned and before these result checks. The normalized missing
result therefore still requires `may_return_missing: true`.

## Activation conformance

Every logical contract names a language-neutral YAML conformance document. It
identifies the same logical name and contract version and contains uniquely
named cases. A case supplies `covers`, logical arguments, and one expected
scalar result. Its arguments obey the same signature, exact-type, default, and
missing rules as a specification call.

`covers` is a non-empty list of unique obligations demonstrated by that case:

| Tag | Required evidence |
|---|---|
| `normal` | A contract-defined ordinary case |
| `boundary` | A contract-defined boundary case |
| `default:name` | The named optional parameter is omitted |
| `accepted-missing:name` | The named accepting parameter is explicitly missing |
| `short-circuit-missing:name` | The named non-accepting parameter is missing and the result is missing |
| `nullable-output` | An invoked nullable binding returns missing |
| `boolean-true:name`, `boolean-false:name` | The named Boolean parameter is supplied with that value |
| `numeric-comparison` | A non-missing `float` result exercises decimal comparison |

Every contract covers `normal` and `boundary`. Every optional parameter covers
its default, every parameter covers its applicable missing behavior, and both
values of every Boolean parameter are covered. A nullable contract covers
`nullable-output`, and a float-returning contract covers
`numeric-comparison`. Static validation checks inferable evidence in each
tagged case and rejects any missing obligation. The contract author identifies
which input is its semantic boundary; activation checks the declared result.

A project claiming the same contract in another language runs the same vector
content.

Activation loads the verified artifact and runs all vectors before any
specification may execute. Success may be cached only for the exact combination
of environment version, artifact digest, every contract fingerprint, every
implementation version, and the complete vector-content identity. Any change
invalidates the cache and requires activation again.

## Numeric conformance and rounding

Nonnumeric expected results and missingness compare by their type's equality,
including R019 for strings. Numeric results compare temporary decimal copies
at the contract's non-negative
`comparison_decimals`; the default is four. At an exact decimal tie the copy is
rounded to the nearest value away from zero. For example, at four places,
`1.23445` becomes `1.2345` and `-1.23445` becomes `-1.2345`.

The comparator never replaces or mutates the runtime result. Calculations use
the unrounded result, and rounding for final display occurs once under R020's
`output.decimals`. Predicate outcomes, keys, row membership and order,
conditions, and final displayed artifacts must still agree exactly across
projects; `comparison_decimals` is not a tolerance for structural differences.

## Errors

- No usable `environment.yaml` when implementation validation, activation, or
  execution is requested at the selected root:
  `project_environment_missing`.
- An invalid environment, contract, binding, vector document, or duplicate
  logical declaration: `project_environment_invalid`.
- Unsupported or mismatched runner language: `runner_language_mismatch`.
- Missing or mismatched immutable artifact: `runtime_artifact_mismatch`.
- A call naming no declared logical function: `unknown_project_function`.
- A call whose exact contract version is unavailable:
  `function_contract_mismatch`.
- An unknown, missing required, or incorrectly typed argument or default:
  `invalid_function_argument`.
- A host exception or enforced resource failure: `function_call_failed`. It is
  fatal and has no R008 local fallback.
- A wrong type or shape, or an undeclared missing value returned by an invoked
  binding: `invalid_function_result`. It is fatal and is not converted.
- A failed activation vector or numeric comparison:
  `function_conformance_failed`.

Each failure identifies the logical function, contract version, implementation
version when available, and original host context when a binding was invoked.
