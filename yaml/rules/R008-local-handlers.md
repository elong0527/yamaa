---
id: R008
title: Local Error Handlers
status: normative
applies_to: [source.missing, source.multiple_matches, expression, derivation]
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
| bind | `source.missing`, aggregate `missing` | Use a literal for an absent source variable or ODM item |
| join | `source.multiple_matches` | Filter, then select one duplicate right-side match |
| mapping | `missing` | Use a literal for a missing mapping input |
| mapping | `unmapped` | Use a literal for a non-missing value with no mapping |
| cut | `missing` | Use a literal for a missing numeric input |
| extract | `missing` | Use a literal for a missing string input |
| extract | `no_match` | Use a literal when a non-missing string does not match |
| convert | `conversion_failure` | Use a literal after failed output conversion |
| final | `override` | Apply the first matching final expression |

Literal handlers are substituted only when their condition occurs. Final
override values are the only handler values that remain nested expressions.

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

`missing` applies when the variable or ODM item does not exist in context. It
does not apply when the variable exists and contains a missing value.

`multiple_matches` relaxes R003 right-side uniqueness. Apply its optional
`filter` to the matching right-side records first, then sort the survivors by
its `order_by` terms and retain `first` or `last`. Remaining ties are resolved
by right-side record order.

Filtering to no surviving record is not a handled condition. It is an ordinary
absent match under R003 and yields missing, so a `filter` narrow enough to
empty the right side silently produces missing rather than firing this
handler. The handler count reports only the records where more than one match
survived the filter.

## Expression handlers

`mapping` and `mapping_from` distinguish a missing source from a non-missing
source with no dictionary entry through `missing` and `unmapped`. When
`mapping_from` declares several sources, `missing` fires when any one of them is
missing, and `unmapped` fires only when every one is present and no record
matches, so the two conditions stay disjoint and neither is reachable by an
incomplete key. `cut` uses `missing`. `str_extract` distinguishes `missing` and
`no_match`. `min` and `max` use `missing` for an absent source variable; a right
side that reduces to no matching record is governed by R003, not by this
handler. Each field is a literal replacement. Omitting the applicable field
makes the condition fatal.

## Result handlers

A derivation with conversion or final handling uses `value` to hold its normal
expression. `conversion_failure` supplies a literal replacement only when
conversion to the declared column type fails.

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
