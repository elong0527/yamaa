# ADaM ADPC: reject elapsed seconds from a date alone

This example uses collected pharmacokinetic samples to attempt one analysis row
per specimen:

- `ELTM` is meant to hold elapsed whole seconds from the reference reading to
  the sample reading.

A calendar date supplies no time of day, so subtracting it from a local moment
cannot produce elapsed seconds. Choosing midnight silently would invent a
sample time, and the run must fail before reading data.

## How to fix

First combine the collected date with a valid collected time, then measure the
two complete readings:

```yaml
- name: ADTM
  type: datetime
  derivation:
    to_datetime: {date: PCDT, time: PCTM}
- name: ELTM
  type: int
  derivation:
    datetime_diff: {start: REFDATM, end: ADTM}
```
