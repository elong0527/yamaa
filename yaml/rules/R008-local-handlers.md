---
id: R008
title: Local Error Handlers
status: normative
applies_to: [source.missing, source.multiple_matches, expression, derivation]
depends_on: [R001, R002, R003, R005, R006, R007, R011, R012, R016]
---

# Local error handlers

## Intent

Attach expected data-defect handling to the expression or result stage that can
encounter it. Handlers are not conditional mapping; use `case` for that.

There is no standalone handler registry. Closed expression and derivation
schemas determine which handlers are legal.

## Boundaries

This rule owns the handler lifecycle: which stage each handler belongs to, when
it fires, what it may substitute, and what must be reported. Which handler
fields an operation offers is declared by its registry entry under R007.

## Evaluation order

Handlers occur in this fixed lifecycle:

| Stage | Local declaration | Behavior |
|---|---|---|
| bind | `source.missing` | Use a literal for an absent source variable or ODM item |
| join | `source.multiple_matches` | Filter, then select one duplicate right-side match |
| mapping | `missing` | Use a literal for a missing mapping input |
| mapping | `unmapped` | Use a literal for a non-missing value with no mapping |
| cut | `missing` | Use a literal for a missing numeric input |
| extract | `missing` | Use a literal for a missing string input |
| extract | `no_match` | Use a literal when a non-missing string does not match |
| template | `missing` | Use a literal when any placeholder value is missing |
| impute | `date_impute.missing`, `date_precision.missing` | Use a literal for a missing source, as R016 defines |
| impute | `date_impute.invalid`, `date_precision.invalid` | Use a literal for an unusable source, as R016 defines |
| convert | `conversion_failure` | Use a literal after failed output conversion |
| final | `override` | Apply the first matching final expression |

Literal handlers are substituted only when their condition occurs. Final
override values are the only handler values that remain nested expressions.
Omitting an applicable handler field makes its condition fatal.

## What `missing` means, by stage

`missing` names two related conditions, distinguished by where it is declared:

- On a `source` binding, it applies when the variable or ODM item **does not
  exist in context**. It does not apply when the variable exists and holds a
  missing value.
- On every other expression, it applies when the named **input value is
  missing**.

## Present but unusable

`unmapped`, `no_match`, and `invalid` fire only when every input is present: a
value with no dictionary entry, a string the pattern does not match, and a
source an operation cannot use are each a different defect from an uncollected
value, and a specification may answer them differently. Which values an
operation cannot use is its owning rule's to state; R016 states it for the two
operations on the `impute` stage.

One stage name serves several operations when their conditions coincide.
`date_impute` and `date_precision` read the same source and answer the same
two conditions about it, so both use `impute` in structured errors.

Where an operation takes several inputs, as `mapping_from` does, `missing`
fires when any one of them is missing and the present-but-unusable handler
fires only when all of them are present. The two conditions therefore stay
disjoint and an incomplete key can never reach the second.

## Source handlers

Under the `source` expression, the concise `source: DATASET.VARIABLE` form has
no handler. Use structured source binding when handling is required:

```yaml
source:
  variable: RAW.AGE
  missing: null
```

Other expressions type their `source` as a plain `variable` and declare their
own handler fields alongside it, so they take the concise form only.

`multiple_matches` relaxes R003 right-side uniqueness. Apply its optional
`filter` to the matching right-side records first, then sort the survivors by
its `order_by` terms and retain `first` or `last`. Remaining ties are resolved
by right-side record order.

Filtering to no surviving record is not a handled condition. It is an ordinary
absent match under R003 and yields missing, so a `filter` narrow enough to
empty the right side silently produces missing rather than firing this handler.
The handler count reports only the records where more than one match survived
the filter.

An aggregate declares no handler at all. A variable it names that does not
exist is R002's unresolved reference, a right side that reduces to no matching
record is R003's absent match, and a group whose records all hold missing
values is neither condition: R013 states what each reducer returns there.

## Result handlers

A derivation with conversion or final handling uses `value` to hold its normal
expression. A bare expression is the R006 shorthand for that wrapper, so every
derivation carries its expression in `value` once expanded.
`conversion_failure` supplies a literal replacement only when conversion to the
declared column type fails. R011 defines which conversions fail and states that
a missing input is not converted at all, so `conversion_failure` never fires
for one.

After successful conversion, evaluate `override` predicates in list order
against the converted output row. Evaluate the first matching `value`, convert
it to the column type, and stop. If no predicate is `TRUE`, retain the original
value.

## Dependencies and audit

Override values and predicates contribute dependencies under R001 even when
their path is not taken. Literal handlers add no dependencies.

Implementations must report, for each handler path, how many records used it.
A handler firing zero times is reportable and is not an error.

## Errors

- A handler field on an expression that does not register it: schema failure.
- A result wrapper with neither `conversion_failure` nor `override`: fail.
- A handler literal incompatible with its result context: fail with both the
  handler and original context.
- `multiple_matches.keep` outside `first` or `last`: schema failure.
- A conversion replacement that cannot be converted: fail.
- More than one successful override is not evaluated; first match wins.
