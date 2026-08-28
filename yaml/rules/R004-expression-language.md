---
id: R004
title: Predicate Language
status: draft
applies_to: [row.filter, sql]
depends_on: [R002, R006]
---

# Predicate language

## Intent

Define the portable Boolean predicate. Expression structure belongs to R006,
which closes the shape of a registry value, and to R007, which defines what each
registered keyword means.

## Predicates

The `sql` primitive is Boolean-valued and is used only by predicates:
`row.filter`, aggregate `filter`, `case.when`, `predicate.assert`,
`implies.when`, `implies.then`, and override predicates.

Supported core syntax includes `=`, `<>`, `<`, `<=`, `>`, `>=`, `IN`,
`BETWEEN`, `LIKE`, `IS NULL`, `IS NOT NULL`, `AND`, `OR`, `NOT`, and
parentheses. String literals use single quotes.

SQL three-valued logic applies. A filtering predicate retains a row only when
it is `TRUE`; `FALSE` and `UNKNOWN` remove it.

Numeric-valued expressions are a separate primitive. `compute.expr` is typed
`numeric_expression` and is governed by R010. The two share notation and
identifier resolution but not their type or their permitted vocabulary, and
neither grammar admits the other's constructs.

The complete predicate grammar, coercion, collation, and literal grammar remain
unresolved, so this rule remains draft. R010 resolves those questions for
numeric computation only; it does not settle string collation or the predicate
literal grammar.

## Errors

- Invalid predicate syntax: fail.
- An identifier in a predicate that does not resolve in its phase: fail under
  R001.
