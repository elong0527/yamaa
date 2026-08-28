---
id: R004
title: Expression Language
status: draft
applies_to: [row.filter, expression, sql]
depends_on: [R002, R006]
---

# Expression language

## Intent

Define portable predicates and self-contained derivations without
evaluating host-language code.

## Predicates

The `sql` primitive is Boolean-valued and is used only by predicates:
`row.filter`, aggregate `filter`, `case.when`, `predicate.assert`,
`implies.when`, `implies.then`, and override predicates.

Numeric-valued expressions are a separate primitive. `compute.expr` is typed
`numeric_expression` and is governed by R010, which closes its grammar,
function vocabulary, type promotion, and failure conditions. The two primitives
share notation and identifier resolution but not their type or their permitted
vocabulary, and neither grammar admits the other's constructs. Supported core syntax includes `=`, `<>`, `<`, `<=`, `>`, `>=`,
`IN`, `BETWEEN`, `LIKE`, `IS NULL`, `IS NOT NULL`, `AND`, `OR`, `NOT`, and
parentheses. String literals use single quotes.

SQL three-valued logic applies. A filtering predicate retains a row only when
it is `TRUE`; `FALSE` and `UNKNOWN` remove it.

## Expressions

An expression is a mapping containing exactly one keyword from the `expressions`
registry assembled by the schema bundle. The keyword determines the complete,
closed schema of its value.

`source` and `literal` are leaves. Other operations name every input variable.
Multi-step composition uses named derived columns:

```yaml
- name: DOUBLED
  type: float
  derivation:
    multiply:
      source: AVAL
      factor: 2
- name: ADJUSTED
  type: float
  derivation:
    add:
      source: DOUBLED
      addend: 1
```

There is no implicit current value, pipeline seed, or execution order derived
from YAML field order. R001 resolves the named dependencies.

Fields typed explicitly as `expression` retain nesting where it is intrinsic to
the construct: `case` branch results, function arguments, and final override
values. Registered operations must not evaluate host-language code themselves;
the `function` expression is the explicit project-environment extension point.

The complete predicate grammar, coercion, collation, and literal grammar remain
unresolved, so this rule remains draft. R010 resolves those questions for
numeric computation only; it does not settle string collation or the predicate
literal grammar.

## Errors

- An expression with zero or more than one keyword: fail.
- An unregistered expression keyword: fail.
- A missing, unknown, or invalid expression field: fail.
- Invalid predicate syntax: fail.
- An unresolved variable reference: fail.
