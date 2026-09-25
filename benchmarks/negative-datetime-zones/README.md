# Reject Mixed Zones

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-datetime-zones.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each adverse event (AE) term into `AETERM` and its
start moment into `ASTDTM`.

**Input:** collected adverse event records carrying term (`AETERM`)
and start text (`AESTDTC`).

**Variables:**

- `ASTDTM` would contain the start moment copied from `AESTDTC`, a
  site-clock reading with no clock offset. A start that carries an
  offset, such as `+02:00`, is not the same kind of value, so the
  run stops while converting values and no artifact is accepted.

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

To inspect the collected characters without failing, carry them in an
intermediate text column that is not `ASTDTM`:

```yaml
- name: AESTDTC_RAW
  type: str
  label: Collected Start Text
  derivation: AE.AESTDTC
```

The text keeps the collected characters but does not order
chronologically across offsets, and it is not an analysis variable:
`ASTDTM` stays a moment.
