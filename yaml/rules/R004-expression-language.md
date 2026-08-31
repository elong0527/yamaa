---
id: R004
title: Predicate Language
status: draft
applies_to: [row.filter, sql]
depends_on: [R001, R002, R006]
---

# Predicate language

## Intent

Define the portable Boolean predicate written in a field typed `sql`.

## Boundaries

This rule owns the Boolean primitive only. Expression structure is R006, what
each registered keyword means is R007, and the numeric-valued
`numeric_expression` primitive used by `compute` is R010. The two primitives
share notation and identifier resolution but not their type or their permitted
vocabulary, and neither grammar admits the other's constructs.

## Predicates

The `sql` primitive is Boolean-valued and is used only by predicates:
`row.filter`, aggregate `filter`, window `filter`, `case.branches[].when`,
`override[].when`, `predicate.assert`, `implies.when`, and `implies.then`.

Supported core syntax includes `=`, `<>`, `<`, `<=`, `>`, `>=`, `IN`,
`BETWEEN`, `LIKE`, `IS NULL`, `IS NOT NULL`, `AND`, `OR`, `NOT`, and
parentheses. String literals use single quotes.

SQL three-valued logic applies. A filtering predicate retains a row only when
it is `TRUE`; `FALSE` and `UNKNOWN` remove it.

An identifier resolves against the phase in which the predicate is evaluated,
as R001 defines.

For an ungrouped row template, `row.filter` evaluates before row derivation and
its identifiers name fields of that template's row driver. For a grouped row
template, it evaluates after every candidate value completes stages 1 through
4 of the R005 lifecycle; its identifiers are unqualified columns derived by
that row template. The two uses share this Boolean language but do not share an
evaluation point. Aggregate and window filters continue to select records
inside their owning expression and do not suppress a candidate row.

## Unresolved

The complete predicate grammar, coercion, collation, and literal grammar remain
unresolved, so this rule remains draft. R010 closes those questions for numeric
computation only; it settles neither string collation nor the predicate literal
grammar.

## Errors

- Invalid predicate syntax: fail.
- An identifier in a predicate that does not resolve in its phase: fail under
  R001.
