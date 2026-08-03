---
id: R008
title: Exception Registry
status: normative
applies_to: [derivation.exception, exceptions.yaml]
depends_on: [R002, R003, R005, R006, R007]
---

# Exception registry

## Intent

Give every exception a fixed name, argument signature, and the derivation stage
it binds to, so that a deviation from normal mapping is declared rather than
inferred.

An exception handles a defect in source data. It does not express conditional
mapping logic; that is the `case` operation in R007.

## Registry document

`exceptions.yaml` is the registry. It uses the notation defined by R006 and the
registry additions in R007.

The root is a mapping containing `version`, `exceptions`, and an optional
`argument_classes`. An exception descriptor supports these keywords:

- `stage` is required and is one of `bind`, `join`, `operation`, `convert`,
  `final`.
- `semantics` is required and states, in prose, what the exception does.
- `arguments` is an R006 class listing the named arguments.
- `relaxes` is an optional list of rule IDs this exception is permitted to
  relax.

Like R007, this rule defines notation and general behavior and lists no
individual exception. Adding an exception is a one-file change.

## Stages

A derivation is evaluated in a fixed stage order, and each exception binds to
exactly one stage:

| Stage | Point of evaluation | Governing rule |
|---|---|---|
| `bind` | resolving `source` against its dataset | R002 |
| `join` | the cross-dataset left join | R003 |
| `operation` | inside one operation of the pipeline | R004, R007 |
| `convert` | converting the result to the column type | R005 |
| `final` | after conversion, replacing the finished value | R005 |

Exceptions are evaluated in stage order, not in the order they are listed. A
derivation may declare at most one exception per stage. Listing two exceptions
that bind to the same stage is an error.

Because `derivation.exception` accepts a list of `action_class` entries, a
single derivation can handle a source item that was never collected and a
collected value that cannot be mapped, which are different defects requiring
different results.

## Operation-stage binding

An `operation` stage exception applies to the operation in the pipeline whose
registry `raises` list contains that exception name.

If more than one operation in the same pipeline raises it, the binding is
ambiguous and the specification is invalid. The author must split the derivation
so that each pipeline raises a given exception at most once. This restriction is
deliberate: no syntax currently addresses an operation by position, and a
positional reference would be fragile under edits.

## Relaxing normative rules

An exception may relax a normative requirement only when the registry lists that
rule in `relaxes`, and only where the relaxed rule itself points back to this
one. An exception whose `relaxes` is absent or empty may not override any
normative requirement, and no exception may suppress a schema validation error.

## Registered semantics

Each exception's behavior is the `semantics` field of its entry in
`exceptions.yaml`, and each `relaxes` entry names the rules it may override.
Those statements are normative.

## Dependencies and audit

A variable named inside a `when` predicate is a dependency of the derivation.
See R001.

Implementations must be able to report, for each derivation and exception, how
many records took the exception path. An exception that fires zero times is
reportable and is not an error, because a correction may legitimately no longer
apply after a data refresh.

## Errors

- An unregistered exception name: fail.
- A registry entry with no `semantics`: fail.
- A missing, unknown, duplicate, or positional argument: fail.
- Two exceptions in one derivation binding to the same stage: fail.
- An `operation` stage exception whose name is raised by no operation in the
  pipeline, or by more than one: fail.
- An exception relaxing a rule not listed in its `relaxes`: fail.
- `multiple_matches` with a `keep` value other than `first` or `last`: fail.
