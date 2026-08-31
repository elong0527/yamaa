# SDTM VS: reject an epoch range with incomparable endpoints

This example uses one vital-sign date and an epoch table expressed in integer
study days to attempt one output record:

- `ADT` is the collected calendar date;
- `EPOCH` is meant to be the period containing the corresponding study day.

A calendar date cannot be ordered directly against integer day bounds. The run
must fail rather than rely on implementation-specific coercion.

## How to fix

Derive the integer study day from the date and the subject's reference date,
then compare that value with the integer bounds:

```yaml
between: {value: VSDY, lower: DYLO, upper: DYHI}
```
