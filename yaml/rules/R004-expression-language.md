---
id: R004
title: Expression Language
status: draft
applies_to: [row.filter, expression, sql]
depends_on: [R002, R006]
---

# Expression language

## Intent

Define portable predicates and recursively composed derivations without
evaluating host-language code.

## Predicates

`row.filter`, structured-source `filter`, `case.when`, verification predicates,
and override predicates use the `sql` primitive. Supported core syntax includes
`=`, `<>`, `<`, `<=`, `>`, `>=`, `IN`, `BETWEEN`, `LIKE`, `IS NULL`,
`IS NOT NULL`, `AND`, `OR`, `NOT`, and parentheses. String literals use single
quotes.

SQL three-valued logic applies. A filtering predicate retains a row only when
it is `TRUE`; `FALSE` and `UNKNOWN` remove it.

## Expressions

An expression is a mapping containing exactly one keyword from the `expressions`
registry assembled by the schema bundle. The keyword determines the complete,
closed schema of its value.

`source` and `literal` are leaves. Other expressions name every input. Nested
expressions provide composition:

```yaml
add:
  value:
    multiply:
      value: {source: AVAL}
      factor: 2
  addend: 1
```

There is no implicit current value, pipeline seed, or execution order derived
from YAML field order. Evaluation is recursive under R001.

Conditional derivation is the `case` expression. Implementations must dispatch
only registered expression keywords and must not evaluate R or Python code.

The complete SQL grammar, coercion, collation, and literal grammar remain
unresolved, so this rule remains draft.

## Errors

- An expression with zero or more than one keyword: fail.
- An unregistered expression keyword: fail.
- A missing, unknown, or invalid expression field: fail.
- Invalid predicate syntax: fail.
- An unresolved variable reference: fail.
