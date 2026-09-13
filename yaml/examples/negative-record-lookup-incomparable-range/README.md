# SDTM VS: reject an epoch range with incomparable endpoints

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-record-lookup-incomparable-range.html)

This example uses one vital-sign date and an epoch table expressed in integer
study days to attempt one output record:

- `VSDTC` is the collected calendar date;
- `EPOCH` is meant to be the period containing the corresponding study day.

A calendar date cannot be ordered directly against integer day bounds. The run
must fail rather than rely on implementation-specific coercion.

## How to fix

Derive the integer study day from the date and the subject's reference date,
then compare that value with the integer bounds:

```yaml
between: {value: VSDY, lower: DYLO, upper: DYHI}
```
