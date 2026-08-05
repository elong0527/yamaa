---
id: R008
title: Local Error Handlers
status: normative
applies_to: [source.missing, source.multiple_matches, expression.unmapped, derivation]
depends_on: [R002, R003, R005, R006, R007]
---

# Local error handlers

## Intent

Attach expected data-defect handling to the expression or result stage that can
encounter it. Handlers are not conditional mapping; use `case` for that.

There is no standalone handler registry. Closed expression and derivation
schemas determine which handlers are legal.

## Evaluation order

Handlers occur in this fixed lifecycle:

| Stage | Local declaration | Behavior |
|---|---|---|
| bind | `source.missing` | Replace an absent source variable or ODM item |
| join | `source.multiple_matches` | Select one duplicate right-side match |
| expression | `unmapped` | Replace a mapping, cut, or extraction failure |
| convert | `conversion_failure` | Replace a failed output conversion |
| final | `override` | Apply the first matching final correction |

Nested handler expressions are evaluated only if their handler is taken.

## Source handlers

The concise `source: DATASET.VARIABLE` form has no handler. Use structured
source binding when handling is required:

```yaml
source:
  variable: RAW.AGE
  missing:
    literal: null
```

`missing` applies when the variable or ODM item does not exist in context. It
does not apply when the variable exists and contains a missing value.

`multiple_matches` relaxes R003 right-side uniqueness. Sort matches ascending by
its `order_by` expression evaluated on each matching right-side record, then
retain `first` or `last`. Remaining ties are resolved by right-side record
order.

## Expression handlers

`mapping`, `mapping_from`, `cut`, and `str_extract` may declare `unmapped`.
When the expression cannot produce a normal result for its input, evaluate that
nested expression instead. Omitting `unmapped` makes the condition fatal.

## Result handlers

A derivation with conversion or final handling uses `value` to hold its normal
expression. `conversion_failure` supplies a replacement only when conversion
to the declared column type fails.

After successful conversion, evaluate `override` predicates in list order
against the converted output row. Evaluate the first matching `value`, convert
it to the column type, and stop. If no predicate is `TRUE`, retain the original
value.

## Dependencies and audit

Every handler expression and override predicate contributes dependencies under
R001 even when its path is not taken.

Implementations must report, for each handler path, how many records used it.
A handler firing zero times is reportable and is not an error.

## Errors

- A handler field on an expression that does not register it: schema failure.
- A result wrapper with neither `conversion_failure` nor `override`: fail.
- A handler expression that fails: fail with both handler and original context.
- `multiple_matches.keep` outside `first` or `last`: schema failure.
- A conversion replacement that cannot be converted: fail.
- More than one successful override is not evaluated; first match wins.
