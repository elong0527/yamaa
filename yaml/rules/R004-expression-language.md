---
id: R004
title: Expression Language
status: draft
applies_to: [row.filter, derivation.filter, derivation.function]
depends_on: [R002]
---

# Expression language

## Intent

Define portable filters and function calls without evaluating host-language
code.

## Filters

Filters use the `sql` primitive. Supported core syntax includes `=`, `<>`, `<`,
`<=`, `>`, `>=`, `IN`, `BETWEEN`, `LIKE`, `IS NULL`, `IS NOT NULL`, `AND`,
`OR`, `NOT`, and parentheses. String literals use single quotes.

SQL three-valued logic applies. A row is retained only when the predicate is
`TRUE`; `FALSE` and `UNKNOWN` remove it. A list of filters is equivalent to
joining its predicates with `AND`.

## Function calls

`function` is a quoted expression using this grammar:

```text
function_call := name "(" [named_argument ("," named_argument)*] ")"
named_argument := name "=" value
value          := variable | literal | list | function_call
```

Arguments must be named and unique; their order has no meaning. Unquoted values
inside the expression are variable references, while string literals use
single quotes. Implementations must parse expressions into a syntax tree and
dispatch registered functions rather than use host-language `eval`.

The complete filter grammar, coercion, collation, literal grammar, and function
registry remain unresolved, so this rule remains draft.

## Errors

- Invalid filter or function syntax: fail.
- An unresolved variable reference: fail.
- A positional, duplicate, missing, or unknown function argument: fail.
- An unknown function: fail.
