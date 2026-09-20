---
id: R008
title: Local Error Handlers
status: normative
applies_to: [source.missing, source.filter, source.multiple_matches,
  expression, derivation]

---

# Local error handlers

## Intent

Handle expected defects at the expression or result stage that encounters each
defect. Handlers are not conditional mapping. Use `case`. There is no
standalone handler registry. Closed expression and derivation schemas determine
which handlers are legal.

## Boundaries

This rule owns handler conditions, substitutions, selection, and reporting.
R005 owns the derivation lifecycle and the order of its result stages. Which
handler fields an operation offers is declared by its registry entry under
R007.

## Handler sites

**R008-1.** Each handler applies at the site below. These sites identify
conditions within expression evaluation or result handling; the ordered
derivation lifecycle is R005-20 through R005-27. Each handler uses a literal
unless its behavior says otherwise:

| Stage | Local declaration | Behavior |
|---|---|---|
| bind | `source.missing` | Absent source variable or ODM item |
| join | `source.multiple_matches` | Choose one `source.filter` result |
| mapping | `missing` | Missing mapping input |
| mapping | `unmapped` | Non-missing value with no mapping |
| cut | `missing` | Missing numeric input |
| extract | `missing` | Missing string input |
| extract | `no_match` | Non-missing string does not match |
| template | `missing` | Any placeholder value is missing |
| impute | `date_impute.missing`, `date_precision.missing` | See R016 |
| impute | `date_impute.invalid`, `date_precision.invalid` | See R016 |
| convert | `conversion_failure` | Failed output conversion |

**R008-2.** Handlers are substituted only when their condition occurs; every
handler value is a literal.

**R008-3.** Omitting an applicable handler field makes its condition
fatal.

## What `missing` means, by stage

**R008-4.** `missing` names two related conditions, distinguished by
where it is declared. On a `source` binding, it applies when the
variable or ODM item **does not exist in context**. It does not apply
when the variable exists and holds a missing value.

**R008-5.** On every other expression, `missing` applies when the named
**input value is missing**.

## Present but unusable

**R008-6.** `unmapped`, `no_match`, and `invalid` fire only when every
input is present.

**R008-7.** The owning rule states which values an operation cannot use.
R016 states them for the two operations on the `impute` stage.

**R008-8.** One stage name serves several operations when their
conditions coincide. `date_impute` and `date_precision` read the same
source and answer the same two conditions about it, so both use
`impute` in structured errors.

**R008-9.** Where an operation takes several inputs, as `lookup`
does, `missing` fires when any input is missing. The
present-but-unusable handler fires only when all inputs are present.

## Source handlers

**R008-10.** Under the `source` expression, the concise `source:
DATASET.VARIABLE` form has no handler. Use structured source binding
when handling is required:

```yaml
source:
  variable: RAW.AGE
  missing: null
```

**R008-11.** Other expressions type their `source` as a variable or a
variable with a `filter`, and declare their own handler fields alongside
it. The binding handlers are not theirs to declare: a source they name
reaches its records through R003 and answers to its own handlers once
it holds a value.

**R008-12.** `multiple_matches` relaxes right-side uniqueness wherever one
source reaches several records: R003's matched records, an ODM item's
contextual matches, and the records a key combination was derived from.
Disagreement among those records is otherwise fatal under R001-44.

**R008-13.** The source's optional `filter` selects the eligible
right-side records first. Sort those survivors by the `order_by` terms and
retain `first` or `last`. Remaining ties are resolved by right-side record
order.

**R008-14.** An empty filtered result is not a handled condition. The
result is an absent match under R003 and yields missing.

**R008-15.** The handler count reports only records where more than
one match survived the filter.

**R008-16.** An aggregate declares no handler at all. A variable it
names that does not exist is R002's unresolved reference, a right side
that reduces to no matching record is R003's absent match, and a group
whose records all hold missing values is neither condition. R013 states
what each reducer returns for such a group.

## Result handlers

**R008-17.** A derivation with conversion handling uses `value`
to hold its normal expression. A bare expression is the R006 shorthand
for that wrapper, so every derivation carries its expression in `value`
once expanded.

**R008-18.** `conversion_failure` supplies a literal replacement only
when conversion to the declared column type fails. Convert the replacement
to that same column type. R011 defines which
conversions fail and states that a missing input is not converted at
all, so `conversion_failure` never fires for one.

## Dependencies and audit

**R008-19.** Literal handlers add no dependencies.

**R008-20.** Implementations must report each handler path's record count.
A handler firing zero times is reportable and is not an error.

## Errors

**R008-21.** A handler field on an expression that does not register it:
schema failure.

**R008-22.** A result wrapper with no `conversion_failure`: fail.

**R008-23.** A handler literal incompatible with its result context:
fail with both the handler and original context.

**R008-24.** `multiple_matches.keep` outside `first` or `last`: schema
failure.

**R008-25.** A conversion replacement that cannot be converted: fail.

## Rationale

A value with no dictionary entry, a string the pattern does not match,
and a source an operation cannot use are each a different defect from
an uncollected value. A specification may answer each defect
differently, so the present-but-unusable handlers fire only when every
input is present. With several inputs the two conditions stay disjoint,
so an incomplete key never reaches the second handler. Filtering to
no surviving record is an ordinary absent match rather than a handled
condition, so a narrow filter silently produces missing instead of
firing the handler.
