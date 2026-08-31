---
id: R017
title: Portable Function Registry
status: normative
applies_to: [portable_registry, root.portable_extensions]
depends_on: [R006]
---

# Portable function registry

## Intent

Give scalar functions and reducers one versioned, machine-readable contract so
implementations discover and validate the same vocabulary without admitting
host-language code into a portable specification.

## Boundaries

This rule owns registry loading, metadata, names, versions, extensions, and
call-validation errors. R010 owns scalar numeric evaluation and R013 owns
reduction, grain, and reducer evaluation. R007 owns where the expressions that
contain calls may be used. The project-environment `function` expression is
outside this registry.

## One typed registry

The schema entry point names one core portable registry through
`portable_registry`. Scalar functions and reducers share that registry and are
distinguished by `evaluation_kind`. A shared registry gives name, signature,
type, failure, determinism, and version metadata one shape while R010 and R013
retain their different grammars and evaluation phases.

The registry is YAML 1.2 data, never a schema module and never part of a study
specification. It must reject duplicate keys, aliases, merge keys, explicit
tags, and unknown fields. It contains declarative contracts only. A field must
not contain R, Python, SQL, or another executable host-language expression.

The core file contains exactly:

- `registry_version`, a semantic version with major, minor, and patch parts;
- `namespace: core`;
- `specification_versions`, the schema versions that may use it; and
- a non-empty `entries` list.

The registry version changes when its contract changes. A compatible addition
increments the minor part, a correction that does not change a contract
increments the patch part, and a breaking change increments the major part.
The schema bundle pins the file, so two implementations reading the same
bundle cannot silently select different registry versions.

## Version 1.0 migration

Core registry version 1.0.0 transcribes every scalar name previously listed in
R010 and every reducer previously listed in R013, with the same canonical name
and behavior. It adds `NORMAL_CDF` without changing an existing call. The
`portable_extensions` root field is optional, so every specification valid
before this registry remains structurally valid. Implementations migrate by
loading the pinned metadata instead of retaining a private name or signature
table; specifications require no rewrite.

## Entry contract

Every entry declares exactly these fields:

- `canonical_name`, an uppercase ASCII name unique in its namespace;
- `aliases`, uppercase alternate spellings, each unique in its namespace;
- `evaluation_kind`, either `scalar` or `reducer`;
- `signature`, containing ordered named `parameters`, `min_arity`, and a
  numeric or null `max_arity`;
- `type_promotion` and `result_type`;
- `missing_values`;
- `failures`, containing `domain`, `overflow`, and `non_finite_result`;
- `determinism` and `accuracy`;
- `availability`, containing `since` and a nullable `deprecated`; and
- `definition`, a non-empty declarative description of the result.

Each parameter has a lowercase ASCII `name`, a non-empty `types` list, and may
set `variadic: true`. Only the final parameter may be variadic. A fixed
signature has one parameter per argument and equal minimum and maximum arity.
A variadic signature repeats its final parameter and has a null maximum.
Parameter names define the contract and diagnostics; the R010 and R013 call
grammars remain positional.

Accepted concrete types are `str`, `int`, `float`, `bool`, `date`, and
`datetime`. `record_star` is permitted only for the R013 qualified-star form.
The literal `NULL` has the bottom type `null`, which is compatible with every
parameter and does not participate in promotion. An argument whose static type
is otherwise absent is not inferred from its stored value.

`type_promotion` has this closed vocabulary:

- `preserve_numeric` retains the one numeric input type;
- `promote_numeric` returns `int` only when every numeric input is `int` and
  returns `float` otherwise;
- `always_float` returns `float`;
- `count` returns `int`; and
- `preserve_input` retains one non-numeric or numeric input type.

`result_type` independently states the result available to a caller. Its closed
vocabulary is `promoted_numeric`, `input_numeric`, `float`, `int`, and `input`.
The promotion and result fields must agree; keeping both lets a validator
answer the result type without reconstructing the promotion algorithm.

`missing_values` has this closed vocabulary:

- `propagate` returns missing when any argument is missing;
- `ignore_missing_all_missing` ignores missing arguments or values and returns
  missing when none remain;
- `first_non_missing` returns the first argument that is not missing;
- `null_if_equal` returns missing when its first argument is missing or both
  non-missing arguments are equal, and otherwise returns its first argument;
  and
- `count_non_missing_or_records` counts non-missing values for an ordinary
  argument and records for `record_star`.

`determinism` is `binary64`, `exact_or_binary64`, or `order_independent`.
`order_independent` requires the same result for every permutation of the input
records.
`accuracy.mode` is `exact`, `binary64`, `exact_or_binary64`, or
`absolute_or_relative`. `exact` requires equality. Every other mode accepts a
result whose absolute error is within the absolute tolerance or whose scaled
relative error is within the relative tolerance.

