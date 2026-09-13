# Reject mixed clock readings in an event start

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-datetime-zone-offset.html)

**Goal:** carry each adverse event (AE) term into `AETERM` and its
start moment into `ASTDTM`.

**Input:** collected adverse event records carrying term (`AETERM`)
and start text (`AESTDTC`).

**Variables:**

- `ASTDTM` would contain the start moment copied from `AESTDTC`.
  One value reads without a clock offset and another reads with
  one, and the two are not the same kind of value, so the run
  stops while converting values and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Decide whether the study collects an offset at all. If the site clock is what
was recorded, correct the collected value so every start reads the same way:

```
PILOT7,P7-971,2,HEADACHE,2025-03-04T09:00:00
```

If the offset is real data, collect it as a field of its own and keep
`AESTDTC` to the site clock. `ASTDTM` then holds the moment and a second text
column holds the offset, where a later analysis can read it.

To see the collected value rather than fail on it, declare `ASTDTM` as text.
It keeps the collected characters and still orders chronologically, and a
column that converts it later can answer for the failure:

```yaml
- name: ASTDTM
  type: str
  derivation:
    source: AE.AESTDTC
```
