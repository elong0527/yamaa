---
id: R007
title: Operation Registry
status: normative
applies_to: [derivation.operations, derivation.filter, operations.yaml]
depends_on: [R004, R006]
---

# Operation registry

## Intent

Give every operation a fixed kind, argument signature, and result so that R and
Python implementations dispatch the same vocabulary. R004 defines the pipeline;
this rule defines what may appear in it.

## Registry document

`operations.yaml` is the registry. It uses the notation defined by R006, with
two additions.

The root is a mapping containing `version`, `operations`, and an optional
`argument_classes`. Each entry under `operations` maps an operation name to a
descriptor. Each entry under `argument_classes` declares a class usable in an
argument type expression.

An operation descriptor supports these keywords:

- `kind` is required and is one of `scalar`, `window`, `aggregate`.
- `seed` is required and is `required` or `ignored`.
- `result` is required and names the result type, or `same` when the result
  takes the type of the pipeline value, or `any` when it takes the type of its
  arguments.
- `semantics` is required and states, in prose, what the operation computes.
  This is the one part of a registry entry that no notation can carry, so it
  lives in the registry rather than in this rule.
- `arguments` is an R006 class listing the named arguments. Omitting it declares
  an operation with no arguments.
- `raises` is an optional list of exception names from R008 that the operation
  can signal.

This rule defines notation and general dispatch. It lists no individual
operation, exactly as R006 defines the notation of `schema.yaml` without listing
its classes. Adding an operation is a one-file change.

The type name `argument` in a registry descriptor means any value permitted by
`action_argument` in `schema.yaml`. The type name `sql` means the `sql`
primitive defined by `schema.yaml` and governed by R004. The type name
`dataset_id` means a string that must name a dataset declared in root
`datasets`, resolved under R002.

`action_argument` cannot itself distinguish a predicate from an ordinary
string, because both are YAML strings. Adding `sql` to that union would not
help, since every `sql` value also matches `str`. The registry signature is
therefore the only authority on which arguments are predicates, and
implementations must consult it rather than inferring from the value.

`schema.yaml` gives operations, exceptions, and verifications the same
`action_class` shape: one registered name and a mapping of named arguments. This
rule registers the operation vocabulary and R008 registers the exception
vocabulary. Verifications have no registry yet.

## Operation kinds

`kind` describes how an operation treats rows, and `seed` describes whether it
consumes the pipeline value. The two are independent, and conflating them is a
mistake: `row_number` partitions rows yet takes every input by name, while
`multiply` is row-wise yet consumes the pipeline.

`scalar` returns one value per row.

`window` partitions the constructed output rows by `group_by` and returns one
value per row, preserving row count. Omitting `group_by` places every row in one
partition.

`aggregate` reduces many records to one value. It is valid in exactly two
positions:

1. First in the pipeline of a derivation whose `source` is a qualified
   reference to a dataset other than the row driver. The reduction applies to
   the right side of the R003 join, partitioned by the applicable keys, before
   the join is evaluated. See R003.
2. With `group_by`, reducing output rows within each partition and broadcasting
   the result back to every row in that partition.

An `aggregate` in any other position is an error. This disambiguation is what
allows first-exposure and last-exposure derivations to be written without an
explicit join statement.

`seed: required` means the operation consumes the preceding pipeline value, so
an operation in first position requires a `source` or `literal`. `seed: ignored`
means the operation constructs its result entirely from its named arguments;
this is the "producer" of R004, and such an operation may open a pipeline with
no seed.

## The escape hatch

Registering every operation a real study needs is not practical, so the registry
includes `call`, which invokes a host-language function by name.

`call` is the single exception to everything above. Its behavior is not
described by the registry, its result type is whatever the function returns, and
two implementations agree only insofar as their function libraries agree. A
specification that uses it is portable exactly as far as that library is.

It is still bound by the rules that keep the model analyzable. Arguments are
named, so a call reads like any other operation. It runs once per row and
returns one value, so it cannot change row count. It must be a pure function of
its arguments, so R001 can place it in the dependency graph from its
`{source: VARIABLE}` arguments alone.

An operation that recurs across studies should graduate out of `call` into a
registered entry. `call` is for the long tail, not for avoiding the registry.

## Dispatch

Implementations must dispatch only registered names. For each operation:

- every required argument is present;
- no unknown argument is present;
- each argument value matches its declared type;
- arguments are named and unique, and their order has no meaning.

An operation declaring `seed: required` that is first in a pipeline requires a
`source` or `literal`. Later operations consume the result of the preceding
operation, and an operation declaring `seed: ignored` discards it.

## Result types

`result` describes the value leaving the operation. R005 converts only after the
last operation in a derivation, so intermediate values carry the registry result
type and no other conversion occurs between operations.

The mapping from registry result names to the closed column type vocabulary is
still governed by R005 and remains unresolved there.

## Registered semantics

Each operation's behavior is the `semantics` field of its entry in
`operations.yaml`. Those statements are normative. Behavior stated nowhere is
unresolved and must not be inferred.

Argument vocabularies are closed by `values` constraints in the registry rather
than by prose here. Collation for `min`, `max`, and `row_number` over strings
follows R004 and remains unresolved there, because it spans operations rather
than belonging to any one entry.

## Errors

- An unregistered operation name: fail.
- A registry entry with no `semantics`: fail.
- A missing, unknown, duplicate, or positional argument: fail.
- An argument value that does not match its declared type or `values`: fail.
- An argument that violates a constraint stated in the entry's `semantics`:
  fail.
- An `aggregate` operation outside the two positions listed above: fail.
- A `seed: required` operation first in a pipeline with no `source` or
  `literal`: fail.
- An operation that changes row count during column derivation: fail.
