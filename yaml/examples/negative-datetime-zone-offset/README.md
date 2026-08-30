# ADaM ADAE: reject an event start recorded against another clock

This example uses collected adverse events to attempt one record per event:

- `ASTDTM` is meant to be the moment each event started.

One start is a plain reading of a wall clock and the other carries an offset
from one. The two are not the same kind of value, and holding both in one
column would first need a rule saying which clock a result is read on. Shifting
the offset value to some other clock would move a collected time, and keeping
the offset beside it would leave two records that cannot be ordered against
each other, so the run must fail and no artifact is accepted. A study that
records an offset keeps it in a column of its own, where it stays readable.

## How to fix

Decide whether the study collects an offset at all. If the site clock is what
was recorded, correct the collected value so every start reads the same way:

```
PILOT7,P7-971,2,HEADACHE,2025-03-04T09:00:00
```

If the offset is real data, collect it as a field of its own and keep
`AESTDTC` to the site clock. `ASTDTM` then holds the moment and a second
`str` column holds the offset, where a later analysis can read it.

To see the malformed value rather than fail on it, declare `ASTDTM` as `str`.
It keeps the collected characters and still orders chronologically, and a
column that converts it later can answer for the failure:

```yaml
- name: ASTDTM
  type: str
  derivation:
    source: AE.AESTDTC
```
