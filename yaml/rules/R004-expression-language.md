---
id: R004
title: Expression Language
status: draft
applies_to: [row.filter, derivation.filter, derivation.operations]
depends_on: [R002]
---

# Expression language

## Intent

Define portable filters and controlled operation pipelines without evaluating
host-language code.

## Filters

Filters use the `sql` primitive. Supported core syntax includes `=`, `<>`, `<`,
`<=`, `>`, `>=`, `IN`, `BETWEEN`, `LIKE`, `IS NULL`, `IS NOT NULL`, `AND`,
`OR`, `NOT`, and parentheses. String literals use single quotes.

SQL three-valued logic applies. A row is retained only when the predicate is
`TRUE`; `FALSE` and `UNKNOWN` remove it. A list of filters is equivalent to
joining its predicates with `AND`.

## Derivation seed

A derivation may declare one `source` or one `literal`, but not both. That value
seeds its operation pipeline. A derivation must contain at least one of
`source`, `literal`, or `operations`.

If neither `source` nor `literal` is present, the first operation must be a
registered producer. A producer constructs its result entirely from named
arguments. Later operations consume the result of the preceding operation.

## Operations

`operations` accepts one `operation_class` or an ordered list of them. A single
operation is equivalent to a one-item list. Each operation is a mapping with
exactly one registered operation name and a mapping of named arguments:

```yaml
source: LB.LBSTRESN
operations:
  - multiply:
      factor: 0.0167
```

The operation registry defines each operation's kind, named signature, result
type, and how it consumes the preceding pipeline value. Arguments must be named
and unique; order has no meaning. An additional variable input is written
explicitly as `{source: VARIABLE}`. Plain strings are literal argument values,
not variable references.

Implementations must dispatch only registered operations and must not evaluate
host-language code. Unknown operations and unknown, missing, or positional
arguments are invalid. An operation used during column derivation must not
change row count.

String-oriented operation names and behavior should use this vocabulary where
applicable: https://rstudio.github.io/cheatsheets/html/strings.html

The complete filter grammar, coercion, collation, literal grammar, and operation
registry remain unresolved, so this rule remains draft.

## Errors

- Invalid filter or operation syntax: fail.
- An unresolved variable reference: fail.
- Both `source` and `literal`, or no seed and no operation: fail.
- A non-producer operation without a pipeline seed: fail.
- A positional, duplicate, missing, or unknown operation argument: fail.
- An unregistered operation: fail.