Every behavior field is required even when it states that no exceptional case
exists. This prevents a loader from inheriting a runtime default. Accuracy
declares its comparison mode and absolute and relative tolerances. Exact
integer or lexical results use zero tolerances. A floating result is conformant
when it satisfies the entry's declared mode against the shared fixture.

`failures.domain` is a unique list of stable snake-case domain conditions, or
an empty list when no finite input in the signature has a domain failure.
`overflow` and `non_finite_result` are both `fail`; no registry entry can turn
either condition into a value.

`availability.since` and `availability.deprecated` name specification
versions. Deprecation does not change evaluation. A validator may warn about a
deprecated call, but must continue to accept it until a later registry removes
it. Removal requires a new schema or registry compatibility plan; an entry is
never silently removed from a file already pinned by a bundle.

## Names and aliases

Core calls are unqualified. Function and reducer names are case-insensitive,
then resolve to the uppercase canonical name. Documentation and newly written
specifications use the canonical name.

An alias resolves to exactly one canonical entry in the same namespace and has
the canonical entry's complete contract. Canonical names and aliases share one
collision set. `AVG` is therefore not accepted merely because a runtime treats
it as an alias of `MEAN`; it would have to be declared explicitly.

## Portable extension packs

Optional portable extension packs are allowed. They use the same file and
entry shape as core, but declare a lowercase namespace other than `core`.
Their calls have the form `namespace::NAME`. An extension must never add an
unqualified name or alias.

A specification using an extension declares it once in
`portable_extensions`, with its exact `namespace` and `registry_version`. The
implementation loads that pack from global configuration, verifies that the
pack lists the specification's schema version, and verifies the exact registry
version before validating any call. Pack availability and code installation
are implementation configuration, not paths or executable content in the
specification.

Two implementations supporting different packs still behave portably: both
accept a specification only when they have every pack and exact version it
declares, and both report `unavailable_extension` otherwise. Portability is
therefore relative to the explicitly declared conformance set, never to an
implementation's undeclared library search path.

Namespaces are unique among loaded packs. A canonical name or alias is unique
within its namespace. Core names cannot collide with extensions because an
extension call is always qualified; two packs cannot claim the same namespace.

## Core, extension, and project boundaries

Every conforming implementation supports every core entry. Core promotion
requires all of the following evidence:

1. two independent real specifications need the operation, or one real
   specification and one published cross-project standard or implementation
   pattern demonstrate the same need;
2. existing portable operations cannot state it without retaining a
   project-specific call or distorting the data model;
3. its types, missing values, failures, determinism, and cross-runtime accuracy
   can be closed declaratively; and
4. shared fixture contracts pass the R and Python CI validators; runtime
   evaluation must pass the same fixtures when an implementation lands.

An extension pack is for a reusable but non-core domain whose compatibility
can still be closed by the same test. A sponsor algorithm, external service,
environment-dependent lookup, stateful routine, or operation whose contract
cannot be closed remains behind `function`. Repetition alone does not turn
project code into a portable function.

## Shared conformance fixtures

`registry/conformance.yaml` is the common fixture source. Each core entry must
have at least one evaluation case. The same file fixes central and tail values
for probability functions and validation cases for name, kind, arity, type,
namespace, extension, and version handling.

The R and Python CI checks validate the same registry metadata, call contracts,
fixture shape, and coverage. They deliberately do not implement function or
reducer evaluation. A future runtime may dispatch to native code only after
registry validation and must evaluate these fixtures using each entry's
accuracy contract. A fixture contains values and expected results, not
executable expressions.

Documentation tables are generated from the registry. Schema files reference
the registry and do not enumerate its names. Normative behavior therefore has
one machine-readable source rather than a prose table and a schema list that
can drift.

## Validation errors

Validation reports a stable `condition` and the implicated call:

- `unknown_function` when an unqualified name or a name inside an available
  namespace is neither canonical nor an alias;
- `wrong_evaluation_kind` when a scalar is used as a reducer or the reverse;
- `wrong_arity` when the argument count is outside the signature;
- `incompatible_type` when an argument's static type is not accepted at its
  position;
- `unavailable_function` when the entry is not available to the specification
  version;
- `unavailable_extension` when a namespaced call has no matching declaration,
  loaded pack, compatible specification version, or exact pack version;
- `name_collision` for a duplicate canonical name or alias in one namespace;
  and
- `namespace_collision` when two loaded packs declare one namespace.

Malformed registry metadata fails before a specification is validated and
reports the registry path and field path. A loader must not skip an invalid
entry or fall back to a host-language function of the same name.
